//! Layer octave-register checks (G2).
//!
//! Every layer may declare an octave register (bass 1–2, chords 3–5, lead
//! 5–7, …). Notes outside the register indicate either a transposed pattern
//! spilling into a neighbor's band or hand-written notes in the wrong octave
//! — both of which produce the muddy, same-sounding mixes the production
//! pipeline exists to prevent. The compiler clamps at compile time; this
//! check flags any violations that remain in the source.

use scorestrata_core::diagnostic::{Diagnostic, MetricScope};
use scorestrata_core::domain::Project;
use scorestrata_core::error::Severity;
use scorestrata_core::ids::DiagnosticId;

/// Check every layer's notes (via its pattern instances) against its declared
/// register, returning one diagnostic per offending layer.
///
/// Only layers with a declared register and at least one note are checked;
/// percussion and legacy layers (no register) are skipped.
#[must_use]
pub fn check_layer_registers(project: &Project) -> Vec<Diagnostic> {
    let mut diagnostics = Vec::new();
    for segment in &project.arrangement.segments {
        for layer in &segment.layers {
            let Some(register) = &layer.register else {
                continue;
            };
            // Collect the notes this layer will actually play (pattern notes
            // plus the instance transpose), so the check mirrors the compiler.
            let mut violations = Vec::new();
            for instance in &layer.pattern_instances {
                let Some(pattern) = project
                    .patterns
                    .iter()
                    .find(|p| p.id == instance.pattern_id)
                else {
                    continue;
                };
                let transpose = instance.transpose;
                for note in &pattern.notes {
                    #[expect(
                        clippy::cast_sign_loss,
                        reason = "MIDI notes 0-127 plus bounded transpose are clamped non-negative before narrowing to u8"
                    )]
                    let pitch =
                        (i64::from(note.pitch.0) + i64::from(transpose)).clamp(0, 127) as u8;
                    if !register.contains_note(scorestrata_core::domain::MidiNote::new(pitch)) {
                        violations.push(format!(
                            "{} (expected oct {}..{} but sits at {})",
                            scorestrata_core::domain::MidiNote::new(pitch).name(),
                            register.min_octave,
                            register.max_octave,
                            scorestrata_core::domain::MidiNote::new(pitch).name()
                        ));
                    }
                }
            }
            violations.dedup();
            if !violations.is_empty() {
                diagnostics.push(Diagnostic {
                    id: DiagnosticId::new(),
                    code: "LAYER_NOTE_OUTSIDE_REGISTER".to_string(),
                    metric: "layers.register_violations".to_string(),
                    severity: Severity::Warning,
                    scope: MetricScope {
                        segment_id: Some(segment.id),
                        layer_id: Some(layer.id),
                        bar_range: None,
                    },
                    evidence_uris: vec![format!(
                        "project://segments/{}/layers/{}",
                        segment.id, layer.id
                    )],
                    message: format!(
                        "Layer '{}' ({} in '{}') has notes outside its declared register {}..{}: {}",
                        layer.name,
                        layer.instrument,
                        segment.name,
                        register.min_octave,
                        register.max_octave,
                        violations.join(", ")
                    ),
                    root_causes: vec![
                        "pattern transposed or notes written outside the layer's octave band"
                            .to_string(),
                    ],
                    suggested_patches: vec![],
                    expected_delta: None,
                    confidence: 1.0,
                });
            }
        }
    }
    diagnostics
}

#[cfg(test)]
#[expect(
    clippy::expect_used,
    reason = "Tests unwrap fixture registers and notes"
)]
mod tests {
    use super::*;
    use scorestrata_core::domain::{MidiNote, Note, Register, Tick, Velocity};

    fn note(pitch: u8, start: u64) -> Note {
        Note {
            id: scorestrata_core::ids::NoteId::new(),
            pitch: MidiNote::new(pitch),
            start: Tick::new(start),
            duration: scorestrata_core::domain::Duration::quarter(),
            velocity: Velocity::default_vel(),
            tied: false,
        }
    }

    /// A project with one segment containing a single lead layer that plays
    /// the given pattern through one instance.
    fn project_with_layer(notes: Vec<Note>) -> Project {
        let mut pattern = scorestrata_testkit::test_pattern();
        pattern.notes = notes;
        let pattern_id = pattern.id;
        let mut project = scorestrata_testkit::minimal_project();
        project.patterns.push(pattern);
        project.arrangement.segments[0].layers = vec![scorestrata_core::domain::Layer {
            id: scorestrata_core::ids::LayerId::new(),
            name: "Lead".to_string(),
            instrument: "lead_synth".to_string(),
            pattern_instances: vec![scorestrata_core::domain::PatternInstance {
                id: scorestrata_core::ids::PatternInstanceId::new(),
                pattern_id,
                start_tick: Tick::new(0),
                transpose: 0,
                velocity_multiplier: 1.0,
            }],
            register: Some(Register::new(5, 7)),
            automation: vec![],
            fx: None,
            mix_gain_db: None,
            mix_pan: None,
            group: None,
        }];
        project
    }

    #[test]
    fn in_register_notes_produce_no_diagnostics() {
        // C5..C7 sit inside the 5..7 register.
        let project = project_with_layer(vec![note(72, 0), note(79, 480), note(84, 960)]);
        let diagnostics = check_layer_registers(&project);
        assert!(
            diagnostics
                .iter()
                .all(|d| d.code != "LAYER_NOTE_OUTSIDE_REGISTER"),
            "in-register notes must not flag: {diagnostics:#?}"
        );
    }

    #[test]
    fn out_of_register_note_is_flagged() {
        // C2 (octave 1) is below a 5..7 register.
        let project = project_with_layer(vec![note(36, 0), note(84, 480)]);
        let diagnostics = check_layer_registers(&project);
        assert!(
            diagnostics
                .iter()
                .any(|d| d.code == "LAYER_NOTE_OUTSIDE_REGISTER"),
            "out-of-register note must be flagged"
        );
        let d = diagnostics
            .iter()
            .find(|d| d.code == "LAYER_NOTE_OUTSIDE_REGISTER")
            .expect("diagnostic");
        assert!(d.message.contains("C2"), "names the offending note");
    }

    #[test]
    fn transposed_instance_notes_are_checked_after_transpose() {
        // C2 at pattern level transposed +48 lands at C6 (in register 5..7).
        let mut pattern = scorestrata_testkit::test_pattern();
        pattern.notes = vec![note(36, 0)];
        let pattern_id = pattern.id;
        let mut project = scorestrata_testkit::minimal_project();
        project.patterns.push(pattern);
        project.arrangement.segments[0].layers = vec![scorestrata_core::domain::Layer {
            id: scorestrata_core::ids::LayerId::new(),
            name: "Lead".to_string(),
            instrument: "lead_synth".to_string(),
            pattern_instances: vec![scorestrata_core::domain::PatternInstance {
                id: scorestrata_core::ids::PatternInstanceId::new(),
                pattern_id,
                start_tick: Tick::new(0),
                transpose: 48,
                velocity_multiplier: 1.0,
            }],
            register: Some(Register::new(5, 7)),
            automation: vec![],
            fx: None,
            mix_gain_db: None,
            mix_pan: None,
            group: None,
        }];
        let diagnostics = check_layer_registers(&project);
        assert!(
            diagnostics
                .iter()
                .all(|d| d.code != "LAYER_NOTE_OUTSIDE_REGISTER"),
            "+48 transpose moves C2 to C6, inside the register"
        );
    }
}
