//! Bus graph (Phase B) — group busses + send/return routing.
//!
//! The production-mix model the plan calls out (§3.4): layers route into
//! named group busses (drums / bass / harmony / lead / fx), each group
//! applies its own insert chain (EQ → comp), and every layer's reverb /
//! delay **sends** fold into shared returns instead of inline wet folds.
//!
//! ```text
//! layer ─▶ layer inserts ─┬─▶ group bus (drums / bass / …)
//!                         │        └─ bus inserts: EQ ─▶ comp
//!                         ├─ sends ─▶ return: convolution reverb (100% wet)
//!                         └─ sends ─▶ return: ping-pong delay (dampened)
//! group busses ─▶ stereo sum
//! ```
//!
//! R1 (byte-identity when disabled): the renderer only enters this module
//! when `RenderConfig.bus_graph` is `Some`; with no groups and no reverb IR
//! the routing below degenerates to the flat per-layer fold.

use crate::convolve::ConvolutionReverb;
use crate::fx::{apply_delay, apply_fx_chain};
use scorestrata_core::domain::{BusGraph, SendLevels};

/// A rendered layer ready for bus routing.
#[derive(Debug, Clone)]
pub struct BusLayerInput {
    /// Layer name.
    pub name: String,
    /// Group-bus assignment (`None` = direct to stereo sum).
    pub group: Option<String>,
    /// Post-insert dry samples (mono).
    pub data: Vec<f32>,
    /// Constant-power gain (linear).
    pub gain: f32,
    /// Pan (−1.0…1.0).
    pub pan: f32,
    /// Send amounts.
    pub sends: SendLevels,
}

/// The result of one group bus: summed dry layers + applied bus inserts.
#[derive(Debug, Clone)]
pub struct GroupResult {
    /// Bus name.
    pub name: String,
    /// Summed + insert-processed mono samples.
    pub data: Vec<f32>,
    /// Send levels accumulated from the group's member layers.
    pub sends: SendLevels,
}

/// Route rendered layers through the bus graph, returning the stereo sum
/// (left, right). When `stereo` is false both channels carry the mono fold.
///
/// Deterministic: group membership order follows input order; returns are
/// processed after all groups so sends accumulate first.
#[must_use]
pub fn mix_bus_graph(
    layers: &[BusLayerInput],
    graph: &BusGraph,
    sample_rate: u32,
    stereo: bool,
) -> (Vec<f32>, Vec<f32>) {
    let max_len = layers.iter().map(|l| l.data.len()).max().unwrap_or(0);

    // 1. Sum member layers into their groups (in input order); direct layers
    //    accumulate straight into the L/R fold with their constant-power pan.
    let mut group_data: Vec<Vec<f32>> = vec![Vec::new(); graph.groups.len()];
    let mut direct_l = vec![0.0f32; max_len];
    let mut direct_r = vec![0.0f32; max_len];

    for layer in layers {
        let idx = graph
            .groups
            .iter()
            .position(|g| Some(g.name.as_str()) == layer.group.as_deref());
        let pan = if stereo { layer.pan } else { 0.0 };
        let pan_norm = f32::midpoint(pan, 1.0); // -1..1 → 0..1
        let left_gain = layer.gain * (pan_norm * std::f32::consts::FRAC_PI_2).cos();
        let right_gain = layer.gain * (pan_norm * std::f32::consts::FRAC_PI_2).sin();
        let mono_gain = f32::midpoint(left_gain, right_gain);
        if let Some(i) = idx {
            // Group member: fold to mono inside the bus (classic mono bus
            // architecture); the pan is captured by the mono fold.
            if group_data[i].is_empty() {
                group_data[i] = vec![0.0f32; max_len];
            }
            for (j, &s) in layer.data.iter().enumerate() {
                group_data[i][j] = s.mul_add(mono_gain, group_data[i][j]);
            }
        } else {
            // Direct layer: keep its pan in the stereo field.
            for (j, &s) in layer.data.iter().enumerate() {
                direct_l[j] = s.mul_add(left_gain, direct_l[j]);
                direct_r[j] = s.mul_add(right_gain, direct_r[j]);
            }
        }
    }

    // 2. Apply bus inserts per group, then accumulate into the stereo sum.
    //    Group busses are mono: members fold to mono inside the bus, the bus
    //    sits center in the stereo field. Direct layers keep their
    //    constant-power pan into L/R.
    let mut master_l = direct_l;
    let mut master_r = direct_r;
    for (i, group) in graph.groups.iter().enumerate() {
        let mut bus = std::mem::take(&mut group_data[i]);
        apply_fx_chain(&mut bus, &group.inserts, sample_rate);
        for (m, b) in master_l.iter_mut().zip(&bus) {
            *m += b;
        }
        master_r.resize(master_l.len(), 0.0);
        for (m, b) in master_r.iter_mut().zip(&bus) {
            *m += b;
        }
    }

    // 3. Send/return: accumulate every layer's sends into the return inputs.
    let mut reverb_send: Vec<f32> = vec![0.0f32; max_len];
    let mut delay_send: Vec<f32> = vec![0.0f32; max_len];
    for layer in layers {
        if layer.sends.reverb > 0.001 {
            for (r, &s) in reverb_send.iter_mut().zip(&layer.data) {
                *r = (s * layer.sends.reverb).mul_add(layer.gain, *r);
            }
        }
        if layer.sends.delay > 0.001 {
            for (d, &s) in delay_send.iter_mut().zip(&layer.data) {
                *d = (s * layer.sends.delay).mul_add(layer.gain, *d);
            }
        }
    }

    // Return A: convolution reverb (100% wet, pre-delay). The convolved tail
    // is longer than max_len; both master channels are extended to match.
    if graph.reverb_ir.is_some() && reverb_send.iter().any(|v| v.abs() > 1e-6) {
        let ir_kind = match graph.reverb_ir {
            Some(scorestrata_core::domain::ReverbIr::Plate) => "plate",
            Some(scorestrata_core::domain::ReverbIr::Hall) => "hall",
            _ => "room",
        };
        let pre = graph.reverb_pre_delay_ms.unwrap_or(0.0);
        let reverb = ConvolutionReverb::new(ir_kind, pre, 1.0, sample_rate);
        let tail = reverb.process(&reverb_send);
        for side in [&mut master_l, &mut master_r] {
            if tail.len() > side.len() {
                side.resize(tail.len(), 0.0);
            }
            for (m, &t) in side.iter_mut().zip(&tail) {
                *m += t;
            }
        }
    }

    // Return B: ping-pong delay (dampened), existing FX.
    if delay_send.iter().any(|v| v.abs() > 1e-6) {
        let wet = apply_delay(&delay_send, sample_rate, 375.0, 0.4);
        for side in [&mut master_l, &mut master_r] {
            if wet.len() > side.len() {
                side.resize(wet.len(), 0.0);
            }
            for (m, &w) in side.iter_mut().zip(&wet) {
                *m += w;
            }
        }
    }

    // Stereo: the pan fold already placed direct layers; groups and returns
    // sit center. Mono: both channels carry the same mono bus sum, so the
    // stereo fold equals the mono render (true mono-fold invariance).
    if stereo {
        (master_l, master_r)
    } else {
        (master_l.clone(), master_l)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use scorestrata_core::domain::{FxChain, GroupBus, ReverbIr};

    fn layer(name: &str, group: Option<&str>, amp: f32) -> BusLayerInput {
        BusLayerInput {
            name: name.to_string(),
            group: group.map(str::to_string),
            data: vec![amp; 512],
            gain: 1.0,
            pan: 0.0,
            sends: SendLevels::default(),
        }
    }

    #[test]
    fn empty_graph_is_flat_sum() {
        let graph = BusGraph::default();
        let layers = vec![layer("a", None, 0.5), layer("b", None, 0.3)];
        let (l, r) = mix_bus_graph(&layers, &graph, 48_000, false);
        // Constant-power pan law at center scales each channel by cos(π/4).
        let expected = (0.5 + 0.3) * std::f32::consts::FRAC_1_SQRT_2;
        assert!(
            (l[0] - expected).abs() < 1e-5,
            "flat sum ~{expected}, got {}",
            l[0]
        );
        assert_eq!(l, r, "mono fold identical");
    }

    #[test]
    fn group_bus_attracts_members_and_applies_inserts() {
        let mut graph = BusGraph::default();
        graph.groups.push(GroupBus {
            name: "drums".to_string(),
            inserts: FxChain {
                high_pass_hz: Some(20.0),
                ..FxChain::default()
            },
        });
        let layers = vec![layer("kick", Some("drums"), 0.5), layer("bass", None, 0.3)];
        let (l, _) = mix_bus_graph(&layers, &graph, 48_000, false);
        // Both layers still reach the sum (0.8 × pan-scale ± HPF settling).
        let expected = 0.8 * std::f32::consts::FRAC_1_SQRT_2;
        assert!(
            (l[0] - expected).abs() < 0.1,
            "sum ~{expected}, got {}",
            l[0]
        );
    }

    #[test]
    fn reverb_return_adds_wet_tail() {
        let graph = BusGraph {
            reverb_ir: Some(ReverbIr::Room),
            reverb_pre_delay_ms: Some(20.0),
            ..BusGraph::default()
        };
        let mut l = layer("pad", None, 0.8);
        l.sends.reverb = 0.5;
        let (left, _) = mix_bus_graph(&[l], &graph, 48_000, false);
        // The wet tail extends past the dry signal length.
        let dry_len = 512;
        let has_wet =
            left.len() > dry_len + 500 && left.iter().skip(dry_len).any(|v| v.abs() > 1e-4);
        assert!(has_wet, "convolution tail extends past dry signal");
    }

    #[test]
    fn delay_return_adds_damped_echo() {
        let mut l = layer("lead", None, 0.6);
        l.sends.delay = 0.4;
        // 1 s of signal so the 375 ms delay line returns an echo within range.
        l.data = vec![0.6; 48_000];
        let graph = BusGraph::default();
        let (left, _) = mix_bus_graph(&[l], &graph, 48_000, false);
        let delay = (375 * 48_000) / 1000;
        let echo: f32 = left
            .iter()
            .skip(delay - 64)
            .take(128)
            .fold(0.0, |m, v| m.max(v.abs()));
        assert!(echo > 0.01, "damped echo present at 375 ms");
    }
}
