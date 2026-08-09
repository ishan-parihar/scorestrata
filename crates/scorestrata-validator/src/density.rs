//! Rhythmic density validation.

use scorestrata_core::diagnostic::{Diagnostic, MetricScope};
use scorestrata_core::domain::Pattern;
use scorestrata_core::error::Severity;
use scorestrata_core::ids::DiagnosticId;

/// Calculate the rhythmic density of a pattern (notes per beat).
#[must_use]
pub fn rhythmic_density(pattern: &Pattern) -> f32 {
    if pattern.notes.is_empty() || pattern.bars == 0 {
        return 0.0;
    }
    #[expect(
        clippy::cast_precision_loss,
        reason = "Rhythmic density computation for music validation"
    )]
    let beats_per_bar =
        pattern.time_signature.numerator as f32 * 4.0 / pattern.time_signature.denominator as f32;
    #[expect(
        clippy::cast_precision_loss,
        reason = "Rhythmic density computation for music validation"
    )]
    let total_beats = pattern.bars as f32 * beats_per_bar;
    #[expect(
        clippy::cast_precision_loss,
        reason = "Rhythmic density computation for music validation"
    )]
    let notes_len = pattern.notes.len() as f32;
    notes_len / total_beats
}

/// Check that drum density meets a minimum threshold.
#[must_use]
pub fn check_drum_density(
    pattern: &Pattern,
    min_density: f32,
    segment_id: scorestrata_core::ids::SegmentId,
    layer_id: scorestrata_core::ids::LayerId,
) -> Option<Diagnostic> {
    let density = rhythmic_density(pattern);
    if density < min_density {
        Some(Diagnostic {
            id: DiagnosticId::new(),
            code: "DRUM_DENSITY_LOW".to_string(),
            metric: "rhythm.drum_density".to_string(),
            severity: Severity::Warning,
            scope: MetricScope {
                segment_id: Some(segment_id),
                layer_id: Some(layer_id),
                bar_range: None,
            },
            evidence_uris: vec![],
            message: format!("Drum density {density:.2} is below minimum {min_density:.2}"),
            root_causes: vec!["insufficient drum hits per beat".to_string()],
            suggested_patches: vec![],
            expected_delta: None,
            confidence: 0.8,
        })
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use scorestrata_core::domain::{Duration, MidiNote, Note, Tick, TimeSignature, Velocity};
    use scorestrata_core::ids::PatternId;

    #[test]
    fn empty_pattern_has_zero_density() {
        let pattern = Pattern {
            id: PatternId::new(),
            name: "empty".to_string(),
            bars: 4,
            time_signature: TimeSignature::default(),
            notes: Vec::new(),
            steps: Vec::new(),
        };
        assert!((rhythmic_density(&pattern) - 0.0).abs() < f32::EPSILON);
    }

    #[test]
    fn four_notes_in_four_bars_has_density_one_quarter() {
        let pattern = Pattern {
            id: PatternId::new(),
            name: "test".to_string(),
            bars: 4,
            time_signature: TimeSignature::default(),
            notes: (0..4)
                .map(|i| Note {
                    id: scorestrata_core::ids::NoteId::new(),
                    pitch: MidiNote::new(60),
                    start: Tick::new(i * 1920),
                    duration: Duration::quarter(),
                    velocity: Velocity::new(100),
                    tied: false,
                    articulation: None,
                })
                .collect(),
            steps: Vec::new(),
        };
        // 4 notes / (4 bars * 4 beats) = 0.25 notes per beat
        let density = rhythmic_density(&pattern);
        assert!((density - 0.25).abs() < 0.01);
    }
}
