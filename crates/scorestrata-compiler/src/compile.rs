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
fn track_id_for_layer(layer_id: &LayerId) -> TrackId {
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

use crate::arrangement::expand_arrangement;
use crate::timeline::{EventKind, TimelineEvent, TimelineIR};

/// Compile a project into a deterministic timeline IR.
///
/// The pipeline:
/// 1. Validate project status is at least ConfigApproved.
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

    let mut events: Vec<TimelineEvent> = Vec::new();
    let mut next_event_id: u64 = 0;

    for expanded_segment in &expanded {
        let segment = &expanded_segment.segment;
        // Segment start in ticks: bars * PPQ * beats-per-bar
        let segment_start_tick = u64::from(expanded_segment.start_bar)
            * u64::from(scorestrata_core::domain::PPQ)
            * u64::from(project.config.time_signature.numerator);

        for layer in &segment.layers {
            for instance in &layer.pattern_instances {
                let pattern = project
                    .patterns
                    .iter()
                    .find(|p| p.id == instance.pattern_id)
                    .ok_or_else(|| ScorestrataError::MissingEntity {
                        entity_type: "pattern".to_string(),
                        entity_id: instance.pattern_id.to_string(),
                    })?;

                let instance_start = u64::from(instance.start_tick.0);
                let transpose = i64::from(instance.transpose);

                // Note-based patterns (melody, bass, pads).
                for note in &pattern.notes {
                    let abs_tick = segment_start_tick + instance_start + note.start.0;
                    // Note pitch 0-127 plus bounded transpose stays in i64 range;
                    // the clamp makes the narrowing cast provably lossless.
                    let pitch = (i64::from(note.pitch.0) + transpose).clamp(0, 127) as u8;

                    let track_id = track_id_for_layer(&layer.id);
                    events.push(TimelineEvent {
                        id: next_event_id,
                        tick: scorestrata_core::domain::Tick::new(abs_tick),
                        kind: EventKind::NoteOn {
                            pitch: scorestrata_core::domain::MidiNote::new(pitch),
                            velocity: note.velocity,
                        },
                        track_id,
                        segment_id: segment.id,
                        layer_id: layer.id,
                        pattern_id: Some(pattern.id),
                    });
                    next_event_id += 1;

                    events.push(TimelineEvent {
                        id: next_event_id,
                        tick: scorestrata_core::domain::Tick::new(abs_tick + note.duration.0),
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
    }

    // Deterministic ordering: tick first, then id for stable tie-breaking.
    events.sort_by_key(|e| (e.tick, e.id));

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
                },
                Note {
                    id: NoteId::new(),
                    pitch: MidiNote::new(63),
                    start: Tick::new(480),
                    duration: Duration::quarter(),
                    velocity: Velocity::new(90),
                    tied: false,
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
