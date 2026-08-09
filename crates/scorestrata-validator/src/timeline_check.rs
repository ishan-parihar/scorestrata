//! Timeline-level validation checks (Level 3).
//!
//! Per architecture §11.2 L3, timeline validation inspects the arrangement
//! as a whole (not just isolated segments): pattern instances must resolve,
//! required sections must not be left empty, notes must not overlap
//! illegitimately, automation must stay in range, and absolute timing must
//! fit inside pattern boundaries.

use scorestrata_core::diagnostic::{Diagnostic, MetricScope};
use scorestrata_core::domain::{PPQ, Project, SegmentRole};
use scorestrata_core::error::Severity;
use scorestrata_core::ids::DiagnosticId;

/// Roles that a fully-arranged track must not leave empty.
const REQUIRED_ROLES: [SegmentRole; 4] = [
    SegmentRole::Verse,
    SegmentRole::Chorus,
    SegmentRole::Build,
    SegmentRole::Bridge,
];

/// Run all Level 3 timeline checks against a project.
///
/// Returns every `TIMELINE_*` diagnostic found across the five check
/// categories: pattern resolution, required-segment fill, note overlap,
/// automation range, and absolute timing.
#[must_use]
pub fn check_timeline(project: &Project) -> Vec<Diagnostic> {
    let mut diagnostics = Vec::new();
    diagnostics.extend(check_unresolved_patterns(project));
    diagnostics.extend(check_required_segment_fill(project));
    diagnostics.extend(check_overlapping_notes(project));
    diagnostics.extend(check_automation_range(project));
    diagnostics.extend(check_layer_automation(project));
    diagnostics.extend(check_layer_fx(project));
    diagnostics.extend(check_absolute_timing(project));
    diagnostics
}

/// Check 1 — every pattern instance must reference a defined pattern.
fn check_unresolved_patterns(project: &Project) -> Vec<Diagnostic> {
    let mut diagnostics = Vec::new();
    for segment in &project.arrangement.segments {
        for layer in &segment.layers {
            for instance in &layer.pattern_instances {
                if !project
                    .patterns
                    .iter()
                    .any(|pattern| pattern.id == instance.pattern_id)
                {
                    diagnostics.push(Diagnostic {
                        id: DiagnosticId::new(),
                        code: "TIMELINE_UNRESOLVED_PATTERN".to_string(),
                        metric: "timeline.pattern_resolution".to_string(),
                        severity: Severity::Error,
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
                            "Layer '{}' in segment '{}' references pattern '{}' which is not defined",
                            layer.name, segment.name, instance.pattern_id
                        ),
                        root_causes: vec!["pattern instance points at a missing pattern".to_string()],
                        suggested_patches: vec![],
                        expected_delta: None,
                        confidence: 1.0,
                    });
                }
            }
        }
    }
    diagnostics
}

/// Check 2 — required (core) segments must contain at least one pattern.
fn check_required_segment_fill(project: &Project) -> Vec<Diagnostic> {
    let mut diagnostics = Vec::new();
    for segment in &project.arrangement.segments {
        if !REQUIRED_ROLES.contains(&segment.role) {
            continue;
        }
        let has_content = segment
            .layers
            .iter()
            .any(|layer| !layer.pattern_instances.is_empty());
        if !has_content {
            diagnostics.push(Diagnostic {
                id: DiagnosticId::new(),
                code: "TIMELINE_EMPTY_REQUIRED_SEGMENT".to_string(),
                metric: "timeline.required_segment_fill".to_string(),
                severity: Severity::Error,
                scope: MetricScope {
                    segment_id: Some(segment.id),
                    layer_id: None,
                    bar_range: None,
                },
                evidence_uris: vec![format!("project://segments/{}", segment.id)],
                message: format!(
                    "Required segment '{}' ({:?}) has no pattern instances",
                    segment.name, segment.role
                ),
                root_causes: vec!["core section left empty in the arrangement".to_string()],
                suggested_patches: vec![],
                expected_delta: None,
                confidence: 0.9,
            });
        }
    }
    diagnostics
}

/// Check 3 — same-pitch notes that overlap in time (and are not tied) are
/// invalid events. Overlaps at different pitches are chords and stay valid.
fn check_overlapping_notes(project: &Project) -> Vec<Diagnostic> {
    let mut diagnostics = Vec::new();
    for pattern in &project.patterns {
        for (index, first) in pattern.notes.iter().enumerate() {
            for second in pattern.notes.iter().skip(index + 1) {
                let same_pitch = first.pitch.0 == second.pitch.0;
                let overlaps = first.start.0 < second.start.0.saturating_add(second.duration.0)
                    && second.start.0 < first.start.0.saturating_add(first.duration.0);
                if same_pitch && overlaps && !first.tied && !second.tied {
                    diagnostics.push(Diagnostic {
                        id: DiagnosticId::new(),
                        code: "TIMELINE_OVERLAPPING_NOTES".to_string(),
                        metric: "timeline.note_overlap".to_string(),
                        severity: Severity::Warning,
                        scope: MetricScope {
                            segment_id: None,
                            layer_id: None,
                            bar_range: None,
                        },
                        evidence_uris: vec![format!("project://patterns/{}", pattern.id)],
                        message: format!(
                            "Pattern '{}' has overlapping notes at pitch {} (ticks {} and {})",
                            pattern.name, first.pitch.0, first.start.0, second.start.0
                        ),
                        root_causes: vec!["illegitimate same-pitch note overlap".to_string()],
                        suggested_patches: vec![],
                        expected_delta: None,
                        confidence: 0.8,
                    });
                }
            }
        }
    }
    diagnostics
}

/// Check 4a — layer automation lanes (G4) must stay within 0.0–1.0 and have
/// at least one point (an empty lane is dead weight the agent should remove).
fn check_layer_automation(project: &Project) -> Vec<Diagnostic> {
    let mut diagnostics = Vec::new();
    for segment in &project.arrangement.segments {
        for layer in &segment.layers {
            for lane in &layer.automation {
                if lane.points.is_empty() {
                    diagnostics.push(Diagnostic {
                        id: DiagnosticId::new(),
                        code: "TIMELINE_AUTOMATION_EMPTY_LANE".to_string(),
                        metric: "timeline.automation_range".to_string(),
                        severity: Severity::Warning,
                        scope: MetricScope {
                            segment_id: Some(segment.id),
                            layer_id: Some(layer.id),
                            bar_range: None,
                        },
                        evidence_uris: vec![format!(
                            "project://{}/segments/{}/layers/{}",
                            project.id, segment.id, layer.id
                        )],
                        message: format!(
                            "Automation lane {:?} on layer '{}' has no points (empty lane)",
                            lane.param, layer.name
                        ),
                        root_causes: vec!["automation lane has no points".to_string()],
                        suggested_patches: vec![],
                        expected_delta: None,
                        confidence: 1.0,
                    });
                    continue;
                }
                for point in &lane.points {
                    if !(0.0..=1.0).contains(&point.value) {
                        diagnostics.push(Diagnostic {
                            id: DiagnosticId::new(),
                            code: "TIMELINE_AUTOMATION_OUT_OF_RANGE".to_string(),
                            metric: "timeline.automation_range".to_string(),
                            severity: Severity::Error,
                            scope: MetricScope {
                                segment_id: Some(segment.id),
                                layer_id: Some(layer.id),
                                bar_range: None,
                            },
                            evidence_uris: vec![format!(
                                "project://{}/segments/{}/layers/{}",
                                project.id, segment.id, layer.id
                            )],
                            message: format!(
                                "Automation {:?} point on layer '{}' is {:.2} (expected 0.0–1.0)",
                                lane.param, layer.name, point.value
                            ),
                            root_causes: vec![
                                "automation value outside its valid range".to_string(),
                            ],
                            suggested_patches: vec![],
                            expected_delta: None,
                            confidence: 1.0,
                        });
                    }
                }
            }
        }
    }
    diagnostics
}

/// Check 4b — FX chain send levels (G5) must stay within 0.0–1.0 and HPF
/// cutoffs must be non-negative (sane insert chains).
#[expect(
    clippy::too_many_lines,
    reason = "FX insert-chain validation enumerates every segment/layer; splitting would fragment the diagnostic walk"
)]
fn check_layer_fx(project: &Project) -> Vec<Diagnostic> {
    let mut diagnostics = Vec::new();
    for segment in &project.arrangement.segments {
        for layer in &segment.layers {
            let Some(fx) = &layer.fx else {
                continue;
            };
            let mut bad_sends = false;
            let mut bad_hpf = false;
            if fx.sends.reverb < 0.0
                || fx.sends.reverb > 1.0
                || fx.sends.delay < 0.0
                || fx.sends.delay > 1.0
            {
                bad_sends = true;
            }
            if fx
                .high_pass_hz
                .is_some_and(|hz| hz < 0.0 || !hz.is_finite())
            {
                bad_hpf = true;
            }
            for band in &fx.eq {
                if band.gain_db.abs() > 24.0 || band.freq_hz <= 0.0 {
                    diagnostics.push(Diagnostic {
                        id: DiagnosticId::new(),
                        code: "TIMELINE_FX_EQ_OUT_OF_RANGE".to_string(),
                        metric: "timeline.fx_eq".to_string(),
                        severity: Severity::Warning,
                        scope: MetricScope {
                            segment_id: Some(segment.id),
                            layer_id: Some(layer.id),
                            bar_range: None,
                        },
                        evidence_uris: vec![format!(
                            "project://{}/segments/{}/layers/{}",
                            project.id, segment.id, layer.id
                        )],
                        message: format!(
                            "Layer '{}' EQ band {:.0} Hz has gain {:.1} dB (sane range ±24 dB)",
                            layer.name, band.freq_hz, band.gain_db
                        ),
                        root_causes: vec!["EQ band outside the sane ±24 dB range".to_string()],
                        suggested_patches: vec![],
                        expected_delta: None,
                        confidence: 1.0,
                    });
                }
            }
            if bad_sends {
                diagnostics.push(Diagnostic {
                    id: DiagnosticId::new(),
                    code: "TIMELINE_FX_SENDS_OUT_OF_RANGE".to_string(),
                    metric: "timeline.fx_sends".to_string(),
                    severity: Severity::Error,
                    scope: MetricScope {
                        segment_id: Some(segment.id),
                        layer_id: Some(layer.id),
                        bar_range: None,
                    },
                    evidence_uris: vec![format!(
                        "project://{}/segments/{}/layers/{}",
                        project.id, segment.id, layer.id
                    )],
                    message: format!(
                        "Layer '{}' send levels reverb={} delay={} (expected 0.0–1.0)",
                        layer.name, fx.sends.reverb, fx.sends.delay
                    ),
                    root_causes: vec!["send level outside its valid range".to_string()],
                    suggested_patches: vec![],
                    expected_delta: None,
                    confidence: 1.0,
                });
            }
            if bad_hpf {
                diagnostics.push(Diagnostic {
                    id: DiagnosticId::new(),
                    code: "TIMELINE_FX_HPF_OUT_OF_RANGE".to_string(),
                    metric: "timeline.fx_hpf".to_string(),
                    severity: Severity::Error,
                    scope: MetricScope {
                        segment_id: Some(segment.id),
                        layer_id: Some(layer.id),
                        bar_range: None,
                    },
                    evidence_uris: vec![format!(
                        "project://{}/segments/{}/layers/{}",
                        project.id, segment.id, layer.id
                    )],
                    message: format!(
                        "Layer '{}' HPF cutoff {:?} is invalid (needs ≥ 0 Hz)",
                        layer.name, fx.high_pass_hz
                    ),
                    root_causes: vec!["HPF cutoff out of range".to_string()],
                    suggested_patches: vec![],
                    expected_delta: None,
                    confidence: 1.0,
                });
            }
        }
    }
    diagnostics
}

/// Check 4 — energy automation must stay within its 0.0–1.0 range.
fn check_automation_range(project: &Project) -> Vec<Diagnostic> {
    let mut diagnostics = Vec::new();
    for point in &project.arrangement.energy_curve {
        if !(0.0..=1.0).contains(&point.target_energy) {
            diagnostics.push(Diagnostic {
                id: DiagnosticId::new(),
                code: "TIMELINE_AUTOMATION_OUT_OF_RANGE".to_string(),
                metric: "timeline.automation_range".to_string(),
                severity: Severity::Error,
                scope: MetricScope {
                    segment_id: Some(point.segment_id),
                    layer_id: None,
                    bar_range: None,
                },
                evidence_uris: vec!["project://arrangement/energy".to_string()],
                message: format!(
                    "Energy automation for segment {} is {:.2} (expected 0.0–1.0)",
                    point.segment_id, point.target_energy
                ),
                root_causes: vec!["automation value outside its valid range".to_string()],
                suggested_patches: vec![],
                expected_delta: None,
                confidence: 1.0,
            });
        }
    }
    diagnostics
}

/// Check 5 — no note may end beyond its pattern's absolute length.
///
/// Pattern length follows the core convention `bars * (PPQ * numerator)`
/// ticks per bar.
fn check_absolute_timing(project: &Project) -> Vec<Diagnostic> {
    let mut diagnostics = Vec::new();
    for pattern in &project.patterns {
        let ticks_per_bar = u64::from(PPQ) * u64::from(pattern.time_signature.numerator);
        let pattern_ticks = u64::from(pattern.bars).saturating_mul(ticks_per_bar);
        for note in &pattern.notes {
            let end = note.start.0.saturating_add(note.duration.0);
            if end > pattern_ticks {
                diagnostics.push(Diagnostic {
                    id: DiagnosticId::new(),
                    code: "TIMELINE_NOTE_OUT_OF_BOUNDS".to_string(),
                    metric: "timeline.absolute_timing".to_string(),
                    severity: Severity::Warning,
                    scope: MetricScope {
                        segment_id: None,
                        layer_id: None,
                        bar_range: None,
                    },
                    evidence_uris: vec![format!("project://patterns/{}", pattern.id)],
                    message: format!(
                        "Pattern '{}' has a note ending at tick {} beyond its {} bars ({} ticks)",
                        pattern.name, end, pattern.bars, pattern_ticks
                    ),
                    root_causes: vec!["note placed past the pattern's absolute end".to_string()],
                    suggested_patches: vec![],
                    expected_delta: None,
                    confidence: 0.8,
                });
            }
        }
    }
    diagnostics
}

#[cfg(test)]
#[expect(
    clippy::expect_used,
    reason = "Tests unwrap fixture timelines and patterns"
)]
mod tests {
    use super::*;
    use scorestrata_core::domain::{Duration, Note, PatternInstance, Tick, Velocity};
    use scorestrata_core::ids::{NoteId, PatternId, PatternInstanceId, SegmentId};

    fn codes(diagnostics: &[Diagnostic]) -> Vec<&str> {
        diagnostics.iter().map(|d| d.code.as_str()).collect()
    }

    #[test]
    fn clean_project_produces_no_timeline_diagnostics() {
        let project = scorestrata_testkit::melodic_techno_project();
        assert!(
            check_timeline(&project).is_empty(),
            "clean fixture should pass timeline: {:?}",
            codes(&check_timeline(&project))
        );
    }

    #[test]
    fn fx_sends_out_of_range_are_flagged() {
        let mut project = scorestrata_testkit::melodic_techno_project();
        let layer = &mut project.arrangement.segments[0].layers[0];
        layer.fx = Some(scorestrata_core::domain::FxChain {
            sends: scorestrata_core::domain::SendLevels {
                reverb: 1.7,
                delay: 0.0,
            },
            ..Default::default()
        });
        let diagnostics = check_timeline(&project);
        assert!(codes(&diagnostics).contains(&"TIMELINE_FX_SENDS_OUT_OF_RANGE"));
    }

    #[test]
    fn sane_fx_chain_passes() {
        let mut project = scorestrata_testkit::melodic_techno_project();
        let layer = &mut project.arrangement.segments[0].layers[0];
        layer.fx = Some(scorestrata_core::domain::FxChain {
            high_pass_hz: Some(40.0),
            eq: vec![scorestrata_core::domain::FreqBand {
                freq_hz: 200.0,
                gain_db: -2.0,
                q: 1.0,
            }],
            sends: scorestrata_core::domain::SendLevels {
                reverb: 0.2,
                delay: 0.0,
            },
            sidechain_amount: None,
            ..Default::default()
        });
        let diagnostics = check_timeline(&project);
        assert!(!codes(&diagnostics).contains(&"TIMELINE_FX_SENDS_OUT_OF_RANGE"));
        assert!(!codes(&diagnostics).contains(&"TIMELINE_FX_HPF_OUT_OF_RANGE"));
        assert!(!codes(&diagnostics).contains(&"TIMELINE_FX_EQ_OUT_OF_RANGE"));
    }

    #[test]
    fn layer_automation_out_of_range_is_flagged() {
        let mut project = scorestrata_testkit::melodic_techno_project();
        let layer = &mut project.arrangement.segments[0].layers[0];
        layer.automation.push(scorestrata_core::domain::Automation {
            layer_id: layer.id,
            param: scorestrata_core::domain::AutomationParam::Cutoff,
            points: vec![scorestrata_core::domain::AutomationPoint {
                tick: Tick::new(0),
                value: 1.5, // out of range
            }],
        });

        let diagnostics = check_timeline(&project);
        assert!(codes(&diagnostics).contains(&"TIMELINE_AUTOMATION_OUT_OF_RANGE"));
    }

    #[test]
    fn empty_automation_lane_is_flagged_as_warning() {
        let mut project = scorestrata_testkit::melodic_techno_project();
        let layer = &mut project.arrangement.segments[0].layers[0];
        layer.automation.push(scorestrata_core::domain::Automation {
            layer_id: layer.id,
            param: scorestrata_core::domain::AutomationParam::Volume,
            points: vec![],
        });

        let diagnostics = check_timeline(&project);
        assert!(codes(&diagnostics).contains(&"TIMELINE_AUTOMATION_EMPTY_LANE"));
    }

    #[test]
    fn valid_automation_lane_passes() {
        let mut project = scorestrata_testkit::melodic_techno_project();
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

        let diagnostics = check_timeline(&project);
        assert!(!codes(&diagnostics).contains(&"TIMELINE_AUTOMATION_OUT_OF_RANGE"));
        assert!(!codes(&diagnostics).contains(&"TIMELINE_AUTOMATION_EMPTY_LANE"));
    }

    #[test]
    fn unresolved_pattern_instance_is_flagged() {
        let mut project = scorestrata_testkit::melodic_techno_project();
        let layer = &mut project.arrangement.segments[0].layers[0];
        layer.pattern_instances.push(PatternInstance {
            id: PatternInstanceId::new(),
            pattern_id: PatternId::new(), // deliberately missing
            start_tick: Tick::new(0),
            transpose: 0,
            velocity_multiplier: 1.0,
        });

        let diagnostics = check_timeline(&project);
        assert!(codes(&diagnostics).contains(&"TIMELINE_UNRESOLVED_PATTERN"));
        let diag = diagnostics
            .iter()
            .find(|d| d.code == "TIMELINE_UNRESOLVED_PATTERN")
            .expect("diagnostic present");
        assert_eq!(diag.severity, Severity::Error);
    }

    #[test]
    fn empty_required_segment_is_flagged() {
        let mut project = scorestrata_testkit::melodic_techno_project();
        project
            .arrangement
            .segments
            .push(scorestrata_core::domain::Segment {
                id: SegmentId::new(),
                name: "empty_verse".to_string(),
                role: SegmentRole::Verse,
                start_bar: 0,
                bars: 8,
                layers: vec![],
            });

        let diagnostics = check_timeline(&project);
        assert!(codes(&diagnostics).contains(&"TIMELINE_EMPTY_REQUIRED_SEGMENT"));
    }

    #[test]
    fn overlapping_same_pitch_notes_are_flagged() {
        let mut project = scorestrata_testkit::melodic_techno_project();
        let pattern = project
            .patterns
            .iter_mut()
            .find(|p| !p.notes.is_empty())
            .expect("fixture has a note-based pattern");
        let pitch = pattern.notes[0].pitch;
        let start = pattern.notes[0].start;
        let duration = pattern.notes[0].duration;
        pattern.notes.push(Note {
            id: NoteId::new(),
            pitch,
            start: Tick::new(start.0.saturating_add(10)),
            duration,
            velocity: Velocity::new(80),
            tied: false,
            articulation: None,
        });

        let diagnostics = check_timeline(&project);
        assert!(codes(&diagnostics).contains(&"TIMELINE_OVERLAPPING_NOTES"));
    }

    #[test]
    fn out_of_range_automation_is_flagged() {
        let mut project = scorestrata_testkit::melodic_techno_project();
        let segment_id = project.arrangement.segments[0].id;
        project
            .arrangement
            .energy_curve
            .push(scorestrata_core::domain::EnergyPoint {
                segment_id,
                target_energy: 1.5,
            });

        let diagnostics = check_timeline(&project);
        assert!(codes(&diagnostics).contains(&"TIMELINE_AUTOMATION_OUT_OF_RANGE"));
    }

    #[test]
    fn note_past_pattern_length_is_flagged() {
        let mut project = scorestrata_testkit::melodic_techno_project();
        let pattern = &mut project.patterns[0];
        let ticks_per_bar = u64::from(PPQ) * u64::from(pattern.time_signature.numerator);
        let beyond = u64::from(pattern.bars) * ticks_per_bar;
        pattern.notes.push(Note {
            id: NoteId::new(),
            pitch: scorestrata_core::domain::MidiNote::new(50),
            start: Tick::new(beyond),
            duration: Duration::whole(),
            velocity: Velocity::new(80),
            tied: false,
            articulation: None,
        });

        let diagnostics = check_timeline(&project);
        assert!(codes(&diagnostics).contains(&"TIMELINE_NOTE_OUT_OF_BOUNDS"));
    }

    #[test]
    fn optional_roles_may_stay_empty() {
        let mut project = scorestrata_testkit::melodic_techno_project();
        project
            .arrangement
            .segments
            .push(scorestrata_core::domain::Segment {
                id: SegmentId::new(),
                name: "empty_outro".to_string(),
                role: SegmentRole::Outro,
                start_bar: 0,
                bars: 4,
                layers: vec![],
            });

        let diagnostics = check_timeline(&project);
        assert!(!codes(&diagnostics).contains(&"TIMELINE_EMPTY_REQUIRED_SEGMENT"));
    }
}
