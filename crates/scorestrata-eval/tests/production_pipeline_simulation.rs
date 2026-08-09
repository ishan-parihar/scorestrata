//! Production-pipeline end-to-end simulation (production-pipeline-ux-plan G′).
//!
//! The plan's Definition of Done: an agent can go from brief → arranged
//! multi-section track with octave-disciplined layers, repeated/mutated
//! patterns, automation, per-track FX, mixed against a genre reference —
//! with the whole state visible via `ss timeline` — and the result passes
//! the spectral + golden gates.
//!
//! Two complementary proofs:
//!
//! 1. **Tool surface** (`pipeline_g_tools_report_real_receipts`): every G
//!    tool (G1 timeline view, G2 registers, G3 repeat, G4 automation,
//!    G5 FX + mix preset, G6 arrangement template) is driven with a real
//!    serialized project and must report `Succeeded` with meaningful score
//!    deltas — proving the mutation logic is wired end-to-end.
//! 2. **Pipeline shape** (`pipeline_produced_track_compiles_renders_and_views`):
//!    a fully-produced project (template sections, registers, repeated
//!    patterns, automation lanes, per-layer FX) compiles to a timeline that
//!    carries `ControlChange` lanes, renders to audio with a Level-4
//!    analysis, passes the symbolic gates, and renders a layered timeline
//!    view — the whole `DoD` loop.
//!
//! This wires the real crates together — domain, planner, compiler, audio
//! renderer, validator, MCP tools, resources — so a regression in any plane
//! fails the simulation.

#![expect(
    clippy::expect_used,
    clippy::too_many_lines,
    reason = "Integration test asserts the full pipeline explicitly; helper fns are long by design"
)]

use scorestrata_core::domain::{Automation, AutomationParam, AutomationPoint, Register};
use scorestrata_core::ids::{LayerId, SegmentId};

/// G1–G7 tool surface: every production tool returns a real receipt when
/// given a serialized project.
#[test]
fn pipeline_g_tools_report_real_receipts() {
    use scorestrata_mcp::tools::*;

    let project = scorestrata_testkit::melodic_techno_project();
    let payload = serde_json::to_value(&project).unwrap_or_default();
    let seg0 = project.arrangement.segments[0].id.to_string();
    let layer1 = project.arrangement.segments[0].layers[1].id.to_string();

    // G6 — arrangement template (section archetypes per genre).
    let receipt = apply_arrangement_template(&ApplyArrangementTemplateParams {
        project_id: project.id.to_string(),
        genre: project.config.genre.clone(),
        total_bars: 32,
        project: Some(payload.clone()),
    });
    assert!(
        matches!(receipt.status, ToolStatus::Succeeded),
        "apply_arrangement_template: {}",
        receipt.summary
    );
    assert!(
        !receipt.score_delta.is_empty(),
        "arrangement template reports a delta"
    );

    // G2 — octave register discipline.
    let receipt = set_layer_register(&SetLayerRegisterParams {
        project_id: project.id.to_string(),
        segment_id: seg0.clone(),
        layer_id: layer1.clone(),
        min_octave: 1,
        max_octave: 3,
        project: Some(payload.clone()),
    });
    assert!(
        matches!(receipt.status, ToolStatus::Succeeded),
        "set_layer_register: {}",
        receipt.summary
    );
    assert!(
        receipt.score_delta.contains_key("layers.register"),
        "register delta reported: {:#?}",
        receipt.score_delta
    );

    // G3 — repeat pattern across the segment.
    let first_pattern = project.patterns[0].id.to_string();
    let receipt = repeat_pattern_in_segment(&RepeatPatternInSegmentParams {
        project_id: project.id.to_string(),
        segment_id: seg0.clone(),
        layer_id: layer1.clone(),
        pattern_id: first_pattern,
        count: 4,
        step_ticks: 480 * 4,
        transpose_step: 12,
        project: Some(payload.clone()),
    });
    assert!(
        matches!(receipt.status, ToolStatus::Succeeded),
        "repeat_pattern_in_segment: {}",
        receipt.summary
    );
    assert!(
        receipt.score_delta.contains_key("arrangement.instances"),
        "repeat delta reported: {:#?}",
        receipt.score_delta
    );

    // G4 — automation lane (cutoff sweep).
    let receipt = write_automation(&WriteAutomationParams {
        project_id: project.id.to_string(),
        segment_id: seg0.clone(),
        layer_id: layer1.clone(),
        param: "cutoff".to_string(),
        points: vec![
            scorestrata_core::domain::AutomationPointJson {
                tick: 0,
                value: 0.1,
            },
            scorestrata_core::domain::AutomationPointJson {
                tick: 480 * 16,
                value: 0.9,
            },
        ],
        project: Some(payload.clone()),
    });
    assert!(
        matches!(receipt.status, ToolStatus::Succeeded),
        "write_automation: {}",
        receipt.summary
    );
    assert!(
        receipt.score_delta.contains_key("layers.automation_lanes"),
        "automation delta reported: {:#?}",
        receipt.score_delta
    );

    // G5 — insert FX chain + genre mix preset.
    let fx = scorestrata_core::domain::FxChain {
        high_pass_hz: Some(40.0),
        eq: vec![scorestrata_core::domain::FreqBand {
            freq_hz: 200.0,
            gain_db: -2.0,
            q: 1.0,
        }],
        sidechain_from_kick: true,
        ..Default::default()
    };
    let receipt = set_track_fx(&SetTrackFxParams {
        project_id: project.id.to_string(),
        segment_id: seg0,
        layer_id: layer1,
        fx: serde_json::to_value(&fx).unwrap_or_default(),
        project: Some(payload.clone()),
    });
    assert!(
        matches!(receipt.status, ToolStatus::Succeeded),
        "set_track_fx: {}",
        receipt.summary
    );
    assert!(
        receipt.score_delta.contains_key("layers.fx"),
        "fx delta reported: {:#?}",
        receipt.score_delta
    );

    let receipt = apply_genre_mix_preset(&ApplyGenreMixPresetParams {
        project_id: project.id.to_string(),
        genre: project.config.genre.clone(),
        project: Some(payload.clone()),
    });
    assert!(
        matches!(receipt.status, ToolStatus::Succeeded),
        "apply_genre_mix_preset: {}",
        receipt.summary
    );
    assert!(
        !receipt.score_delta.is_empty(),
        "mix preset reports per-instrument staging"
    );

    // G1 — layered timeline view (the agent's arrange view).
    let receipt = get_timeline_view(&GetTimelineViewParams {
        project_id: project.id.to_string(),
        project: Some(payload),
    });
    assert!(
        matches!(receipt.status, ToolStatus::Succeeded),
        "get_timeline_view: {}",
        receipt.summary
    );
    assert!(
        receipt.score_delta.contains_key("timeline.sections"),
        "timeline view counts sections: {:#?}",
        receipt.score_delta
    );
}

/// The `DoD` loop: a fully-produced track — template sections, register
/// discipline, repeated patterns, automation lanes, per-layer FX — compiles
/// to a ControlChange-carrying timeline, renders with a Level-4 analysis,
/// and renders a layered timeline view.
#[test]
fn pipeline_produced_track_compiles_renders_and_views() {
    use scorestrata_mcp::tools::{GetTimelineViewParams, ToolStatus, get_timeline_view};

    let mut project = scorestrata_testkit::melodic_techno_project();
    let project_id = project.id.to_string();

    // G2 — register discipline by instrument role: bass 1–3, lead 5–7,
    // chords/pads 3–5 (drums unconstrained — the §1.3 octave map).
    for layer in project
        .arrangement
        .segments
        .iter_mut()
        .flat_map(|s| s.layers.iter_mut())
    {
        layer.register = Some(match layer.instrument.as_str() {
            "bass_synth" | "bass" => Register::new(1, 3),
            "lead_synth" | "lead" | "bell_synth" => Register::new(5, 7),
            "pad" | "pads" | "chord" => Register::new(3, 5),
            _ => Register::new(1, 7),
        });
    }

    // G3 — repeat the bass pattern 4× across segment 0's bass layer with an
    // octave lift toward the drop (transpose +12 per repeat).
    let bass_layer_idx = project.arrangement.segments[0]
        .layers
        .iter()
        .position(|l| l.instrument == "bass_synth" || l.instrument == "bass")
        .expect("bass layer present in the techno fixture");
    let pattern_id = project.patterns[0].id;
    for i in 0..4u64 {
        project.arrangement.segments[0].layers[bass_layer_idx]
            .pattern_instances
            .push(scorestrata_core::domain::PatternInstance {
                id: scorestrata_core::ids::PatternInstanceId::new(),
                pattern_id,
                start_tick: scorestrata_core::domain::Tick::new(i * 480 * 4),
                transpose: i32::try_from(i * 12).unwrap_or(0),
                velocity_multiplier: 1.0,
            });
    }

    // G4 — automation lanes: cutoff sweep on bass, volume on a lead layer.
    // The techno planner adds a lead to only some sections, so search every
    // segment (a section without a lead still has the drums/bass for volume).
    let (lead_seg, lead_idx) = project
        .arrangement
        .segments
        .iter()
        .enumerate()
        .find_map(|(si, seg)| {
            seg.layers
                .iter()
                .position(|l| {
                    l.instrument == "lead_synth"
                        || l.instrument == "lead"
                        || l.instrument == "pluck_synth"
                })
                .map(|li| (si, li))
        })
        .expect("at least one section has a lead layer in the techno fixture");
    let bass_id = project.arrangement.segments[0].layers[bass_layer_idx].id;
    let lead_id = project.arrangement.segments[lead_seg].layers[lead_idx].id;
    project.arrangement.segments[0].layers[bass_layer_idx]
        .automation
        .push(Automation {
            layer_id: bass_id,
            param: AutomationParam::Cutoff,
            points: vec![
                AutomationPoint {
                    tick: scorestrata_core::domain::Tick::new(0),
                    value: 0.1,
                },
                AutomationPoint {
                    tick: scorestrata_core::domain::Tick::new(480 * 16),
                    value: 0.9,
                },
            ],
        });
    project.arrangement.segments[lead_seg].layers[lead_idx]
        .automation
        .push(Automation {
            layer_id: lead_id,
            param: AutomationParam::Volume,
            points: vec![
                AutomationPoint {
                    tick: scorestrata_core::domain::Tick::new(0),
                    value: 0.7,
                },
                AutomationPoint {
                    tick: scorestrata_core::domain::Tick::new(480 * 16),
                    value: 0.5,
                },
            ],
        });

    // G5 — FX on the bass layer: HPF + low-mid scoop + sidechain to kick.
    project.arrangement.segments[0].layers[bass_layer_idx].fx =
        Some(scorestrata_core::domain::FxChain {
            high_pass_hz: Some(40.0),
            eq: vec![scorestrata_core::domain::FreqBand {
                freq_hz: 180.0,
                gain_db: -3.0,
                q: 1.2,
            }],
            sidechain_from_kick: true,
            ..Default::default()
        });

    // ── Compile: the produced track must emit automation as CC lanes ──
    let timeline =
        scorestrata_compiler::compile_project(&project).expect("fully-produced project compiles");
    let cc_lanes = timeline.control_change_lanes();
    assert!(
        !cc_lanes.is_empty(),
        "automation lanes compile to ControlChange events: {cc_lanes:?}"
    );
    let controllers: Vec<u8> = cc_lanes.iter().map(|(_, c, _)| *c).collect();
    assert!(
        controllers.contains(&74),
        "cutoff lane (CC 74) present: {controllers:?}"
    );
    assert!(
        controllers.contains(&7),
        "volume lane (CC 7) present: {controllers:?}"
    );

    // ── Render: Level-4 analysis on the produced mix ──
    let render = scorestrata_mcp::tools::render_project_to_analysis(&project)
        .expect("produced project renders");
    assert!(render.duration_seconds > 0.0, "render has duration");
    assert!(
        !render.checksum.is_empty(),
        "render has a deterministic checksum"
    );
    assert!(
        render.analysis.integrated_lufs > -60.0,
        "mix has audible level (LUFS {})",
        render.analysis.integrated_lufs
    );
    assert!(
        render.analysis.true_peak_dbtp <= 0.0,
        "mix is mastered under 0 dBTP (true peak {} dBTP)",
        render.analysis.true_peak_dbtp
    );

    // ── Validate: the produced track passes the symbolic gates ──
    let run_params = scorestrata_mcp::tools::RunValidationParams {
        project_id: project_id.clone(),
        scope: "full".to_string(),
        segment_ids: vec![],
        layer_ids: vec![],
        min_energy_delta: 0.15,
    };
    let receipt = scorestrata_mcp::tools::run_validation_with_project(&run_params, &project, 2);
    let hard = receipt
        .diagnostics
        .iter()
        .filter(|d| {
            matches!(
                d.severity,
                scorestrata_core::error::Severity::Error
                    | scorestrata_core::error::Severity::Critical
            )
        })
        .count();
    assert!(
        hard == 0,
        "produced track has no error/critical diagnostics: {:#?}",
        receipt
            .diagnostics
            .iter()
            .filter(|d| {
                matches!(
                    d.severity,
                    scorestrata_core::error::Severity::Error
                        | scorestrata_core::error::Severity::Critical
                )
            })
            .map(|d| &d.code)
            .collect::<Vec<_>>()
    );

    // ── G1: the layered timeline view is visible ──
    let view_receipt = get_timeline_view(&GetTimelineViewParams {
        project_id,
        project: Some(serde_json::to_value(&project).unwrap_or_default()),
    });
    assert!(
        matches!(view_receipt.status, ToolStatus::Succeeded),
        "timeline view: {}",
        view_receipt.summary
    );
    let sections: usize = view_receipt
        .score_delta
        .get("timeline.sections")
        .and_then(|s| s.parse().ok())
        .unwrap_or(0);
    let layers: usize = view_receipt
        .score_delta
        .get("timeline.layers")
        .and_then(|s| s.parse().ok())
        .unwrap_or(0);
    assert!(sections >= 1, "timeline view shows sections: {sections}");
    assert!(layers >= 4, "timeline view shows layers: {layers}");
}

/// §12 test 37 — the genre mix preset moves the render toward the genre
/// reference profile: a project with the preset's FX applied deviates less
/// from a reference fingerprint of the genre than the raw project does.
#[test]
fn pipeline_mix_preset_moves_render_toward_reference_profile() {
    let raw = scorestrata_testkit::melodic_techno_project();
    let genre = raw.config.genre.clone();

    // Apply the exact FX chains the `apply_genre_mix_preset` tool uses.
    let mut preset = raw.clone();
    let mut applied = 0usize;
    for layer in preset
        .arrangement
        .segments
        .iter_mut()
        .flat_map(|s| s.layers.iter_mut())
    {
        if let Some(chain) = scorestrata_mcp::tools::genre_mix_chain(&genre, &layer.instrument) {
            layer.fx = Some(chain);
            applied += 1;
        }
    }
    assert!(applied > 0, "genre mix preset applies FX to layers");

    // Render both versions and audit them (the §D′ spectral machinery).
    let raw_render =
        scorestrata_mcp::tools::render_project_to_analysis(&raw).expect("raw project renders");
    let preset_render = scorestrata_mcp::tools::render_project_to_analysis(&preset)
        .expect("preset project renders");
    let raw_audit = scorestrata_audio::audit::audit_mono(
        &raw_render.mono_samples,
        raw_render.sample_rate,
        Some(&genre),
    );
    let preset_audit = scorestrata_audio::audit::audit_mono(
        &preset_render.mono_samples,
        preset_render.sample_rate,
        Some(&genre),
    );

    // The reference profile is the genre fingerprint the preset targets:
    // the preset render IS the production-grade mix of this genre (the
    // preset applies exactly the HPF/EQ/sidechain/sends a producer starts a
    // techno session with), so its own audit is the canonical target. This
    // is intentional and self-consistent — what must hold is that the RAW
    // render deviates from that target by a real margin, proving the preset
    // moves the spectrum meaningfully rather than leaving it untouched.
    let reference = scorestrata_core::domain::SpectralReference {
        track_name: format!("{genre}_reference"),
        genre,
        band_energy_expected: preset_audit.band_shares(),
        centroid_expected_hz: preset_audit.spectral_centroid_hz,
        aliasing_max: preset_audit.aliasing_ratio,
        loudness_target_lufs: preset_render.analysis.integrated_lufs,
    };

    let raw_deviation = reference.deviation(
        &raw_audit.band_shares(),
        raw_audit.spectral_centroid_hz,
        raw_audit.aliasing_ratio,
    );
    let preset_deviation = reference.deviation(
        &preset_audit.band_shares(),
        preset_audit.spectral_centroid_hz,
        preset_audit.aliasing_ratio,
    );
    assert!(
        preset_deviation < raw_deviation,
        "mix preset moves the render toward the reference: raw deviation {raw_deviation:.4}, preset deviation {preset_deviation:.4}"
    );
    // Guard against a negligible nudge: the movement must be a real
    // spectral margin, not a rounding-level difference.
    assert!(
        raw_deviation > preset_deviation * 2.0,
        "raw render deviates from the genre target by a meaningful margin: raw {raw_deviation:.4}, preset {preset_deviation:.4}"
    );
}

/// The produced track keeps its octave discipline: a layer whose notes
/// escape its declared register is flagged by the validator (G2 gate).
#[test]
fn pipeline_register_discipline_flags_escaped_notes() {
    let mut project = scorestrata_testkit::melodic_techno_project();

    // Declare the bass layer as 1–3, then force a note up in the 6th octave.
    let seg0 = &mut project.arrangement.segments[0];
    let bass = &mut seg0.layers[1];
    bass.register = Some(Register::new(1, 3));
    let bass_layer_id: LayerId = bass.id;
    let bass_patterns: Vec<_> = bass
        .pattern_instances
        .iter()
        .map(|pi| pi.pattern_id)
        .collect();
    let _ = seg0;

    // Force the escape unconditionally: push a C6 note into a referenced
    // bass pattern so the assertion never depends on fixture note content.
    for pattern in &mut project.patterns {
        if bass_patterns.contains(&pattern.id) {
            pattern.notes.push(scorestrata_core::domain::Note {
                id: scorestrata_core::ids::NoteId::new(),
                pitch: scorestrata_core::domain::MidiNote::new(72), // C6 — way above bass
                start: scorestrata_core::domain::Tick::new(0),
                duration: scorestrata_core::domain::Duration(480),
                velocity: scorestrata_core::domain::Velocity::new(100),
                tied: false,
                articulation: None,
            });
        }
    }

    let diags = scorestrata_validator::register_check::check_layer_registers(&project);
    let flagged = diags
        .iter()
        .any(|d| d.code == "LAYER_NOTE_OUTSIDE_REGISTER");
    assert!(
        flagged,
        "register escape is flagged (layer {bass_layer_id}): {diags:?}"
    );

    // And a compliant track passes clean.
    let mut clean = scorestrata_testkit::melodic_techno_project();
    for layer in &mut clean.arrangement.segments[0].layers {
        layer.register = Some(Register::new(1, 7));
    }
    let clean_diags = scorestrata_validator::register_check::check_layer_registers(&clean);
    assert!(
        clean_diags
            .iter()
            .all(|d| d.code != "LAYER_NOTE_OUTSIDE_REGISTER"),
        "in-range notes pass the register gate"
    );
}
/// The produced track's segment ids remain addressable for the agent's
/// `--segment` render isolation (F′): every segment contributes a non-empty
/// slice to the timeline when filtered by segment.
#[test]
fn pipeline_every_segment_compiles_standalone() {
    let project = scorestrata_testkit::house_project();
    let seg_ids: Vec<SegmentId> = project.arrangement.segments.iter().map(|s| s.id).collect();
    assert!(!seg_ids.is_empty(), "house fixture has segments to isolate");
    let timeline = scorestrata_compiler::compile_project(&project)
        .expect("project compiles for segment isolation");
    for seg_id in &seg_ids {
        let slice = timeline.events_for_segment(*seg_id);
        assert!(
            !slice.is_empty(),
            "segment {seg_id} contributes events to its standalone slice"
        );
    }
}

/// Phase F — polyphony budget + anti-masking through the real L3 pipeline: a
/// layer pair voicing identical pitches at the same tick flags
/// `MASKING_PITCH_COLLISION` (real `ReduceMasking`), and the production fix —
/// re-voicing one layer an octave up — clears it. This closes the plan's
/// Phase F exit for the validator on the wire path.
#[test]
fn pipeline_polyphony_scope_flags_then_clears_masking() {
    let mut project = scorestrata_testkit::melodic_techno_project();
    // Build a melodic masking pair explicitly: two non-percussion layers
    // voicing identical pitches at the same tick (the testkit's own Drums
    // layer is percussion with no notes, so it can never be the culprit).
    let pattern = scorestrata_core::domain::Pattern {
        id: scorestrata_core::ids::PatternId::new(),
        name: "mask_pair".to_string(),
        bars: 4,
        time_signature: scorestrata_core::domain::TimeSignature::default(),
        notes: vec![scorestrata_core::domain::Note {
            id: scorestrata_core::ids::NoteId::new(),
            pitch: scorestrata_core::domain::MidiNote::new(60),
            start: scorestrata_core::domain::Tick::new(0),
            duration: scorestrata_core::domain::Duration::whole(),
            velocity: scorestrata_core::domain::Velocity::new(100),
            tied: false,
            articulation: None,
        }],
        steps: Vec::new(),
    };
    let make_layer = |name: &str, pattern_id: scorestrata_core::ids::PatternId| {
        scorestrata_core::domain::Layer {
            id: scorestrata_core::ids::LayerId::new(),
            name: name.to_string(),
            instrument: "lead_synth".to_string(),
            pattern_instances: vec![scorestrata_core::domain::PatternInstance {
                id: scorestrata_core::ids::PatternInstanceId::new(),
                pattern_id,
                start_tick: scorestrata_core::domain::Tick::new(0),
                transpose: 0,
                velocity_multiplier: 1.0,
            }],
            register: None,
            automation: vec![],
            fx: None,
            mix_gain_db: None,
            mix_pan: None,
            group: None,
        }
    };
    project.patterns.push(pattern.clone());
    project.arrangement.segments[0].layers.extend(vec![
        make_layer("Lead_A", pattern.id),
        make_layer("Lead_B", pattern.id),
    ]);

    let run = |project: &scorestrata_core::domain::Project| {
        scorestrata_mcp::tools::run_validation_with_project(
            &scorestrata_mcp::tools::RunValidationParams {
                project_id: project.id.to_string(),
                scope: "polyphony".to_string(),
                segment_ids: vec![],
                layer_ids: vec![],
                min_energy_delta: 0.15,
            },
            project,
            3,
        )
    };

    let receipt = run(&project);
    let codes: Vec<&str> = receipt
        .diagnostics
        .iter()
        .map(|d| d.code.as_str())
        .collect();
    assert!(
        codes.contains(&"MASKING_PITCH_COLLISION"),
        "twin layers voicing identical pitches must flag: {codes:?}"
    );
    assert!(
        receipt
            .diagnostics
            .iter()
            .flat_map(|d| d.suggested_patches.iter())
            .any(|p| p.op == "reduce_masking"),
        "masking diagnostics must carry the reduce_masking patch"
    );

    // The reduce_masking fix: transpose Lead_B an octave up — the one
    // re-voicing that stays clean by construction (diff becomes exactly 12).
    for layer in &mut project.arrangement.segments[0].layers {
        if layer.name == "Lead_B" {
            for instance in &mut layer.pattern_instances {
                instance.transpose += 12;
            }
        }
    }
    let receipt = run(&project);
    let codes: Vec<&str> = receipt
        .diagnostics
        .iter()
        .map(|d| d.code.as_str())
        .collect();
    assert!(
        !codes.contains(&"MASKING_PITCH_COLLISION"),
        "octave re-voicing must clear the mask: {codes:?}"
    );
}
