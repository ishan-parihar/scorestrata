//! Project → Timeline IR compilation orchestrator.
//!
//! Wires the Phase 1 exit criteria together: arrangement expansion, pattern
//! instantiation, and timeline IR generation into a single deterministic
//! pipeline. This is the compiler entry point agents and the CLI call.

use scorestrata_core::domain::{Project, ProjectStatus};
use scorestrata_core::error::ScorestrataError;
use scorestrata_core::ids::{LayerId, PatternId, SegmentId, TrackId};

/// Derive a deterministic track ID from a layer ID.
///
/// Track IDs are derived from the layer's UUID so compilation is
/// deterministic: the same project always produces the same timeline
/// (per §17 determinism rules).
#[must_use]
const fn track_id_for_layer(layer_id: &LayerId) -> TrackId {
    TrackId::from_uuid(*layer_id.as_uuid())
}

/// Map a step velocity to a GM drum pitch.
///
/// The planner writes kick/snare/hat steps with distinct velocity tiers;
/// the compiler reconstructs the drum voice from that tier so step patterns
/// compile into real percussion events.
#[must_use]
const fn drum_pitch_for_velocity(velocity: u8) -> u8 {
    match velocity {
        105.. => 36,    // kick (C1)
        95..=104 => 38, // snare (D1)
        _ => 42,        // closed hat (F#1)
    }
}

use crate::arrangement::{ExpandedSegment, expand_arrangement};
use crate::timeline::{EventKind, TimelineEvent, TimelineIR};

/// Generate the timeline events for a single expanded segment.
///
/// Events carry **local** sequential ids (0..n in generation order). The
/// orchestrators — [`compile_project`] and the §7.3 incremental path —
/// renumber them into global ids afterwards, so both emit byte-identical
/// timelines for the same project.
#[expect(
    clippy::too_many_lines,
    reason = "Segment event compilation walks lanes then instances; splitting obscures the ordering invariant"
)]
pub(crate) fn compile_segment_events(
    project: &Project,
    expanded_segment: &ExpandedSegment,
) -> Result<Vec<TimelineEvent>, ScorestrataError> {
    let segment = &expanded_segment.segment;
    // Segment start in ticks: bars * PPQ * beats-per-bar
    let segment_start_tick = u64::from(expanded_segment.start_bar)
        * u64::from(scorestrata_core::domain::PPQ)
        * u64::from(project.config.time_signature.numerator);

    let mut events: Vec<TimelineEvent> = Vec::new();
    let mut next_event_id: u64 = 0;

    for layer in &segment.layers {
        // G4 — emit automation lanes as ControlChange events (segment-relative
        // ticks, matching pattern-instance placement). Each lane is linearly
        // interpolated by the audio renderer when sweeping the parameter.
        for lane in &layer.automation {
            for point in &lane.points {
                let abs_tick = segment_start_tick + point.tick.0;
                events.push(TimelineEvent {
                    id: next_event_id,
                    tick: scorestrata_core::domain::Tick::new(abs_tick),
                    kind: EventKind::ControlChange {
                        controller: lane.param.cc_number(),
                        value: lane.cc_value_at(point.tick),
                    },
                    track_id: track_id_for_layer(&layer.id),
                    segment_id: segment.id,
                    layer_id: layer.id,
                    pattern_id: None,
                });
                next_event_id += 1;
            }
        }
        for instance in &layer.pattern_instances {
            let pattern = project
                .patterns
                .iter()
                .find(|p| p.id == instance.pattern_id)
                .ok_or_else(|| ScorestrataError::MissingEntity {
                    entity_type: "pattern".to_string(),
                    entity_id: instance.pattern_id.to_string(),
                })?;

            let instance_start = instance.start_tick.0;
            let transpose = i64::from(instance.transpose);

            // Note-based patterns (melody, bass, pads).
            for note in &pattern.notes {
                let abs_tick = segment_start_tick + instance_start + note.start.0;
                // Note pitch 0-127 plus bounded transpose stays in i64 range;
                // the clamp makes the narrowing cast provably lossless.
                #[expect(
                    clippy::cast_sign_loss,
                    reason = "Pitch clamped to 0-127 before narrowing to u8"
                )]
                let mut pitch = (i64::from(note.pitch.0) + transpose).clamp(0, 127) as u8;
                // G2 — enforce the layer's declared octave register: notes
                // outside it are clamped into the band so a transposed
                // pattern can never spill into a neighboring layer's
                // register (the production-grade octave discipline).
                if let Some(register) = &layer.register {
                    pitch = register
                        .clamp_note(scorestrata_core::domain::MidiNote::new(pitch))
                        .0;
                }

                // Phase C — resolve per-note articulation into the emitted
                // events (accent → velocity boost + gate cut; staccato →
                // short gate; legato → full gate; portamento → glide CC
                // curve; pedal → sustain CC). Unarticulated notes pass
                // through with their written values (R1).
                let mut emit_cc = |cc: u8, value: u8| {
                    events.push(TimelineEvent {
                        id: next_event_id,
                        tick: scorestrata_core::domain::Tick::new(abs_tick),
                        kind: EventKind::ControlChange {
                            controller: cc,
                            value,
                        },
                        track_id: track_id_for_layer(&layer.id),
                        segment_id: segment.id,
                        layer_id: layer.id,
                        pattern_id: Some(pattern.id),
                    });
                    next_event_id += 1;
                };
                let (velocity, gate_ticks) = articulation_resolution(note, &mut emit_cc);

                let track_id = track_id_for_layer(&layer.id);
                events.push(TimelineEvent {
                    id: next_event_id,
                    tick: scorestrata_core::domain::Tick::new(abs_tick),
                    kind: EventKind::NoteOn {
                        pitch: scorestrata_core::domain::MidiNote::new(pitch),
                        velocity,
                    },
                    track_id,
                    segment_id: segment.id,
                    layer_id: layer.id,
                    pattern_id: Some(pattern.id),
                });
                next_event_id += 1;

                events.push(TimelineEvent {
                    id: next_event_id,
                    tick: scorestrata_core::domain::Tick::new(abs_tick + gate_ticks),
                    kind: EventKind::NoteOff {
                        pitch: scorestrata_core::domain::MidiNote::new(pitch),
                    },
                    track_id,
                    segment_id: segment.id,
                    layer_id: layer.id,
                    pattern_id: Some(pattern.id),
                });
                next_event_id += 1;
            }

            // Step-sequencer patterns (drums): each active step becomes
            // a 16th-note hit. The GM drum pitch is inferred from the
            // step velocity tier set by the planner (kick=110, snare=100,
            // hat=85); steps without a tier map to a closed hat.
            if !pattern.steps.is_empty() {
                let sixteenth = u64::from(scorestrata_core::domain::PPQ) / 4;
                for step in &pattern.steps {
                    if !step.active {
                        continue;
                    }
                    let abs_tick =
                        segment_start_tick + instance_start + u64::from(step.index) * sixteenth;
                    let pitch = drum_pitch_for_velocity(step.velocity.0);
                    let track_id = track_id_for_layer(&layer.id);
                    events.push(TimelineEvent {
                        id: next_event_id,
                        tick: scorestrata_core::domain::Tick::new(abs_tick),
                        kind: EventKind::NoteOn {
                            pitch: scorestrata_core::domain::MidiNote::new(pitch),
                            velocity: step.velocity,
                        },
                        track_id,
                        segment_id: segment.id,
                        layer_id: layer.id,
                        pattern_id: Some(pattern.id),
                    });
                    next_event_id += 1;

                    events.push(TimelineEvent {
                        id: next_event_id,
                        tick: scorestrata_core::domain::Tick::new(abs_tick + sixteenth),
                        kind: EventKind::NoteOff {
                            pitch: scorestrata_core::domain::MidiNote::new(pitch),
                        },
                        track_id,
                        segment_id: segment.id,
                        layer_id: layer.id,
                        pattern_id: Some(pattern.id),
                    });
                    next_event_id += 1;
                }
            }
        }
    }

    Ok(events)
}

/// Resolve a note's optional articulation into `(velocity, gate_ticks)`.
///
/// Side-channel CC events (pitch-bend for portamento, sustain for pedal) are
/// emitted through `emit_cc` at the note's start tick. Unarticulated notes
/// return the written velocity and duration unchanged (R1).
fn articulation_resolution<F: FnMut(u8, u8)>(
    note: &scorestrata_core::domain::Note,
    emit_cc: &mut F,
) -> (scorestrata_core::domain::Velocity, u64) {
    use scorestrata_core::domain::Articulation;
    match note.articulation {
        None | Some(Articulation::Legato) => (note.velocity, note.duration.0),
        Some(Articulation::Accent) => {
            let boosted =
                scorestrata_core::domain::Velocity::new(note.velocity.0.saturating_add(24));
            (boosted, note.duration.0 * 9 / 10)
        }
        Some(Articulation::Staccato) => (note.velocity, note.duration.0 / 2),
        Some(Articulation::Portamento {
            semitones,
            glide_ms,
        }) => {
            // Pitch-bend send (CC 84) opening the glide, then its depth.
            emit_cc(84, 127);
            #[expect(
                clippy::cast_sign_loss,
                reason = "Depth clamped to 0-127 before narrowing to u8"
            )]
            let depth = (semitones.clamp(-24, 24) + 64).clamp(0, 127) as u8;
            emit_cc(85, depth);
            let _ = glide_ms;
            (note.velocity, note.duration.0)
        }
        Some(Articulation::Pedal) => {
            // Sustain (CC 64) down at the note start; released by the
            // renderer at the note end.
            emit_cc(64, 127);
            (note.velocity, note.duration.0 * 12 / 10)
        }
    }
}

/// Compile a project into a deterministic timeline IR.
///
/// The pipeline:
/// 1. Validate project status is at least `ConfigApproved`.
/// 2. Expand the arrangement into concrete segment boundaries.
/// 3. Instantiate every pattern instance into absolute-tick NoteOn/NoteOff events.
/// 4. Sort events by tick (deterministic ordering).
/// 5. Compute the input hash and total duration.
///
/// # Errors
///
/// Returns an error if the arrangement is invalid, a referenced pattern is
/// missing, or the project status is too early for compilation.
pub fn compile_project(project: &Project) -> Result<TimelineIR, ScorestrataError> {
    use ProjectStatus as S;
    if matches!(
        project.status,
        S::BriefReceived | S::ConfigDrafted | S::ConfigValidationFailed
    ) {
        return Err(ScorestrataError::CompilerError {
            pass: "compile_project".to_string(),
            message: format!(
                "Project status {:?} is too early to compile (requires ConfigApproved or later)",
                project.status
            ),
        });
    }

    let expanded = expand_arrangement(&project.arrangement)?;

    // Compile each segment independently, then renumber into global ids in
    // traversal order. The §7.3 incremental path reuses this exact helper,
    // so a cached compile is byte-identical to a full one.
    let mut events: Vec<TimelineEvent> = Vec::new();
    let mut next_event_id: u64 = 0;

    for expanded_segment in &expanded {
        let mut segment_events = compile_segment_events(project, expanded_segment)?;
        for event in &mut segment_events {
            event.id = next_event_id;
            next_event_id += 1;
        }
        events.extend(segment_events);
    }

    // Deterministic ordering: tick first, then id for stable tie-breaking.
    events.sort_by_key(|e| (e.tick, e.id));

    // Phase C — performance layer: groove → humanize. Applied to the flat IR
    // before hashing so the manifest captures the performed timeline (R4).
    // Disabled (or `None` on the project) is a no-op: byte-identical to the
    // un-humanized render (R1).
    if let Some(spec) = &project.humanize {
        let seed = spec.seed.unwrap_or_else(|| content_seed(&events));
        crate::performance::apply_performance_passes(&mut events, spec, seed, project.config.bpm);
        events.sort_by_key(|e| (e.tick, e.id));
    }

    let total_ticks = events.iter().map(|e| e.tick.0).max().unwrap_or(0);

    let mut timeline = TimelineIR {
        project_id: project.id,
        events,
        total_ticks,
        bpm: project.config.bpm,
        time_signature_num: project.config.time_signature.numerator,
        input_hash: String::new(),
        compiler_version: env!("CARGO_PKG_VERSION").to_string(),
    };

    timeline.input_hash = timeline.content_hash();
    Ok(timeline)
}

/// Derive a deterministic project seed from the pre-performance event
/// content (used when `HumanizeSpec.seed` is `None`).
///
/// Stable across runs, unique per project: it hashes the grid ticks, pitches,
/// velocities, and layer ids before any performance jitter is applied, so the
/// same project always produces the same performance.
fn content_seed(events: &[TimelineEvent]) -> u64 {
    let mut h: u64 = 0xCBF2_9CE4_8422_2325;
    for e in events {
        h = (h ^ e.tick.0).wrapping_mul(0x1000_0000_01B3);
        // Fold the layer UUID so two layers with identical note content still
        // seed differently, and the same project always seeds the same.
        for b in e.layer_id.to_string().as_bytes() {
            h = (h ^ u64::from(*b)).wrapping_mul(0x1000_0000_01B3);
        }
        match &e.kind {
            EventKind::NoteOn { pitch, velocity } => {
                h = (h ^ u64::from(pitch.0)).wrapping_mul(0x1000_0000_01B3);
                h = (h ^ u64::from(velocity.0)).wrapping_mul(0x1000_0000_01B3);
            }
            EventKind::NoteOff { pitch } => {
                h = (h ^ u64::from(pitch.0) ^ 0xFFFF).wrapping_mul(0x1000_0000_01B3);
            }
            _ => {}
        }
    }
    h
}

/// Compile only events for a specific segment.
///
/// Returns a timeline containing just that segment's events, with the
/// segment-relative origin preserved (ticks are still absolute for
/// deterministic comparison, matching the full-timeline representation).
#[must_use]
pub fn compile_segment(project: &Project, segment_id: SegmentId) -> Option<TimelineIR> {
    let full = compile_project(project).ok()?;
    let events: Vec<TimelineEvent> = full
        .events
        .into_iter()
        .filter(|e| e.segment_id == segment_id)
        .collect();
    let total_ticks = events.iter().map(|e| e.tick.0).max().unwrap_or(0);

    Some(TimelineIR {
        project_id: full.project_id,
        events,
        total_ticks,
        bpm: full.bpm,
        time_signature_num: full.time_signature_num,
        input_hash: full.input_hash,
        compiler_version: full.compiler_version,
    })
}

/// Compile only events for a specific layer (across all segments).
#[must_use]
pub fn compile_layer(project: &Project, layer_id: LayerId) -> Option<TimelineIR> {
    let full = compile_project(project).ok()?;
    let events: Vec<TimelineEvent> = full
        .events
        .into_iter()
        .filter(|e| e.layer_id == layer_id)
        .collect();
    let total_ticks = events.iter().map(|e| e.tick.0).max().unwrap_or(0);

    Some(TimelineIR {
        project_id: full.project_id,
        events,
        total_ticks,
        bpm: full.bpm,
        time_signature_num: full.time_signature_num,
        input_hash: full.input_hash,
        compiler_version: full.compiler_version,
    })
}

/// Compile only events for a specific pattern (all instances).
#[must_use]
pub fn compile_pattern(project: &Project, pattern_id: PatternId) -> Option<TimelineIR> {
    let full = compile_project(project).ok()?;
    let events: Vec<TimelineEvent> = full
        .events
        .into_iter()
        .filter(|e| e.pattern_id == Some(pattern_id))
        .collect();
    let total_ticks = events.iter().map(|e| e.tick.0).max().unwrap_or(0);

    Some(TimelineIR {
        project_id: full.project_id,
        events,
        total_ticks,
        bpm: full.bpm,
        time_signature_num: full.time_signature_num,
        input_hash: full.input_hash,
        compiler_version: full.compiler_version,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use scorestrata_core::domain::{
        Arrangement, Brief, DeliveryTarget, Duration, Layer, MidiNote, Note, Pattern,
        ProjectConfig, Segment, SegmentRole, Tick, TimeSignature, Velocity,
    };
    use scorestrata_core::ids::{NoteId, PatternInstanceId, ProjectId};

    fn build_project() -> Project {
        let pattern = Pattern {
            id: PatternId::new(),
            name: "lead".to_string(),
            bars: 1,
            time_signature: TimeSignature::default(),
            notes: vec![
                Note {
                    id: NoteId::new(),
                    pitch: MidiNote::new(60),
                    start: Tick::new(0),
                    duration: Duration::quarter(),
                    velocity: Velocity::new(100),
                    tied: false,
                    articulation: None,
                },
                Note {
                    id: NoteId::new(),
                    pitch: MidiNote::new(63),
                    start: Tick::new(480),
                    duration: Duration::quarter(),
                    velocity: Velocity::new(90),
                    tied: false,
                    articulation: None,
                },
            ],
            steps: Vec::new(),
        };

        let layer = Layer {
            id: LayerId::new(),
            name: "Lead".to_string(),
            instrument: "synth".to_string(),
            pattern_instances: vec![scorestrata_core::domain::PatternInstance {
                id: PatternInstanceId::new(),
                pattern_id: pattern.id,
                start_tick: Tick::new(0),
                transpose: 0,
                velocity_multiplier: 1.0,
            }],
            register: None,
            automation: vec![],
            fx: None,
            mix_gain_db: None,
            mix_pan: None,
            group: None,
        };

        Project {
            id: ProjectId::new(),
            name: "Test".to_string(),
            created_at: chrono::Utc::now(),
            modified_at: chrono::Utc::now(),
            brief: Brief {
                raw_text: "test".to_string(),
                genre: None,
                mood: vec![],
                references: vec![],
                delivery_target: None,
                constraints: vec![],
                confidence: 0.5,
            },
            config: ProjectConfig {
                name: "Test".to_string(),
                genre: "techno".to_string(),
                bpm: 124,
                key: "C minor".to_string(),
                time_signature: TimeSignature::default(),
                duration_bars: 8,
                delivery_target: DeliveryTarget::Streaming,
                mood: vec![],
                references: vec![],
            },
            arrangement: Arrangement {
                total_bars: 8,
                time_signature: TimeSignature::default(),
                segments: vec![Segment {
                    id: SegmentId::new(),
                    name: "Loop".to_string(),
                    role: SegmentRole::Chorus,
                    start_bar: 0,
                    bars: 8,
                    layers: vec![layer],
                }],
                energy_curve: Vec::new(),
            },
            tracks: vec![],
            patterns: vec![pattern],
            spectral_reference: None,
            mixer: scorestrata_core::domain::MixerState::default(),
            sound_bank: None,
            melodic_bank: None,
            bus_graph: None,
            humanize: None,
            master_chain: None,
            status: ProjectStatus::ConfigApproved,
        }
    }

    #[test]
    fn compiles_two_notes_per_pattern() {
        let project = build_project();
        #[expect(clippy::unwrap_used, reason = "Test assertions")]
        let timeline = compile_project(&project).unwrap();
        // 2 notes × 2 events (on+off)
        assert_eq!(timeline.event_count(), 4);
        assert_eq!(timeline.bpm, 124);
        assert!(timeline.total_ticks >= 960);
        assert!(!timeline.input_hash.is_empty());
    }

    #[test]
    fn compiles_deterministically() {
        let project = build_project();
        #[expect(clippy::unwrap_used, reason = "Test assertions")]
        let a = compile_project(&project).unwrap();
        #[expect(clippy::unwrap_used, reason = "Test assertions")]
        let b = compile_project(&project).unwrap();
        assert_eq!(a.content_hash(), b.content_hash());
    }

    #[test]
    fn events_are_sorted_by_tick() {
        let project = build_project();
        #[expect(clippy::unwrap_used, reason = "Test assertions")]
        let timeline = compile_project(&project).unwrap();
        assert!(timeline.events.windows(2).all(|w| w[0].tick <= w[1].tick));
    }

    #[test]
    fn automation_lanes_emit_control_change_events() {
        let mut project = build_project();
        let layer = &mut project.arrangement.segments[0].layers[0];
        layer.automation.push(scorestrata_core::domain::Automation {
            layer_id: layer.id,
            param: scorestrata_core::domain::AutomationParam::Cutoff,
            points: vec![
                scorestrata_core::domain::AutomationPoint {
                    tick: Tick::new(0),
                    value: 0.2,
                },
                scorestrata_core::domain::AutomationPoint {
                    tick: Tick::new(480),
                    value: 1.0,
                },
            ],
        });
        #[expect(clippy::unwrap_used, reason = "Test assertions")]
        let timeline = compile_project(&project).unwrap();
        let ccs: Vec<&TimelineEvent> = timeline
            .events
            .iter()
            .filter(|e| matches!(e.kind, EventKind::ControlChange { .. }))
            .collect();
        assert_eq!(ccs.len(), 2, "one CC per automation point");
        for cc in &ccs {
            let EventKind::ControlChange { controller, value } = cc.kind else {
                unreachable!();
            };
            assert_eq!(controller, 74, "cutoff maps to CC 74");
            assert!(value <= 127);
        }
        // The lane is exposed for renderers via the grouped helper.
        let lanes = timeline.control_change_lanes();
        assert_eq!(lanes.len(), 1);
        let (_, controller, points) = &lanes[0];
        assert_eq!(*controller, 74);
        assert_eq!(points.len(), 2);
    }

    #[test]
    fn layer_register_clamps_out_of_range_notes() {
        // G2 — the compiler enforces the layer's declared octave register:
        // notes pushed outside the band by transposition are clamped back
        // into it, so a transposed pattern can never spill into a neighbor-
        // ing layer's register (§1.3 octave discipline).
        let mut project = build_project();
        let layer = &mut project.arrangement.segments[0].layers[0];
        layer.register = Some(scorestrata_core::domain::Register::new(4, 5));
        // +24 = two octaves up: 60 (C4) → 84 and 63 → 87, both above octave 5.
        project.arrangement.segments[0].layers[0].pattern_instances[0].transpose = 24;

        #[expect(clippy::unwrap_used, reason = "Test assertions")]
        let timeline = compile_project(&project).unwrap();

        let note_ons: Vec<scorestrata_core::domain::MidiNote> = timeline
            .events
            .iter()
            .filter_map(|e| match e.kind {
                EventKind::NoteOn { pitch, .. } => Some(pitch),
                _ => None,
            })
            .collect();
        assert_eq!(note_ons.len(), 2, "both notes compile");
        assert!(
            note_ons.iter().all(|p| (4..=5).contains(&p.octave())),
            "all notes clamped into octave 4..5: {note_ons:?}"
        );
        // 84 (C6) clamps down to 72 (C5); 87 clamps to 75 — pitch class
        // preserved while the octave is pulled back into the band.
        assert!(note_ons.contains(&scorestrata_core::domain::MidiNote::new(72)));
        assert!(note_ons.contains(&scorestrata_core::domain::MidiNote::new(75)));
    }

    #[test]
    fn rejects_early_project_status() {
        let mut project = build_project();
        project.status = ProjectStatus::BriefReceived;
        assert!(compile_project(&project).is_err());
    }

    #[test]
    fn compile_segment_filters_events() {
        let project = build_project();
        let segment_id = project.arrangement.segments[0].id;
        #[expect(clippy::unwrap_used, reason = "Test assertions")]
        let seg = compile_segment(&project, segment_id).unwrap();
        assert_eq!(seg.event_count(), 4);
    }

    #[test]
    fn compile_layer_filters_events() {
        let project = build_project();
        let layer_id = project.arrangement.segments[0].layers[0].id;
        #[expect(clippy::unwrap_used, reason = "Test assertions")]
        let layer = compile_layer(&project, layer_id).unwrap();
        assert_eq!(layer.event_count(), 4);
    }

    #[test]
    fn compile_pattern_filters_events() {
        let project = build_project();
        let pattern_id = project.patterns[0].id;
        #[expect(clippy::unwrap_used, reason = "Test assertions")]
        let pattern = compile_pattern(&project, pattern_id).unwrap();
        assert_eq!(pattern.event_count(), 4);
    }
}
