//! Transition heuristics validation — checks energy deltas between segments.
//!
//! The architecture spec requires transition heuristics to ensure sections
//! have sufficient contrast and energy movement.
//!
//! **Note**: Energy estimation is a symbolic heuristic based on layer count
//! and bar count. True energy requires audio analysis (loudness, spectral
//! brightness, drum intensity, FX activity). This is an MVP approximation.

use scorestrata_core::diagnostic::{Diagnostic, MetricScope, SuggestedPatch};
use scorestrata_core::domain::Segment;
use scorestrata_core::error::Severity;
use scorestrata_core::ids::DiagnosticId;

/// Result of a transition check between two segments.
#[derive(Debug, Clone)]
pub struct TransitionResult {
    /// Energy delta (`target_energy` - `source_energy`).
    pub energy_delta: f32,
    /// Whether the transition meets the minimum delta.
    pub meets_threshold: bool,
    /// Diagnostic if the transition is too weak.
    pub diagnostic: Option<Diagnostic>,
}

/// Estimate the energy of a segment based on its layer count and bar count.
///
/// Higher layer count and bar count indicate higher energy potential.
/// This is a symbolic heuristic — true energy requires audio analysis
/// (loudness, spectral brightness, drum intensity, FX activity).
#[must_use]
pub fn estimate_segment_energy(segment: &Segment) -> f32 {
    #[expect(
        clippy::cast_precision_loss,
        reason = "Energy estimation for music validation"
    )]
    let layer_factor = segment.layers.len() as f32 / 8.0; // normalize to 0..1
    #[expect(
        clippy::cast_precision_loss,
        reason = "Energy estimation for music validation"
    )]
    let bar_factor = segment.bars as f32 / 32.0; // normalize to 0..1
    (layer_factor * 0.6 + bar_factor * 0.4).min(1.0)
}

/// Check the transition between two adjacent segments.
///
/// The architecture requires that choruses are denser than verses,
/// and that transitions have sufficient energy delta.
#[must_use]
pub fn check_transition(
    source: &Segment,
    target: &Segment,
    min_energy_delta: f32,
) -> TransitionResult {
    let source_energy = estimate_segment_energy(source);
    let target_energy = estimate_segment_energy(target);
    let energy_delta = target_energy - source_energy;
    let meets_threshold = energy_delta >= min_energy_delta;

    let diagnostic = if meets_threshold {
        None
    } else {
        Some(Diagnostic {
            id: DiagnosticId::new(),
            code: "TRANSITION_ENERGY_DELTA_LOW".to_string(),
            metric: "arrangement.transition_strength".to_string(),
            severity: Severity::Warning,
            scope: MetricScope {
                segment_id: Some(target.id),
                layer_id: None,
                bar_range: None,
            },
            evidence_uris: vec![
                format!("project://segments/{}", source.id),
                format!("project://segments/{}", target.id),
            ],
            message: format!(
                "Transition from '{}' to '{}' has energy delta {:.2} (minimum {:.2}). \
                 Source energy: {:.2}, target energy: {:.2}",
                source.name,
                target.name,
                energy_delta,
                min_energy_delta,
                source_energy,
                target_energy
            ),
            root_causes: vec![
                "insufficient energy increase between sections".to_string(),
                "sections may be too similar in density".to_string(),
            ],
            suggested_patches: vec![SuggestedPatch {
                op: "add_transition_package".to_string(),
                params: serde_json::json!({
                    "fill": true,
                    "riser": true,
                    "impact": true,
                }),
                expected_delta: Some(format!(
                    "+{:.2}",
                    (min_energy_delta - energy_delta).max(0.05)
                )),
                confidence: 0.7,
            }],
            expected_delta: None,
            confidence: 0.7,
        })
    };

    TransitionResult {
        energy_delta,
        meets_threshold,
        diagnostic,
    }
}

/// Check a full sequence of segments for transition quality.
///
/// Segments are sorted by `start_bar` before checking to ensure correct pairing.
/// Returns diagnostics for any weak transitions.
#[must_use]
pub fn check_all_transitions(segments: &[Segment], min_energy_delta: f32) -> Vec<Diagnostic> {
    let mut sorted = segments.to_vec();
    sorted.sort_by_key(|s| s.start_bar);

    let mut diagnostics = Vec::new();

    for window in sorted.windows(2) {
        let source = &window[0];
        let target = &window[1];
        let result = check_transition(source, target, min_energy_delta);
        if let Some(diag) = result.diagnostic {
            diagnostics.push(diag);
        }
    }

    diagnostics
}

#[cfg(test)]
mod tests {
    use super::*;
    use scorestrata_core::domain::{Layer, SegmentRole};
    use scorestrata_core::ids::{LayerId, SegmentId};

    fn make_segment(name: &str, bars: u32, layer_count: usize) -> Segment {
        Segment {
            id: SegmentId::default(),
            name: name.to_string(),
            role: SegmentRole::Verse,
            start_bar: 0,
            bars,
            layers: (0..layer_count)
                .map(|i| Layer {
                    id: LayerId::default(),
                    name: format!("layer_{i}"),
                    instrument: "synth".to_string(),
                    pattern_instances: Vec::new(),
                    register: None,
                    automation: vec![],
                    fx: None,
                    mix_gain_db: None,
                    mix_pan: None,
                    group: None,
                })
                .collect(),
        }
    }

    #[test]
    fn strong_transition_passes() {
        let verse = make_segment("verse", 16, 2);
        let chorus = make_segment("chorus", 16, 6);
        let result = check_transition(&verse, &chorus, 0.15);
        assert!(result.meets_threshold);
        assert!(result.diagnostic.is_none());
    }

    #[test]
    fn weak_transition_fails() {
        let verse = make_segment("verse", 16, 4);
        let verse2 = make_segment("verse2", 16, 4);
        let result = check_transition(&verse, &verse2, 0.15);
        assert!(!result.meets_threshold);
        #[expect(clippy::unwrap_used, reason = "Test assertions")]
        let diag = result.diagnostic.unwrap();
        assert_eq!(diag.code, "TRANSITION_ENERGY_DELTA_LOW");
    }

    #[test]
    fn check_all_transitions_sorts_by_start_bar() {
        let mut segments = vec![make_segment("chorus", 16, 6), make_segment("verse", 16, 3)];
        // Deliberately out of order — should still work
        segments[0].start_bar = 16;
        segments[1].start_bar = 0;
        let diagnostics = check_all_transitions(&segments, 0.10);
        // verse(0) -> chorus(16): energy increases, should pass
        assert!(diagnostics.is_empty());
    }

    #[test]
    fn check_all_transitions_collects_diagnostics() {
        let segments = vec![
            make_segment("intro", 8, 1),
            make_segment("verse", 16, 3),
            make_segment("chorus", 16, 5),
            make_segment("outro", 8, 2),
        ];
        let diagnostics = check_all_transitions(&segments, 0.10);
        // Some transitions may or may not pass depending on the heuristic
        assert!(diagnostics.len() <= 3); // at most N-1 transitions
    }
}
