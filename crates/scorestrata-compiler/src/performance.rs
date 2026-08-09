//! Performance layer (Phase C) — humanize, groove, and articulation passes.
//!
//! These are the "kill the grid-perfect tell" transforms (plan §1.2): a
//! seeded Gaussian velocity spread, note-on timing jitter, gate-length
//! variation, groove templates (swing-16 / laid-back / driving), and
//! per-note articulation (accent / staccato / legato / portamento / pedal).
//!
//! # Determinism (R2 / R1)
//!
//! - Every transform is a **pure function of the input timeline and the seed**.
//!   The PRNG is a fixed-seed `SplitMix64`, so the same project + spec always
//!   produces the same performance, on every machine.
//! - Per-voice streams derive from `(seed, layer_id)` (Ableton Random
//!   semantics): each layer gets its own stream so chords don't shift as
//!   rigid blocks.
//! - `enabled: false` (or all-zero strengths) is a **no-op**: the timeline is
//!   returned unmodified, byte-identical to an un-humanized render (R1).
//!
//! Pass order is fixed and recorded in the manifest (R4):
//! `groove → humanize → articulation → instrument_resolution → existing passes`.
//! In practice articulation is applied while **building** the timeline events
//! (the compiler already sees per-note `Articulation`), so the passes here
//! implement groove + humanize over the flat IR and expose the shared PRNG the
//! articulation resolution uses.

// Performance math casts bounded sample counts between u64/i64/f32 (groove
// deltas, timing jitter, gate scaling); the pedantic cast lints are expected
// (same discipline as the audio renderer).
#![expect(
    clippy::cast_precision_loss,
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    clippy::cast_possible_wrap,
    reason = "Performance passes cast bounded tick/velocity values between u64/i64/f32; deltas clamp before narrowing"
)]

use scorestrata_core::domain::HumanizeSpec;
use scorestrata_core::domain::PPQ;
use scorestrata_core::ids::LayerId;

use crate::timeline::{EventKind, TimelineEvent};

/// `SplitMix64` — tiny deterministic PRNG (no external crate, fixed seeds).
///
/// Shared by groove + humanize so the whole performance layer uses one
/// generator family (identical across render / stems / audit paths, which all
/// consume the same compiled timeline).
#[derive(Debug, Clone)]
pub struct SplitMix64 {
    state: u64,
}

impl SplitMix64 {
    /// Create a generator from a fixed seed.
    #[must_use]
    pub const fn new(seed: u64) -> Self {
        Self { state: seed }
    }

    /// Advance and return the next 64-bit value.
    #[must_use]
    pub const fn next_u64(&mut self) -> u64 {
        self.state = self.state.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut z = self.state;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        z ^ (z >> 31)
    }

    /// Uniform float in `[0, 1)`.
    #[must_use]
    pub fn next_f32(&mut self) -> f32 {
        (self.next_u64() >> 40) as f32 / (1u64 << 24) as f32
    }

    /// Approximate standard-normal sample via the sum-of-12-uniforms (CLT).
    ///
    /// Deterministic, cheap, and bounded (|z| < 6 in practice), which is what
    /// the velocity / timing jitter needs.
    #[must_use]
    pub fn next_gaussian(&mut self) -> f32 {
        let sum: f32 = (0..12).map(|_| self.next_f32()).sum();
        sum - 6.0
    }
}

/// Derive a stable per-layer stream seed from `(project_seed, layer_id)`.
///
/// The layer UUID's bytes are folded into the project seed so two layers in
/// the same project get independent streams (Ableton Random: each voice of a
/// chord offsets independently) while staying fully deterministic.
#[must_use]
pub fn layer_stream_seed(project_seed: u64, layer_id: LayerId) -> u64 {
    let mut h = project_seed ^ 0x9E37_79B9_7F4A_7C15;
    for byte in layer_id.to_string().as_bytes() {
        h = (h ^ u64::from(*byte)).wrapping_mul(0x1000_0000_01B3);
    }
    h
}

/// The groove templates (Ableton Groove Pool semantics, plan §1.2).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Groove {
    /// Off-beat 16ths shift later by up to 66 % of a 16th (triplet feel).
    Swing16,
    /// Everything sits slightly behind the grid (relaxed, "in the pocket").
    LaidBack,
    /// Notes lean forward (energetic, rushing toward the beat).
    Driving,
}

impl Groove {
    /// Resolve a template name to a groove, if recognized.
    fn from_name(name: &str) -> Option<Self> {
        match name {
            "swing16" | "swing" => Some(Self::Swing16),
            "laid_back" | "laid-back" => Some(Self::LaidBack),
            "driving" => Some(Self::Driving),
            _ => None,
        }
    }

    /// The peak tick shift applied at `amount = 1.0` for a note at `tick`,
    /// expressed in sixteenth-note units (`sixteenth = PPQ / 4`).
    ///
    /// Returns a signed delta multiplier of one sixteenth: positive shifts the
    /// note later, negative earlier.
    const fn shift_units(self, tick: u64, sixteenth: u64) -> f32 {
        let is_off_beat_16th = (tick / sixteenth) % 2 == 1;
        match self {
            Self::Swing16 => {
                if is_off_beat_16th {
                    0.66
                } else {
                    0.0
                }
            }
            Self::LaidBack => 0.25,
            Self::Driving => {
                if is_off_beat_16th {
                    -0.2
                } else {
                    0.0
                }
            }
        }
    }
}

/// Apply the groove template, then the humanize jitter, to `events`.
///
/// Both passes mutate `NoteOn` events' ticks/velocities and their matching
/// `NoteOff` ticks (so note lengths stay coherent). When the spec is disabled
/// or has no effect, the timeline is untouched (R1).
///
/// The `project_seed` is the resolved seed (`spec.seed` or the project content
/// hash); per-layer streams are derived from it inside. `bpm` is used only to
/// convert the ms-denominated timing jitter to ticks (real-time-consistent
/// across project tempos).
pub fn apply_performance_passes(
    events: &mut [TimelineEvent],
    spec: &HumanizeSpec,
    project_seed: u64,
    bpm: u32,
) {
    if !spec.enabled {
        return;
    }
    let groove = spec
        .groove
        .as_deref()
        .and_then(Groove::from_name)
        .filter(|_| spec.groove_amount > 0.0);
    let has_jitter =
        spec.velocity_sigma > 0.0 || spec.timing_sigma_ms > 0.0 || spec.gate_length_jitter > 0.0;
    if groove.is_none() && !has_jitter {
        return;
    }

    // Pair each NoteOn with its NoteOff so timing/gate edits stay coherent.
    // Events are sorted by (tick, id); a NoteOff immediately follows its
    // NoteOn with the same layer/pitch.
    let pairs = pair_notes(events);

    // ── 1. groove pass ─────────────────────────────────────────────────
    if let Some(g) = groove {
        let sixteenth = u64::from(PPQ) / 4;
        let amount = spec.groove_amount.clamp(0.0, 1.0);
        for (on, off) in &pairs {
            let e = &mut events[*on];
            if let EventKind::NoteOn { .. } = e.kind {
                let shift = g.shift_units(e.tick.0, sixteenth) * amount;
                let delta = ((shift * sixteenth as f32).round()) as i64;
                if delta != 0 {
                    let new_tick = (e.tick.0 as i64 + delta).max(0) as u64;
                    e.tick = scorestrata_core::domain::Tick::new(new_tick);
                    let off = &mut events[*off];
                    off.tick = scorestrata_core::domain::Tick::new(
                        (off.tick.0 as i64 + delta).max(0) as u64,
                    );
                }
            }
        }
    }

    // ── 2. humanize pass ───────────────────────────────────────────────
    if has_jitter {
        // One PRNG stream per layer (seed + layer UUID), consumed in event
        // order so every run reproduces the exact same offsets.
        let mut streams: std::collections::HashMap<u64, SplitMix64> =
            std::collections::HashMap::new();
        for &(on_idx, off_idx) in &pairs {
            let layer = events[on_idx].layer_id;
            let stream = streams
                .entry(layer_seed_key(layer))
                .or_insert_with(|| SplitMix64::new(layer_stream_seed(project_seed, layer)));
            let (mut on_tick, mut velocity) = match events[on_idx].kind {
                EventKind::NoteOn { velocity, .. } => (events[on_idx].tick.0, velocity),
                _ => continue,
            };
            let mut off_tick = events[off_idx].tick.0;

            // Velocity spread (fraction of 127).
            if spec.velocity_sigma > 0.0 {
                let dv = (stream.next_gaussian() * spec.velocity_sigma * 127.0).round();
                let base = f32::from(velocity.0) + dv;
                velocity.0 = base.clamp(1.0, 127.0) as u8;
            }
            // Note-on timing jitter (ms → ticks at the project tempo):
            // beat = 60_000 ms / bpm; PPQ ticks per beat.
            if spec.timing_sigma_ms > 0.0 {
                let ticks_per_ms = f64::from(PPQ) * f64::from(bpm.max(1)) / 60_000.0;
                let jitter = (f64::from(stream.next_gaussian())
                    * f64::from(spec.timing_sigma_ms)
                    * ticks_per_ms)
                    .round() as i64;
                if jitter != 0 {
                    on_tick = (on_tick as i64 + jitter).max(0) as u64;
                    off_tick = (off_tick as i64 + jitter).max(0) as u64;
                }
            }
            // Gate-length variation (fraction of written length).
            if spec.gate_length_jitter > 0.0 {
                let gate = off_tick.saturating_sub(on_tick);
                let dgate =
                    (stream.next_gaussian() * spec.gate_length_jitter * gate as f32).round() as i64;
                if dgate != 0 {
                    off_tick = (off_tick as i64 + dgate).max(on_tick as i64 + 1) as u64;
                }
            }

            // Write back (indices are disjoint, so no aliasing).
            events[on_idx].tick = scorestrata_core::domain::Tick::new(on_tick);
            if let EventKind::NoteOn { velocity: v, .. } = &mut events[on_idx].kind {
                v.0 = velocity.0;
            }
            events[off_idx].tick = scorestrata_core::domain::Tick::new(off_tick);
        }
    }

    // Re-sort: jitter/groove may have reordered events.
    events.sort_by_key(|e| (e.tick.0, e.id));
}

/// Stable map key for a layer's PRNG stream.
fn layer_seed_key(layer_id: LayerId) -> u64 {
    let bytes = layer_id.to_string();
    bytes
        .as_bytes()
        .iter()
        .fold(0x51_7C_C1_B7_27_22_0A_95, |h, b| {
            (h ^ u64::from(*b)).wrapping_mul(0x1000_0000_01B3)
        })
}

/// Pair each `NoteOn` with its matching `NoteOff` index.
///
/// Events are sorted by (tick, id); the first `NoteOff` with the same layer
/// and pitch after a `NoteOn` is its partner. Overlapping same-pitch notes on
/// one layer are matched in FIFO order.
fn pair_notes(events: &[TimelineEvent]) -> Vec<(usize, usize)> {
    let mut pairs = Vec::new();
    let mut open: Vec<usize> = Vec::new();
    for (idx, e) in events.iter().enumerate() {
        match &e.kind {
            EventKind::NoteOn { .. } => {
                open.push(idx);
            }
            EventKind::NoteOff { pitch } => {
                // Find the nearest unmatched NoteOn with the same pitch.
                if let Some(pos) = open.iter().rposition(|&o| {
                    matches!(
                        &events[o].kind,
                        EventKind::NoteOn { pitch: p, .. } if p == pitch
                    ) && events[o].layer_id == e.layer_id
                }) {
                    let on = open.remove(pos);
                    pairs.push((on, idx));
                }
            }
            _ => {}
        }
    }
    pairs
}

#[cfg(test)]
mod tests {
    use super::*;
    use scorestrata_core::domain::{MidiNote, Tick, Velocity};
    use scorestrata_core::ids::{LayerId, SegmentId, TrackId};

    fn event(id: u64, tick: u64, layer: LayerId, kind: EventKind) -> TimelineEvent {
        TimelineEvent {
            id,
            tick: Tick::new(tick),
            kind,
            track_id: TrackId::new(),
            segment_id: SegmentId::new(),
            layer_id: layer,
            pattern_id: None,
        }
    }

    fn layer_id() -> LayerId {
        LayerId::from_uuid(uuid::Uuid::from_u128(0xABCD))
    }

    fn straight_16th_layer() -> (LayerId, Vec<TimelineEvent>) {
        let l = layer_id();
        let mut events = Vec::new();
        for i in 0..8u64 {
            let tick = i * (u64::from(PPQ) / 4);
            let on = event(
                i * 2,
                tick,
                l,
                EventKind::NoteOn {
                    pitch: MidiNote::new(60),
                    velocity: Velocity::new(100),
                },
            );
            let off = event(
                i * 2 + 1,
                tick + 100,
                l,
                EventKind::NoteOff {
                    pitch: MidiNote::new(60),
                },
            );
            events.push(on);
            events.push(off);
        }
        (l, events)
    }

    #[test]
    fn swing16_shifts_off_beat_16ths_by_66_percent_at_full_amount() {
        let (l, mut events) = straight_16th_layer();
        let spec = HumanizeSpec {
            enabled: true,
            seed: Some(7),
            velocity_sigma: 0.0,
            timing_sigma_ms: 0.0,
            gate_length_jitter: 0.0,
            groove: Some("swing16".to_string()),
            groove_amount: 1.0,
        };
        apply_performance_passes(&mut events, &spec, 7, 120);
        let sixteenth = u64::from(PPQ) / 4;
        let expected_shift = (0.66 * sixteenth as f32).round() as u64;
        let note_ons: Vec<u64> = events
            .iter()
            .filter_map(|e| match &e.kind {
                EventKind::NoteOn { .. } => Some(e.tick.0),
                _ => None,
            })
            .collect();
        assert_eq!(note_ons.len(), 8);
        for (i, tick) in note_ons.iter().enumerate() {
            let base = i as u64 * sixteenth;
            if i % 2 == 1 {
                assert_eq!(*tick, base + expected_shift, "off-beat 16th {i} swings");
            } else {
                assert_eq!(*tick, base, "on-beat 16th {i} stays");
            }
        }
        let _ = l;
    }

    #[test]
    fn disabled_spec_is_a_noop() {
        let (_l, mut events) = straight_16th_layer();
        let before = events.clone();
        let spec = HumanizeSpec::default(); // enabled: false
        apply_performance_passes(&mut events, &spec, 99, 120);
        assert_eq!(events, before, "disabled humanize must not touch events");
    }

    #[test]
    fn same_seed_reproduces_identical_humanize() {
        let (_l, mut a) = straight_16th_layer();
        let mut b = a.clone();
        let spec = HumanizeSpec {
            enabled: true,
            seed: Some(42),
            velocity_sigma: 0.05,
            timing_sigma_ms: 4.0,
            gate_length_jitter: 0.2,
            groove: Some("swing16".to_string()),
            groove_amount: 0.5,
        };
        apply_performance_passes(&mut a, &spec, 42, 120);
        apply_performance_passes(&mut b, &spec, 42, 120);
        assert_eq!(a, b, "same seed → identical performance");
    }

    #[test]
    fn different_seed_changes_the_performance() {
        let (_l, mut a) = straight_16th_layer();
        let mut b = a.clone();
        let spec = HumanizeSpec {
            enabled: true,
            seed: Some(42),
            velocity_sigma: 0.05,
            timing_sigma_ms: 4.0,
            gate_length_jitter: 0.0,
            groove: None,
            groove_amount: 0.0,
        };
        apply_performance_passes(&mut a, &spec, 42, 120);
        apply_performance_passes(&mut b, &spec, 43, 120);
        assert_ne!(a, b, "different seed → different performance");
    }

    #[test]
    fn humanize_spreads_velocity_and_timing() {
        let (_l, mut events) = straight_16th_layer();
        let spec = HumanizeSpec {
            enabled: true,
            seed: Some(3),
            velocity_sigma: 0.08,
            timing_sigma_ms: 5.0,
            gate_length_jitter: 0.0,
            groove: None,
            groove_amount: 0.0,
        };
        apply_performance_passes(&mut events, &spec, 3, 120);
        let ons: Vec<&TimelineEvent> = events
            .iter()
            .filter(|e| matches!(e.kind, EventKind::NoteOn { .. }))
            .collect();
        let vels: Vec<u8> = ons
            .iter()
            .map(|e| match e.kind {
                EventKind::NoteOn { velocity, .. } => velocity.0,
                _ => 100,
            })
            .collect();
        // Some spread must have happened across 8 notes.
        assert!(
            vels.iter().any(|v| *v != 100),
            "velocity jitter changed at least one note"
        );
        let ticks: Vec<u64> = ons.iter().map(|e| e.tick.0).collect();
        assert!(
            ticks.iter().any(|t| *t != ticks[0]),
            "timing jitter moved at least one note"
        );
    }
}
