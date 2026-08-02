//! Chord-tone ratio validation — checks that melody notes align with chord tones.
//!
//! A high chord-tone ratio means the melody strongly relates to the harmony.
//! The architecture spec requires this as a key symbolic validation metric.

use scorestrata_core::diagnostic::{Diagnostic, MetricScope, SuggestedPatch};
use scorestrata_core::domain::Pattern;
use scorestrata_core::error::Severity;
use scorestrata_core::ids::{DiagnosticId, LayerId, SegmentId};
use scorestrata_theory::chord::Chord;
use scorestrata_theory::key::Key;

/// Configuration for a chord-tone ratio check.
#[derive(Debug, Clone)]
pub struct ChordToneConfig {
    /// Segment being checked (None if segment-level check).
    pub segment_id: Option<SegmentId>,
    /// Layer being checked (None if layer-level check).
    pub layer_id: Option<LayerId>,
    /// Minimum acceptable chord-tone ratio.
    pub min_ratio: f32,
}

/// Result of a chord-tone ratio check.
#[derive(Debug, Clone)]
pub struct ChordToneResult {
    /// Total notes checked.
    pub total_notes: u32,
    /// Notes that are chord tones.
    pub chord_tones: u32,
    /// Notes that are scale tones but not chord tones.
    pub scale_tones: u32,
    /// Notes that are neither chord tones nor scale tones.
    pub non_scale_tones: u32,
    /// Chord-tone ratio (`chord_tones` / `total_notes`).
    pub chord_tone_ratio: f32,
    /// Diagnostics for non-chord-tone notes.
    pub diagnostics: Vec<Diagnostic>,
}

/// Check chord-tone ratio for a pattern against a chord and key.
///
/// Notes are classified as:
/// - **chord tone**: pitch is in the chord
/// - **scale tone**: pitch is in the scale but not the chord
/// - **non-scale tone**: pitch is outside the scale entirely
///
/// Chord tones on strong beats (1 and 3 in 4/4) are weighted higher.
#[must_use]
#[expect(
    clippy::too_many_lines,
    reason = "Chord-tone validation logic is naturally verbose with diagnostic generation"
)]
pub fn check_chord_tone_ratio(
    pattern: &Pattern,
    chord: &Chord,
    key: &Key,
    config: &ChordToneConfig,
) -> ChordToneResult {
    let mut total = 0u32;
    let mut chord_tones = 0u32;
    let mut scale_tones = 0u32;
    let mut non_scale_tones = 0u32;
    let mut diagnostics = Vec::new();

    let beats_per_bar = pattern.time_signature.numerator;

    for note in &pattern.notes {
        total += 1;
        let (bar, beat, _) = note.start.to_bar_beat_tick(beats_per_bar);
        let is_strong_beat = beat % 2 == 0;

        if chord.contains_note(&note.pitch) {
            chord_tones += 1;
        } else if key.contains_note(&note.pitch) {
            scale_tones += 1;
            if is_strong_beat {
                diagnostics.push(Diagnostic {
                    id: DiagnosticId::new(),
                    code: "CHORD_TONE_STRONG_BEAT_MISMATCH".to_string(),
                    metric: "harmony.chord_tone_ratio".to_string(),
                    severity: Severity::Warning,
                    scope: MetricScope {
                        segment_id: config.segment_id,
                        layer_id: config.layer_id,
                        bar_range: Some((bar, bar + 1)),
                    },
                    evidence_uris: vec![format!(
                        "project://segments/{:?}/{:?}/notes?bar={}",
                        config.segment_id, config.layer_id, bar
                    )],
                    message: format!(
                        "{} at bar {} beat {} is a scale tone but not a chord tone on a strong beat",
                        note.pitch.name(), bar, beat
                    ),
                    root_causes: vec![
                        "non-chord-tone on strong beat".to_string(),
                        "melody-harmony misalignment".to_string(),
                    ],
                    suggested_patches: vec![SuggestedPatch {
                        op: "harmonize_melody".to_string(),
                        params: serde_json::json!({
                            "segment_id": config.segment_id,
                            "layer_id": config.layer_id,
                            "strength": 0.8,
                        }),
                        expected_delta: Some("+0.10".to_string()),
                        confidence: 0.7,
                    }],
                    expected_delta: None,
                    confidence: 0.7,
                });
            }
        } else {
            non_scale_tones += 1;
            diagnostics.push(Diagnostic {
                id: DiagnosticId::new(),
                code: "CHORD_TONE_NON_SCALE_NOTE".to_string(),
                metric: "harmony.chord_tone_ratio".to_string(),
                severity: Severity::Warning,
                scope: MetricScope {
                    segment_id: config.segment_id,
                    layer_id: config.layer_id,
                    bar_range: Some((bar, bar + 1)),
                },
                evidence_uris: vec![format!(
                    "project://segments/{:?}/{:?}/notes?bar={}",
                    config.segment_id, config.layer_id, bar
                )],
                message: format!(
                    "{} at bar {} beat {} is outside the chord and scale",
                    note.pitch.name(),
                    bar,
                    beat
                ),
                root_causes: vec!["note outside chord and scale".to_string()],
                suggested_patches: vec![SuggestedPatch {
                    op: "quantize_layer_to_scale".to_string(),
                    params: serde_json::json!({
                        "segment_id": config.segment_id,
                        "layer_id": config.layer_id,
                        "strength": 1.0,
                    }),
                    expected_delta: Some("+0.14".to_string()),
                    confidence: 0.9,
                }],
                expected_delta: None,
                confidence: 0.9,
            });
        }
    }

    let chord_tone_ratio = if total > 0 {
        #[expect(
            clippy::cast_precision_loss,
            reason = "Chord-tone ratio for music validation"
        )]
        let ratio = chord_tones as f32 / total as f32;
        ratio
    } else {
        1.0
    };

    if chord_tone_ratio < config.min_ratio && total > 0 {
        diagnostics.push(Diagnostic {
            id: DiagnosticId::new(),
            code: "CHORD_TONE_RATIO_LOW".to_string(),
            metric: "harmony.chord_tone_ratio".to_string(),
            severity: Severity::Warning,
            scope: MetricScope {
                segment_id: config.segment_id,
                layer_id: config.layer_id,
                bar_range: None,
            },
            evidence_uris: vec![format!(
                "project://segments/{:?}/layers/{:?}",
                config.segment_id, config.layer_id
            )],
            message: format!(
                "Chord-tone ratio {:.2} is below minimum {:.2} ({}/{} notes are chord tones)",
                chord_tone_ratio, config.min_ratio, chord_tones, total
            ),
            root_causes: vec![
                "melody-harmony misalignment".to_string(),
                "insufficient chord-tone emphasis".to_string(),
            ],
            suggested_patches: vec![SuggestedPatch {
                op: "harmonize_melody".to_string(),
                params: serde_json::json!({
                    "segment_id": config.segment_id,
                    "layer_id": config.layer_id,
                    "strength": 1.0,
                }),
                expected_delta: Some(format!(
                    "+{:.2}",
                    (config.min_ratio - chord_tone_ratio).max(0.05)
                )),
                confidence: 0.8,
            }],
            expected_delta: None,
            confidence: 0.8,
        });
    }

    ChordToneResult {
        total_notes: total,
        chord_tones,
        scale_tones,
        non_scale_tones,
        chord_tone_ratio,
        diagnostics,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use scorestrata_core::domain::{Duration, MidiNote, Tick, TimeSignature, Velocity};
    use scorestrata_core::ids::PatternId;
    use scorestrata_theory::chord::ChordQuality;
    use scorestrata_theory::key::Mode;

    fn c_minor_key() -> Key {
        Key::new(0, Mode::Minor)
    }

    fn c_minor_chord() -> Chord {
        Chord {
            root: 0,
            quality: ChordQuality::Minor,
        }
    }

    fn make_config(min_ratio: f32) -> ChordToneConfig {
        ChordToneConfig {
            segment_id: None,
            layer_id: None,
            min_ratio,
        }
    }

    fn make_pattern(notes: Vec<(u8, u64)>) -> Pattern {
        Pattern {
            id: PatternId::new(),
            name: "test".to_string(),
            bars: 1,
            time_signature: TimeSignature::default(),
            notes: notes
                .into_iter()
                .map(|(pitch, tick)| scorestrata_core::domain::Note {
                    id: scorestrata_core::ids::NoteId::new(),
                    pitch: MidiNote::new(pitch),
                    start: Tick::new(tick),
                    duration: Duration::quarter(),
                    velocity: Velocity::new(100),
                    tied: false,
                })
                .collect(),
            steps: Vec::new(),
        }
    }

    #[test]
    fn all_chord_tones_pass() {
        let pattern = make_pattern(vec![(60, 0), (63, 480), (67, 960), (60, 1440)]);
        let result = check_chord_tone_ratio(
            &pattern,
            &c_minor_chord(),
            &c_minor_key(),
            &make_config(0.65),
        );
        assert!((result.chord_tone_ratio - 1.0).abs() < f32::EPSILON);
        assert!(result.diagnostics.is_empty());
    }

    #[test]
    fn low_ratio_triggers_diagnostic() {
        let pattern = make_pattern(vec![(62, 0), (65, 480), (69, 960), (62, 1440)]);
        let result = check_chord_tone_ratio(
            &pattern,
            &c_minor_chord(),
            &c_minor_key(),
            &make_config(0.65),
        );
        assert!(result.chord_tone_ratio < 0.5);
        assert!(result.diagnostics.len() >= 2);
    }

    #[test]
    fn empty_pattern_returns_one() {
        let pattern = make_pattern(vec![]);
        let result = check_chord_tone_ratio(
            &pattern,
            &c_minor_chord(),
            &c_minor_key(),
            &make_config(0.65),
        );
        assert!((result.chord_tone_ratio - 1.0).abs() < f32::EPSILON);
    }
}
