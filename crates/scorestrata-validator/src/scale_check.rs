//! Scale compliance validation — checks that notes are in the correct key.

use scorestrata_core::diagnostic::{Diagnostic, MetricScope, SuggestedPatch};
use scorestrata_core::domain::Pattern;
use scorestrata_core::error::Severity;
use scorestrata_core::ids::DiagnosticId;
use scorestrata_theory::key::Key;

/// Result of a scale compliance check.
#[derive(Debug, Clone)]
pub struct ScaleCheckResult {
    /// Total notes checked.
    pub total_notes: u32,
    /// Notes that are in scale.
    pub in_scale: u32,
    /// Notes that are out of scale.
    pub out_of_scale: u32,
    /// Compliance ratio (`in_scale` / `total_notes`).
    pub compliance_ratio: f32,
    /// Diagnostics for out-of-scale notes.
    pub diagnostics: Vec<Diagnostic>,
}

/// Check scale compliance for a pattern against a key.
#[must_use]
pub fn check_scale_compliance(
    pattern: &Pattern,
    key: &Key,
    segment_id: scorestrata_core::ids::SegmentId,
    layer_id: scorestrata_core::ids::LayerId,
) -> ScaleCheckResult {
    let mut total = 0u32;
    let mut in_scale = 0u32;
    let mut out_of_scale = 0u32;
    let mut diagnostics = Vec::new();

    for note in &pattern.notes {
        total += 1;
        if key.contains_note(&note.pitch) {
            in_scale += 1;
        } else {
            out_of_scale += 1;
            let (bar, beat, _) = note
                .start
                .to_bar_beat_tick(pattern.time_signature.numerator);
            diagnostics.push(Diagnostic {
                id: DiagnosticId::new(),
                code: "HARMONY_SCALE_VIOLATION".to_string(),
                metric: "harmony.scale_compliance".to_string(),
                severity: Severity::Warning,
                scope: MetricScope {
                    segment_id: Some(segment_id),
                    layer_id: Some(layer_id),
                    bar_range: Some((bar, bar + 1)),
                },
                evidence_uris: vec![format!(
                    "project://segments/{}/{}/notes?bar={}",
                    segment_id, layer_id, bar
                )],
                message: format!(
                    "{} at bar {} beat {} is not in {}",
                    note.pitch.name(),
                    bar,
                    beat,
                    key.name()
                ),
                root_causes: vec!["note outside scale".to_string()],
                suggested_patches: vec![SuggestedPatch {
                    op: "quantize_layer_to_scale".to_string(),
                    params: serde_json::json!({
                        "segment_id": segment_id,
                        "layer_id": layer_id,
                        "strength": 1.0,
                    }),
                    expected_delta: Some("+0.14".to_string()),
                    confidence: 0.9,
                }],
                expected_delta: None,
                confidence: 1.0,
            });
        }
    }

    let compliance_ratio = if total > 0 {
        #[expect(
            clippy::cast_precision_loss,
            reason = "Scale compliance ratio for music validation"
        )]
        let ratio = in_scale as f32 / total as f32;
        ratio
    } else {
        1.0
    };

    ScaleCheckResult {
        total_notes: total,
        in_scale,
        out_of_scale,
        compliance_ratio,
        diagnostics,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use scorestrata_core::domain::{Duration, MidiNote, Tick, Velocity};
    use scorestrata_core::ids::{LayerId, PatternId, SegmentId};
    use scorestrata_theory::key::{Key, Mode};

    fn c_minor() -> Key {
        Key::new(0, Mode::Minor)
    }

    fn make_pattern(notes: Vec<(u8, u64)>) -> Pattern {
        Pattern {
            id: PatternId::new(),
            name: "test".to_string(),
            bars: 1,
            time_signature: scorestrata_core::domain::TimeSignature::default(),
            notes: notes
                .into_iter()
                .map(|(pitch, tick)| scorestrata_core::domain::Note {
                    id: scorestrata_core::ids::NoteId::new(),
                    pitch: MidiNote::new(pitch),
                    start: Tick::new(tick),
                    duration: Duration::quarter(),
                    velocity: Velocity::new(100),
                    tied: false,
                    articulation: None,
                })
                .collect(),
            steps: Vec::new(),
        }
    }

    #[test]
    fn all_notes_in_scale_passes() {
        let key = c_minor();
        // C, D, Eb, F, G, Ab, Bb in C minor
        let pattern = make_pattern(vec![(60, 0), (62, 480), (63, 960), (65, 1440)]);
        let result =
            check_scale_compliance(&pattern, &key, SegmentId::default(), LayerId::default());
        assert!((result.compliance_ratio - 1.0).abs() < f32::EPSILON);
        assert!(result.diagnostics.is_empty());
    }

    #[test]
    fn out_of_scale_note_detected() {
        let key = c_minor();
        // C# (61) is not in C minor
        let pattern = make_pattern(vec![(60, 0), (61, 480)]);
        let result =
            check_scale_compliance(&pattern, &key, SegmentId::default(), LayerId::default());
        assert!(result.compliance_ratio < 1.0);
        assert_eq!(result.diagnostics.len(), 1);
        assert_eq!(result.diagnostics[0].code, "HARMONY_SCALE_VIOLATION");
    }
}
