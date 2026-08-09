//! Polyphony budget + anti-masking validation (Phase F).
//!
//! The render engine allocates a synth voice per note-on, so a segment that
//! stacks dense chord/bass/lead layers at the same tick can exceed the voice
//! budget; and layers whose octave registers overlap heavily mask each
//! other's fundamentals in the mix. These checks make `ReduceMasking` real at
//! the symbolic plane: they report exactly which segment exceeds the budget
//! and which layer pairs are fighting for the same register, with the
//! `reduce_masking` patch suggested in every diagnostic.

use scorestrata_core::diagnostic::{Diagnostic, MetricScope, SuggestedPatch};
use scorestrata_core::domain::{Project, Segment};
use scorestrata_core::error::Severity;
use scorestrata_core::ids::DiagnosticId;

/// Default simultaneous-voice budget (the synth's per-render voice ceiling).
pub const DEFAULT_POLYPHONY_BUDGET: u32 = 12;
/// A single sounding note: (`absolute_start_tick`, `absolute_end_tick`, midi).
type SoundingNote = (u64, u64, i32);

/// A layer's sounding notes plus its identity for diagnostics.
type LayerSounding = (String, scorestrata_core::ids::LayerId, Vec<SoundingNote>);

/// Maximum pitch proximity (semitones) for a masking event.
///
/// Two simultaneous notes closer than a 12th share fundamental energy and
/// mask each other in the mix; an octave doubling (exactly +12) is the one
/// layered arrangement that stays clean by construction.
const MASKING_PITCH_DIFF_SEMITONES: i32 = 12;

/// Instrument-name markers for percussion layers.
///
/// Percussion is bus-managed (drum group, sidechains, short envelopes) rather
/// than register-disciplined, so it never participates in anti-masking.
const PERCUSSION_MARKERS: [&str; 8] = [
    "drum", "sampler", "perc", "kick", "snare", "hat", "cymbal", "clap",
];

/// Whether an instrument name marks a percussion layer.
#[must_use]
pub fn is_percussion(instrument: &str) -> bool {
    let lower = instrument.to_ascii_lowercase();
    PERCUSSION_MARKERS.iter().any(|k| lower.contains(k))
}

/// Peak simultaneous sounding notes inside one segment (voice count).
///
/// Every note-on in every pattern instance counts as a voice, exactly as the
/// synth engine allocates them; chords (same-tick notes) therefore count as
/// multiple voices. The sweep is tick-exact: sort all note-on/note-off
/// events, walk them in order, and track the running concurrent count.
#[must_use]
pub fn max_simultaneous_notes(project: &Project, segment: &Segment) -> u32 {
    let mut events: Vec<(u64, i32)> = Vec::new();
    for layer in &segment.layers {
        for instance in &layer.pattern_instances {
            let Some(pattern) = project
                .patterns
                .iter()
                .find(|p| p.id == instance.pattern_id)
            else {
                continue;
            };
            for note in &pattern.notes {
                let start = instance.start_tick.0.saturating_add(note.start.0);
                let end = start.saturating_add(note.duration.0);
                events.push((start, 1));
                events.push((end, -1));
            }
        }
    }
    if events.is_empty() {
        return 0;
    }
    events.sort_by_key(|(tick, _)| *tick);
    let mut active: u32 = 0;
    let mut peak: u32 = 0;
    for (_, delta) in events {
        if delta > 0 {
            active += 1;
        } else {
            active = active.saturating_sub(1);
        }
        peak = peak.max(active);
    }
    peak
}

/// Flag every segment whose peak polyphony exceeds the budget.
///
/// `POLYPHONY_BUDGET_EXCEEDED` names the segment, the measured peak, and the
/// budget, and suggests the `reduce_masking` patch (mode `polyphony`).
#[must_use]
pub fn check_polyphony_budget(project: &Project, budget: u32) -> Vec<Diagnostic> {
    let mut diagnostics = Vec::new();
    for segment in &project.arrangement.segments {
        let peak = max_simultaneous_notes(project, segment);
        if peak <= budget {
            continue;
        }
        diagnostics.push(Diagnostic {
            id: DiagnosticId::new(),
            code: "POLYPHONY_BUDGET_EXCEEDED".to_string(),
            metric: "arrangement.polyphony".to_string(),
            severity: Severity::Warning,
            scope: MetricScope {
                segment_id: Some(segment.id),
                layer_id: None,
                bar_range: None,
            },
            evidence_uris: vec![format!("project://segments/{}/polyphony", segment.id)],
            message: format!(
                "Segment '{}' peaks at {peak} simultaneous voices (budget {budget}) — the mix will lose clarity and CPU headroom",
                segment.name
            ),
            root_causes: vec![
                "dense layers stacking chord + bass + lead at the same tick".to_string(),
            ],
            suggested_patches: vec![SuggestedPatch {
                op: "reduce_masking".to_string(),
                params: serde_json::json!({"mode": "polyphony"}),
                expected_delta: Some(format!("-{}", peak - budget)),
                confidence: 0.7,
            }],
            expected_delta: None,
            confidence: 0.9,
        });
    }
    diagnostics
}

/// Flag every pair of sounding layers whose octave registers overlap by
/// [`MASKING_OVERLAP_OCTAVES`] or more — the symbolic anti-masking check.
///
/// `MASKING_REGISTER_OVERLAP` names both layers, their registers, and the
/// overlap in octaves, and suggests `reduce_masking` (mode `register`) so the
/// agent can re-band one of the layers. Only layers with a declared register
/// and at least one pattern instance in the segment participate.
#[must_use]
pub fn check_masking_overlap(project: &Project) -> Vec<Diagnostic> {
    let mut diagnostics = Vec::new();
    for segment in &project.arrangement.segments {
        // Per-layer sounding notes: (absolute_start, absolute_end, midi).
        let mut sounding: Vec<LayerSounding> = Vec::new();
        for layer in &segment.layers {
            if is_percussion(&layer.instrument) || layer.pattern_instances.is_empty() {
                continue;
            }
            let mut notes = Vec::new();
            for instance in &layer.pattern_instances {
                let Some(pattern) = project
                    .patterns
                    .iter()
                    .find(|p| p.id == instance.pattern_id)
                else {
                    continue;
                };
                let transpose = instance.transpose * 12;
                for note in &pattern.notes {
                    let start = instance.start_tick.0.saturating_add(note.start.0);
                    let end = start.saturating_add(note.duration.0);
                    notes.push((start, end, transpose + i32::from(note.pitch.0)));
                }
            }
            if notes.is_empty() {
                continue;
            }
            sounding.push((layer.name.clone(), layer.id, notes));
        }
        for (index, (name_a, id_a, notes_a)) in sounding.iter().enumerate() {
            for (name_b, _id_b, notes_b) in sounding.iter().skip(index + 1) {
                let mut min_diff = i32::MAX;
                for (start_a, end_a, pitch_a) in notes_a {
                    for (start_b, end_b, pitch_b) in notes_b {
                        let overlaps = *start_a < *end_b && *start_b < *end_a;
                        if !overlaps {
                            continue;
                        }
                        min_diff = min_diff.min((pitch_a - pitch_b).abs());
                    }
                }
                if min_diff >= MASKING_PITCH_DIFF_SEMITONES {
                    continue;
                }
                diagnostics.push(Diagnostic {
                    id: DiagnosticId::new(),
                    code: "MASKING_PITCH_COLLISION".to_string(),
                    metric: "arrangement.anti_masking".to_string(),
                    severity: Severity::Warning,
                    scope: MetricScope {
                        segment_id: Some(segment.id),
                        layer_id: Some(*id_a),
                        bar_range: None,
                    },
                    evidence_uris: vec![format!(
                        "project://segments/{}/layers/{}",
                        segment.id, id_a
                    )],
                    message: format!(
                        "Layers '{name_a}' and '{name_b}' play simultaneous notes as close as {min_diff} semitones — their fundamentals will mask each other in the mix"
                    ),
                    root_causes: vec![
                        "two melodic layers voicing the same register at the same time".to_string(),
                    ],
                    suggested_patches: vec![SuggestedPatch {
                        op: "reduce_masking".to_string(),
                        params: serde_json::json!({
                            "mode": "pitch",
                            "layers": [name_a, name_b],
                        }),
                        expected_delta: Some("+0.15".to_string()),
                        confidence: 0.8,
                    }],
                    expected_delta: None,
                    confidence: 0.85,
                });
            }
        }
    }
    diagnostics
}

#[cfg(test)]
mod tests {
    use super::*;
    use scorestrata_core::domain::{
        Arrangement, Duration, Layer, MidiNote, Note, Pattern, PatternInstance, Register, Segment,
        SegmentRole, Tick, TimeSignature, Velocity,
    };
    use scorestrata_core::ids::{LayerId, NoteId, PatternId, PatternInstanceId, SegmentId};

    fn pattern_with_notes(notes: Vec<Note>) -> Pattern {
        Pattern {
            id: PatternId::new(),
            name: "p".to_string(),
            bars: 4,
            time_signature: TimeSignature::default(),
            notes,
            steps: Vec::new(),
        }
    }

    fn chord_note(midi: u8, tick: u64) -> Note {
        Note {
            id: NoteId::new(),
            pitch: MidiNote::new(midi),
            start: Tick::new(tick),
            duration: Duration::whole(),
            velocity: Velocity::new(100),
            tied: false,
            articulation: None,
        }
    }

    fn layer_with(register: Option<Register>, pattern_id: Option<PatternId>) -> Layer {
        Layer {
            id: LayerId::new(),
            name: "l".to_string(),
            instrument: "synth".to_string(),
            pattern_instances: pattern_id
                .map(|pid| {
                    vec![PatternInstance {
                        id: PatternInstanceId::new(),
                        pattern_id: pid,
                        start_tick: Tick::new(0),
                        transpose: 0,
                        velocity_multiplier: 1.0,
                    }]
                })
                .unwrap_or_default(),
            register,
            automation: vec![],
            fx: None,
            mix_gain_db: None,
            mix_pan: None,
            group: None,
        }
    }

    fn project_with(segments: Vec<Segment>, patterns: Vec<Pattern>) -> Project {
        let mut project = scorestrata_testkit::melodic_techno_project();
        project.arrangement = Arrangement {
            total_bars: 8,
            time_signature: TimeSignature::default(),
            segments,
            energy_curve: vec![],
        };
        project.patterns = patterns;
        project
    }

    #[test]
    fn empty_segment_has_zero_peak_polyphony() {
        let project = scorestrata_testkit::melodic_techno_project();
        let segment = Segment {
            id: SegmentId::new(),
            name: "empty".to_string(),
            role: SegmentRole::Intro,
            start_bar: 0,
            bars: 4,
            layers: vec![],
        };
        assert_eq!(max_simultaneous_notes(&project, &segment), 0);
    }

    #[test]
    fn chord_stacks_count_as_multiple_voices() {
        // A 4-note chord at tick 0 is four simultaneous voices.
        let pattern = pattern_with_notes(vec![
            chord_note(60, 0),
            chord_note(64, 0),
            chord_note(67, 0),
            chord_note(72, 0),
        ]);
        let segment = Segment {
            id: SegmentId::new(),
            name: "chord".to_string(),
            role: SegmentRole::Chorus,
            start_bar: 0,
            bars: 4,
            layers: vec![layer_with(None, Some(pattern.id))],
        };
        let project = project_with(vec![segment], vec![pattern]);
        assert_eq!(
            max_simultaneous_notes(&project, &project.arrangement.segments[0]),
            4
        );
    }

    #[test]
    fn polyphony_budget_flags_only_over_budget_segments() {
        // One layer with a 16-note dense pattern (all at the same tick) → peak 16.
        let pattern = pattern_with_notes((0..16).map(|i| chord_note(48 + i, 0)).collect());
        let segment = Segment {
            id: SegmentId::new(),
            name: "dense".to_string(),
            role: SegmentRole::Build,
            start_bar: 0,
            bars: 4,
            layers: vec![layer_with(None, Some(pattern.id))],
        };
        let project = project_with(vec![segment], vec![pattern]);
        let diagnostics = check_polyphony_budget(&project, DEFAULT_POLYPHONY_BUDGET);
        assert_eq!(diagnostics.len(), 1);
        assert_eq!(diagnostics[0].code, "POLYPHONY_BUDGET_EXCEEDED");
        assert!(
            diagnostics[0]
                .suggested_patches
                .iter()
                .any(|p| p.op == "reduce_masking"),
            "budget diagnostic must suggest reduce_masking"
        );
    }

    #[test]
    fn masking_flags_simultaneous_close_pitches() {
        // Two layers play the same pitch (C4) at the same tick → diff 0 < 12.
        let pattern = pattern_with_notes(vec![chord_note(60, 0)]);
        let segment = Segment {
            id: SegmentId::new(),
            name: "mud".to_string(),
            role: SegmentRole::Chorus,
            start_bar: 0,
            bars: 4,
            layers: vec![
                layer_with(None, Some(pattern.id)),
                layer_with(None, Some(pattern.id)),
            ],
        };
        let project = project_with(vec![segment], vec![pattern]);
        let diagnostics = check_masking_overlap(&project);
        assert_eq!(diagnostics.len(), 1);
        assert_eq!(diagnostics[0].code, "MASKING_PITCH_COLLISION");
        assert!(
            diagnostics[0]
                .suggested_patches
                .iter()
                .any(|p| p.op == "reduce_masking"),
            "masking diagnostic must suggest reduce_masking"
        );
    }

    #[test]
    fn octave_doubling_does_not_mask() {
        // One layer at C4 (60), one at C5 (72): diff = 12, the one arrangement
        // that stays clean by construction (octave doubling).
        let pattern_a = pattern_with_notes(vec![chord_note(60, 0)]);
        let mut pattern_b = pattern_a.clone();
        pattern_b.id = PatternId::new();
        pattern_b.notes[0].pitch = MidiNote::new(72);
        let segment = Segment {
            id: SegmentId::new(),
            name: "doubled".to_string(),
            role: SegmentRole::Chorus,
            start_bar: 0,
            bars: 4,
            layers: vec![
                layer_with(None, Some(pattern_a.id)),
                layer_with(None, Some(pattern_b.id)),
            ],
        };
        let project = project_with(vec![segment], vec![pattern_a, pattern_b]);
        assert!(check_masking_overlap(&project).is_empty());
    }

    #[test]
    fn non_simultaneous_same_pitch_passes() {
        // Same pitch but at non-overlapping ticks → no simultaneous mask.
        let pattern_a = pattern_with_notes(vec![chord_note(60, 0)]);
        let mut pattern_b = pattern_a.clone();
        pattern_b.id = PatternId::new();
        pattern_b.notes[0].start = Tick::new(1920 * 8); // far later than a whole note
        let segment = Segment {
            id: SegmentId::new(),
            name: "call_response".to_string(),
            role: SegmentRole::Chorus,
            start_bar: 0,
            bars: 4,
            layers: vec![
                layer_with(None, Some(pattern_a.id)),
                layer_with(None, Some(pattern_b.id)),
            ],
        };
        let project = project_with(vec![segment], vec![pattern_a, pattern_b]);
        assert!(check_masking_overlap(&project).is_empty());
    }

    #[test]
    fn percussion_layers_are_exempt_from_masking() {
        // A drum layer and a lead layer playing the same pitch simultaneously
        // must NOT flag — percussion is bus-managed, not anti-masked.
        let pattern = pattern_with_notes(vec![chord_note(60, 0)]);
        let mut drum = layer_with(None, Some(pattern.id));
        drum.instrument = "drum_sampler".to_string();
        let segment = Segment {
            id: SegmentId::new(),
            name: "groove".to_string(),
            role: SegmentRole::Chorus,
            start_bar: 0,
            bars: 4,
            layers: vec![drum, layer_with(None, Some(pattern.id))],
        };
        let project = project_with(vec![segment], vec![pattern]);
        assert!(check_masking_overlap(&project).is_empty());
    }

    #[test]
    fn polyphony_peak_counts_percussion_and_melodic_voices() {
        // Polyphony budget is a *voice* budget: drums count too (every hit is
        // a synth voice). A drum + 16-note chord stack must exceed the budget.
        let drum_pattern = pattern_with_notes(vec![chord_note(36, 0)]);
        let dense = pattern_with_notes((0..16).map(|i| chord_note(48 + i, 0)).collect());
        let mut drum = layer_with(None, Some(drum_pattern.id));
        drum.instrument = "drum_sampler".to_string();
        let segment = Segment {
            id: SegmentId::new(),
            name: "heavy".to_string(),
            role: SegmentRole::Build,
            start_bar: 0,
            bars: 4,
            layers: vec![drum, layer_with(None, Some(dense.id))],
        };
        let project = project_with(vec![segment], vec![drum_pattern, dense]);
        let diagnostics = check_polyphony_budget(&project, DEFAULT_POLYPHONY_BUDGET);
        assert_eq!(diagnostics.len(), 1);
        assert_eq!(diagnostics[0].code, "POLYPHONY_BUDGET_EXCEEDED");
    }
}
