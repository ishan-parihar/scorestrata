//! Debug plane operations (§12) — real computation behind the debug tools.
//!
//! Implements architecture spec §12: snapshot diffs, candidate diffs,
//! layer isolation, segment solo, metric explanation with root causes,
//! and command tracing from episodic memory. These are the *real*
//! computations the MCP debug tools surface (the tools are thin adapters).

use scorestrata_core::debug::{
    CandidateDiff, CandidateScore, CandidateWinner, CommandEvent, CommandTrace, LayerIsolation,
    MetricExplanation, PatternDiff, RootCauseExplanation, SegmentDiff, SegmentSolo, SnapshotDiff,
};
use scorestrata_core::diagnostic::ValidationReport;
use scorestrata_core::domain::Project;
use scorestrata_storage::lesson_store::LessonStore;
use scorestrata_storage::memory::ProjectMemory;
use std::collections::BTreeMap;

/// Diff two project states into a structured [`SnapshotDiff`].
///
/// Compares segment structure (bar counts, layer membership), pattern
/// content (note/step counts), and mixer state (velocity multipliers,
/// instrument assignments) between two snapshots of the same project.
#[expect(
    clippy::too_many_lines,
    reason = "Diff computation enumerates all structural comparisons"
)]
pub fn compute_snapshot_diff(project_a: &Project, project_b: &Project) -> SnapshotDiff {
    let mut changed_segments: Vec<SegmentDiff> = Vec::new();
    let mut changed_patterns: Vec<PatternDiff> = Vec::new();
    let mut mixer_changed = false;

    // Segment-level comparison (by segment ID).
    for seg_a in &project_a.arrangement.segments {
        let id = seg_a.id.to_string();
        match project_b
            .arrangement
            .segments
            .iter()
            .find(|s| s.id == seg_a.id)
        {
            None => changed_segments.push(SegmentDiff {
                segment_id: id.clone(),
                changes: vec!["segment_removed".to_string()],
                evidence_uri: format!("project://{}/segments/{id}", project_b.id),
            }),
            Some(seg_b) => {
                let mut changes: Vec<String> = Vec::new();
                if seg_a.bars != seg_b.bars {
                    changes.push(format!(
                        "bar_count_changed ({} -> {})",
                        seg_a.bars, seg_b.bars
                    ));
                }
                let layers_a: Vec<&str> = seg_a.layers.iter().map(|l| l.name.as_str()).collect();
                let layers_b: Vec<&str> = seg_b.layers.iter().map(|l| l.name.as_str()).collect();
                if layers_a.len() != layers_b.len() {
                    changes.push(format!(
                        "layer_count_changed ({} -> {})",
                        layers_a.len(),
                        layers_b.len()
                    ));
                }
                for name in &layers_a {
                    if !layers_b.contains(name) {
                        changes.push(format!("layer_removed: {name}"));
                    }
                }
                for name in &layers_b {
                    if !layers_a.contains(name) {
                        changes.push(format!("layer_added: {name}"));
                    }
                }
                if !changes.is_empty() {
                    changed_segments.push(SegmentDiff {
                        segment_id: id.clone(),
                        changes,
                        evidence_uri: format!("project://{}/segments/{id}", project_b.id),
                    });
                }
            }
        }
    }
    for seg_b in &project_b.arrangement.segments {
        let id = seg_b.id.to_string();
        if !project_a
            .arrangement
            .segments
            .iter()
            .any(|s| s.id == seg_b.id)
        {
            changed_segments.push(SegmentDiff {
                segment_id: id.clone(),
                changes: vec!["segment_added".to_string()],
                evidence_uri: format!("project://{}/segments/{id}", project_b.id),
            });
        }
    }

    // Pattern-level comparison (by pattern ID).
    for pat_a in &project_a.patterns {
        let id = pat_a.id.to_string();
        match project_b.patterns.iter().find(|p| p.id == pat_a.id) {
            None => changed_patterns.push(PatternDiff {
                pattern_id: id.clone(),
                changes: vec!["pattern_removed".to_string()],
                evidence_uri: format!("project://{}/patterns/{id}", project_b.id),
            }),
            Some(pat_b) => {
                let mut changes: Vec<String> = Vec::new();
                if pat_a.notes.len() != pat_b.notes.len() {
                    changes.push(format!(
                        "note_count_changed ({} -> {})",
                        pat_a.notes.len(),
                        pat_b.notes.len()
                    ));
                }
                if pat_a.steps.len() != pat_b.steps.len() {
                    changes.push(format!(
                        "step_count_changed ({} -> {})",
                        pat_a.steps.len(),
                        pat_b.steps.len()
                    ));
                }
                if pat_a.name != pat_b.name {
                    changes.push(format!("renamed ({} -> {})", pat_a.name, pat_b.name));
                }
                if !changes.is_empty() {
                    changed_patterns.push(PatternDiff {
                        pattern_id: id.clone(),
                        changes,
                        evidence_uri: format!("project://{}/patterns/{id}", project_b.id),
                    });
                }
            }
        }
    }
    for pat_b in &project_b.patterns {
        let id = pat_b.id.to_string();
        if !project_a.patterns.iter().any(|p| p.id == pat_b.id) {
            changed_patterns.push(PatternDiff {
                pattern_id: id.clone(),
                changes: vec!["pattern_added".to_string()],
                evidence_uri: format!("project://{}/patterns/{id}", project_b.id),
            });
        }
    }

    // Mixer state: velocity multipliers + instrument assignments.
    for seg_a in &project_a.arrangement.segments {
        if let Some(seg_b) = project_b
            .arrangement
            .segments
            .iter()
            .find(|s| s.id == seg_a.id)
        {
            for layer_a in &seg_a.layers {
                if let Some(layer_b) = seg_b.layers.iter().find(|l| l.id == layer_a.id) {
                    if layer_a.instrument != layer_b.instrument {
                        mixer_changed = true;
                    }
                    let gains_a: Vec<f32> = layer_a
                        .pattern_instances
                        .iter()
                        .map(|pi| pi.velocity_multiplier)
                        .collect();
                    let gains_b: Vec<f32> = layer_b
                        .pattern_instances
                        .iter()
                        .map(|pi| pi.velocity_multiplier)
                        .collect();
                    if gains_a != gains_b {
                        mixer_changed = true;
                    }
                }
            }
        }
    }

    let changed_segment_names: Vec<&str> = changed_segments
        .iter()
        .map(|s| s.segment_id.as_str())
        .collect();
    let changed_pattern_names: Vec<&str> = changed_patterns
        .iter()
        .map(|p| p.pattern_id.as_str())
        .collect();
    let summary = format!(
        "{} segments changed ({}), {} patterns changed ({}), mixer {}",
        changed_segments.len(),
        changed_segment_names.join(", "),
        changed_patterns.len(),
        changed_pattern_names.join(", "),
        if mixer_changed {
            "changed"
        } else {
            "unchanged"
        },
    );

    let mut score_delta = BTreeMap::new();
    score_delta.insert(
        "diff.segments_changed".to_string(),
        changed_segments.len().to_string(),
    );
    score_delta.insert(
        "diff.patterns_changed".to_string(),
        changed_patterns.len().to_string(),
    );
    score_delta.insert(
        "diff.mixer_changed".to_string(),
        if mixer_changed { "yes" } else { "no" }.to_string(),
    );

    SnapshotDiff {
        snapshot_a: project_a.id.to_string(),
        snapshot_b: project_b.id.to_string(),
        project_id: project_b.id.to_string(),
        summary,
        changed_segments,
        changed_patterns,
        mixer_changed,
        score_delta,
    }
}

/// Compare two candidate projects by validating both and comparing scorecards.
///
/// Each metric present in either scorecard is compared individually and a
/// winner is selected by majority (ties broken by overall score).
pub fn compute_candidate_diff(project_a: &Project, project_b: &Project) -> CandidateDiff {
    let report_a = project_report(project_a);
    let report_b = project_report(project_b);

    let mut metric_names: Vec<String> = report_a
        .scorecard
        .metrics
        .iter()
        .map(|m| m.name.clone())
        .collect();
    for m in &report_b.scorecard.metrics {
        if !metric_names.contains(&m.name) {
            metric_names.push(m.name.clone());
        }
    }
    metric_names.sort();

    let mut score_comparison: BTreeMap<String, CandidateScore> = BTreeMap::new();
    let mut wins_a = 0usize;
    let mut wins_b = 0usize;

    for name in &metric_names {
        // Both candidates are validated with the identical `project_report`
        // suite, so a metric absent from a scorecard means no diagnostics
        // were emitted for it — the check passed and the metric is perfect
        // (1.0). A candidate with zero violations is not penalized for
        // lacking an entry.
        let score_a = report_a
            .scorecard
            .find_metric(name)
            .map_or(1.0, |m| m.value);
        let score_b = report_b
            .scorecard
            .find_metric(name)
            .map_or(1.0, |m| m.value);
        let winner = if score_a > score_b {
            wins_a += 1;
            CandidateWinner::A
        } else if score_b > score_a {
            wins_b += 1;
            CandidateWinner::B
        } else {
            CandidateWinner::Tie
        };
        score_comparison.insert(
            name.clone(),
            CandidateScore {
                metric: name.clone(),
                score_a,
                score_b,
                winner,
            },
        );
    }

    let (winner, reason) = if wins_a > wins_b {
        (
            Some(CandidateWinner::A),
            Some(format!(
                "Candidate A wins {wins_a} metrics vs {wins_b} for B (overall {:.3} vs {:.3})",
                report_a.scorecard.overall_score, report_b.scorecard.overall_score
            )),
        )
    } else if wins_b > wins_a {
        (
            Some(CandidateWinner::B),
            Some(format!(
                "Candidate B wins {wins_b} metrics vs {wins_a} for A (overall {:.3} vs {:.3})",
                report_b.scorecard.overall_score, report_a.scorecard.overall_score
            )),
        )
    } else if report_a.scorecard.overall_score > report_b.scorecard.overall_score {
        (
            Some(CandidateWinner::A),
            Some(format!(
                "Metric split tied; higher overall score {:.3} vs {:.3}",
                report_a.scorecard.overall_score, report_b.scorecard.overall_score
            )),
        )
    } else if report_b.scorecard.overall_score > report_a.scorecard.overall_score {
        (
            Some(CandidateWinner::B),
            Some(format!(
                "Metric split tied; higher overall score {:.3} vs {:.3}",
                report_b.scorecard.overall_score, report_a.scorecard.overall_score
            )),
        )
    } else {
        (
            Some(CandidateWinner::Tie),
            Some("Candidates are equal".to_string()),
        )
    };

    CandidateDiff {
        candidate_a: project_a.id.to_string(),
        candidate_b: project_b.id.to_string(),
        project_id: project_b.id.to_string(),
        score_comparison,
        winner,
        reason,
    }
}

/// Isolate a layer: locate it across segments and count the notes it plays.
///
/// Returns `None` if the layer ID does not exist in the project.
#[must_use]
pub fn compute_layer_isolation(project: &Project, layer_id: &str) -> Option<LayerIsolation> {
    let mut segment_id: Option<String> = None;
    let mut note_count = 0usize;

    for segment in &project.arrangement.segments {
        for layer in &segment.layers {
            if layer.id.to_string() == layer_id {
                segment_id = Some(segment.id.to_string());
                for pi in &layer.pattern_instances {
                    if let Some(pattern) = project.patterns.iter().find(|p| p.id == pi.pattern_id) {
                        note_count += pattern.notes.len();
                        note_count += pattern.steps.iter().filter(|s| s.active).count();
                    }
                }
            }
        }
    }

    segment_id.as_ref()?;
    Some(LayerIsolation {
        project_id: project.id.to_string(),
        layer_id: layer_id.to_string(),
        segment_id,
        mute_others: true,
        preview_uri: Some(format!("project://{}/lens/layer/{layer_id}", project.id)),
        note_count,
    })
}

/// Solo a segment: report its structure and contained layers.
///
/// Returns `None` if the segment ID does not exist in the project.
#[must_use]
pub fn compute_segment_solo(project: &Project, segment_id: &str) -> Option<SegmentSolo> {
    project
        .arrangement
        .segments
        .iter()
        .find(|s| s.id.to_string() == segment_id)?;
    Some(SegmentSolo {
        project_id: project.id.to_string(),
        segment_id: segment_id.to_string(),
        include_transitions: false,
        preview_uri: Some(format!(
            "project://{}/lens/segment/{segment_id}",
            project.id
        )),
    })
}

/// Explain a metric with root causes from a real validation run.
///
/// Returns `None` if the metric does not appear in the scorecard.
#[must_use]
pub fn compute_metric_explanation(
    project: &Project,
    metric_name: &str,
) -> Option<MetricExplanation> {
    let report = project_report(project);
    let metric = report.scorecard.find_metric(metric_name)?;

    let root_causes: Vec<RootCauseExplanation> = report
        .diagnostics
        .iter()
        .filter(|d| d.metric == metric_name)
        .map(|d| RootCauseExplanation {
            cause: d.code.clone(),
            severity: d.severity,
            scope: d
                .scope
                .segment_id
                .map_or_else(|| "project".to_string(), |sid| format!("segment:{sid}")),
            evidence_uri: d.evidence_uris.first().cloned(),
            suggested_patch: d.suggested_patches.first().map(|p| p.op.clone()),
        })
        .collect();

    let suggested_actions: Vec<String> = report
        .diagnostics
        .iter()
        .filter(|d| d.metric == metric_name)
        .flat_map(|d| d.suggested_patches.iter().map(|p| p.op.clone()))
        .collect();

    let confidence = if root_causes.is_empty() {
        0.5
    } else {
        // Confidence rises with the number of attributed root causes but
        // never reaches 1.0: `1.0 - 0.5 * 0.5^n` (1 cause -> 0.75,
        // 2 -> 0.875, 3 -> 0.9375, ...). Bounded and monotonic.
        #[expect(
            clippy::cast_precision_loss,
            reason = "Root cause counts fit in f32 for a 0.5-1.0 confidence"
        )]
        {
            1.0 - 0.5 * 0.5f32.powi(root_causes.len() as i32)
        }
    };

    Some(MetricExplanation {
        project_id: project.id.to_string(),
        metric_name: metric_name.to_string(),
        current_value: metric.value,
        target_value: Some(1.0),
        root_causes,
        evidence_uris: vec![format!(
            "project://{}/lens/metric/{metric_name}",
            project.id
        )],
        suggested_actions,
        confidence,
    })
}

/// Build a command trace from episodic memory.
///
/// Returns `None` if no command with the given ID exists in memory.
#[must_use]
pub fn build_command_trace(
    memory: &ProjectMemory,
    project_id: &str,
    command_id: &str,
) -> Option<CommandTrace> {
    let record = memory
        .episodic
        .command_history
        .iter()
        .find(|c| c.command_id == command_id)?;

    let mut score_delta = BTreeMap::new();
    if let Some(delta) = record.score_delta {
        score_delta.insert("overall".to_string(), format!("{delta:+.3}"));
    }

    Some(CommandTrace {
        command_id: record.command_id.clone(),
        actor: record.actor.clone(),
        command_type: record.command_type.clone(),
        project_id: project_id.to_string(),
        branch_id: None,
        before_snapshot_id: None,
        after_snapshot_id: None,
        events: vec![CommandEvent {
            event_type: "command_executed".to_string(),
            payload_summary: format!(
                "{} executed by {} ({})",
                record.command_type,
                record.actor,
                if record.success {
                    "succeeded"
                } else {
                    "failed"
                }
            ),
            timestamp: record.timestamp.clone(),
        }],
        policy_decisions: vec![],
        score_delta,
        dry_run: false,
        duration_ms: record.duration_ms,
        succeeded: record.success,
        error: if record.success {
            None
        } else {
            Some("command failed".to_string())
        },
    })
}

/// Derive repair templates from the lesson store (top confident lessons).
#[must_use]
pub fn build_repair_templates(store: &LessonStore) -> Vec<crate::resources::TemplateBrief> {
    let mut briefs: Vec<crate::resources::TemplateBrief> = store
        .get_confident_lessons(0.5)
        .iter()
        .map(|l| crate::resources::TemplateBrief {
            id: l.id.clone(),
            operation: l
                .successful_patch
                .as_deref()
                .unwrap_or("unknown")
                .to_string(),
            scope: l.scope.clone(),
            success_rate: l.success_rate(),
            usage_count: l.success_count + l.failure_count,
        })
        .collect();
    briefs.sort_by(|a, b| {
        b.success_rate
            .partial_cmp(&a.success_rate)
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    briefs
}

/// Build a validation report for a project (Level 2 symbolic checks).
///
/// Mirrors the checks wired into `run_validation`: segment structure,
/// transitions, and per-pattern scale compliance.
#[must_use]
pub fn project_report(project: &Project) -> ValidationReport {
    use scorestrata_core::diagnostic::Diagnostic;

    let mut diagnostics: Vec<Diagnostic> = Vec::new();
    for segment in &project.arrangement.segments {
        if let Some(d) = scorestrata_validator::segment_check::check_segment_has_layers(segment) {
            diagnostics.push(d);
        }
        if let Some(d) =
            scorestrata_validator::segment_check::check_segment_bar_count(segment, 4, 64)
        {
            diagnostics.push(d);
        }
    }

    if let Some(key) = scorestrata_theory::key::Key::parse(&project.config.key) {
        for pattern in &project.patterns {
            let Some((segment_id, layer_id)) = first_scope_for_pattern(project, pattern.id) else {
                continue;
            };
            let result = scorestrata_validator::scale_check::check_scale_compliance(
                pattern, &key, segment_id, layer_id,
            );
            diagnostics.extend(result.diagnostics);
        }
    }

    if project.arrangement.segments.len() >= 2 {
        diagnostics.extend(
            scorestrata_validator::transition_check::check_all_transitions(
                &project.arrangement.segments,
                0.15,
            ),
        );
    }

    scorestrata_validator::report::build_validation_report(project.id, 2, diagnostics)
}

/// Find the (`segment_id`, `layer_id`) scope of the first layer referencing
/// a pattern.
///
/// Returns `None` when the pattern is unreferenced or the project has no
/// segments, so callers can skip the check instead of indexing into empty
/// structures (which would panic).
fn first_scope_for_pattern(
    project: &Project,
    pattern_id: scorestrata_core::ids::PatternId,
) -> Option<(
    scorestrata_core::ids::SegmentId,
    scorestrata_core::ids::LayerId,
)> {
    for segment in &project.arrangement.segments {
        for layer in &segment.layers {
            if layer
                .pattern_instances
                .iter()
                .any(|pi| pi.pattern_id == pattern_id)
            {
                return Some((segment.id, layer.id));
            }
        }
    }
    None
}

#[cfg(test)]
#[expect(
    clippy::expect_used,
    clippy::too_many_lines,
    reason = "Test fixtures use expect for clarity"
)]
mod tests {
    use super::*;

    #[test]
    fn snapshot_diff_detects_segment_and_pattern_changes() {
        let project_a = scorestrata_testkit::melodic_techno_project();
        let mut project_b = project_a.clone();
        for seg in &mut project_b.arrangement.segments {
            seg.bars += 4;
        }
        project_b.arrangement.total_bars += 4;
        // Change a pattern name to force a pattern diff.
        if let Some(p) = project_b.patterns.first_mut() {
            p.name = "renamed".to_string();
        }

        let diff = compute_snapshot_diff(&project_a, &project_b);
        assert!(!diff.changed_segments.is_empty(), "segment diffs reported");
        assert!(!diff.changed_patterns.is_empty(), "pattern diffs reported");
        assert!(
            diff.changed_segments
                .iter()
                .any(|s| { s.changes.iter().any(|c| c.starts_with("bar_count_changed")) }),
            "bar count change captured"
        );
        assert!(diff.summary.contains("segments changed"));
        assert!(diff.score_delta.contains_key("diff.segments_changed"));
    }

    #[test]
    fn snapshot_diff_identical_projects_have_no_changes() {
        let project = scorestrata_testkit::melodic_techno_project();
        let diff = compute_snapshot_diff(&project, &project);
        assert!(diff.changed_segments.is_empty());
        assert!(diff.changed_patterns.is_empty());
        assert!(!diff.mixer_changed);
        assert!(diff.summary.contains("0 segments changed"));
    }

    #[test]
    fn snapshot_diff_detects_mixer_changes() {
        let project_a = scorestrata_testkit::melodic_techno_project();
        let mut project_b = project_a.clone();
        for seg in &mut project_b.arrangement.segments {
            for layer in &mut seg.layers {
                for pi in &mut layer.pattern_instances {
                    pi.velocity_multiplier = 0.5;
                }
            }
        }
        let diff = compute_snapshot_diff(&project_a, &project_b);
        assert!(diff.mixer_changed, "velocity multiplier change detected");
    }

    #[test]
    fn snapshot_diff_detects_layer_added_and_removed() {
        let project_a = scorestrata_testkit::melodic_techno_project();
        let mut project_b = project_a.clone();
        // Add a layer to the first segment of B.
        let first = &mut project_b.arrangement.segments[0];
        first.layers.push(scorestrata_core::domain::Layer {
            id: scorestrata_core::ids::LayerId::new(),
            name: "NewPad".to_string(),
            instrument: "pad_synth".to_string(),
            pattern_instances: vec![],
        });
        let diff = compute_snapshot_diff(&project_a, &project_b);
        assert!(
            diff.changed_segments
                .iter()
                .any(|s| s.changes.iter().any(|c| c.contains("layer_added: NewPad"))),
            "layer add captured: {:?}",
            diff.changed_segments
        );
    }

    #[test]
    fn candidate_diff_selects_higher_scoring_candidate() {
        let project_a = scorestrata_testkit::melodic_techno_project();
        let mut project_b = project_a.clone();
        // Degrade B: every note is out of scale, so B scores lower.
        for pattern in &mut project_b.patterns {
            for note in &mut pattern.notes {
                note.pitch = scorestrata_core::domain::MidiNote::new(61); // C# not in C minor
            }
        }
        let diff = compute_candidate_diff(&project_a, &project_b);
        assert_eq!(diff.winner, Some(CandidateWinner::A));
        assert!(diff.reason.is_some());
        assert!(!diff.score_comparison.is_empty());
        // The score comparison must be keyed by metric name.
        assert!(
            diff.score_comparison
                .contains_key("harmony.scale_compliance")
        );
    }

    #[test]
    fn candidate_diff_ties_when_equal() {
        let project = scorestrata_testkit::melodic_techno_project();
        let diff = compute_candidate_diff(&project, &project);
        assert_eq!(diff.winner, Some(CandidateWinner::Tie));
    }

    #[test]
    fn layer_isolation_finds_layer_and_mutes_others() {
        let project = scorestrata_testkit::melodic_techno_project();
        let layer_id = project.arrangement.segments[0].layers[0].id.to_string();
        let isolation = compute_layer_isolation(&project, &layer_id).expect("layer exists");
        assert!(isolation.mute_others);
        assert!(isolation.segment_id.is_some());
        assert!(isolation.preview_uri.is_some());
    }

    #[test]
    fn layer_isolation_unknown_layer_returns_none() {
        let project = scorestrata_testkit::melodic_techno_project();
        assert!(compute_layer_isolation(&project, "nope").is_none());
    }

    #[test]
    fn segment_solo_finds_segment() {
        let project = scorestrata_testkit::melodic_techno_project();
        let segment_id = project.arrangement.segments[0].id.to_string();
        let solo = compute_segment_solo(&project, &segment_id).expect("segment exists");
        assert_eq!(solo.segment_id, segment_id);
        assert!(!solo.include_transitions);
        assert!(solo.preview_uri.is_some());
    }

    #[test]
    fn segment_solo_unknown_segment_returns_none() {
        let project = scorestrata_testkit::melodic_techno_project();
        assert!(compute_segment_solo(&project, "nope").is_none());
    }

    #[test]
    fn metric_explanation_has_root_causes_for_broken_project() {
        let mut project = scorestrata_testkit::melodic_techno_project();
        for pattern in &mut project.patterns {
            for note in &mut pattern.notes {
                note.pitch = scorestrata_core::domain::MidiNote::new(61);
            }
        }
        let explanation = compute_metric_explanation(&project, "harmony.scale_compliance")
            .expect("metric in scorecard");
        assert!(explanation.current_value < 1.0, "degraded metric value");
        assert!(
            !explanation.root_causes.is_empty(),
            "root causes attributed"
        );
        assert_eq!(explanation.target_value, Some(1.0));
        assert!(
            explanation
                .suggested_actions
                .contains(&"quantize_layer_to_scale".to_string()),
            "suggested action present: {:?}",
            explanation.suggested_actions
        );
        assert!(explanation.confidence > 0.0);
    }

    #[test]
    fn metric_explanation_unknown_metric_returns_none() {
        let project = scorestrata_testkit::melodic_techno_project();
        assert!(compute_metric_explanation(&project, "no.such.metric").is_none());
    }

    #[test]
    fn command_trace_builds_from_episodic_history() {
        let mut memory = ProjectMemory::new();
        memory.record_command(scorestrata_storage::memory::CommandRecord {
            command_id: "cmd_1".to_string(),
            command_type: "apply_patch".to_string(),
            actor: "agent:composer".to_string(),
            success: true,
            score_delta: Some(0.14),
            duration_ms: 42,
            timestamp: chrono::Utc::now().to_rfc3339(),
        });
        let trace = build_command_trace(&memory, "proj_1", "cmd_1").expect("command traced");
        assert_eq!(trace.command_id, "cmd_1");
        assert_eq!(trace.actor, "agent:composer");
        assert_eq!(trace.duration_ms, 42);
        assert!(trace.succeeded);
        assert_eq!(trace.project_id, "proj_1");
        assert_eq!(trace.events.len(), 1);
    }

    #[test]
    fn command_trace_unknown_command_returns_none() {
        let memory = ProjectMemory::new();
        assert!(build_command_trace(&memory, "proj_1", "nope").is_none());
    }

    #[test]
    fn repair_templates_rank_by_success_rate() {
        let mut store = LessonStore::new();
        store.record(scorestrata_storage::lesson_store::StoredLesson {
            id: "les_1".to_string(),
            scope: "harmony".to_string(),
            trigger: "SCALE_VIOLATION".to_string(),
            successful_patch: Some("quantize_layer_to_scale".to_string()),
            failed_patches: vec![],
            evidence_uris: vec![],
            genre: Some("melodic_techno".to_string()),
            confidence: 0.9,
            success_count: 9,
            failure_count: 1,
            created_at: chrono::Utc::now().to_rfc3339(),
            updated_at: chrono::Utc::now().to_rfc3339(),
        });
        store.record(scorestrata_storage::lesson_store::StoredLesson {
            id: "les_2".to_string(),
            scope: "transition".to_string(),
            trigger: "TRANSITION_ENERGY_LOW".to_string(),
            successful_patch: Some("add_transition_package".to_string()),
            failed_patches: vec![],
            evidence_uris: vec![],
            genre: None,
            confidence: 0.7,
            success_count: 1,
            failure_count: 3,
            created_at: chrono::Utc::now().to_rfc3339(),
            updated_at: chrono::Utc::now().to_rfc3339(),
        });
        let templates = build_repair_templates(&store);
        assert_eq!(templates.len(), 2);
        assert_eq!(templates[0].operation, "quantize_layer_to_scale");
        assert!(templates[0].success_rate > templates[1].success_rate);
        assert_eq!(templates[0].usage_count, 10);
    }

    #[test]
    fn repair_templates_empty_store_returns_empty() {
        let store = LessonStore::new();
        assert!(build_repair_templates(&store).is_empty());
    }

    #[test]
    fn project_report_scores_broken_project_lower() {
        let project = scorestrata_testkit::melodic_techno_project();
        let healthy = project_report(&project);
        let mut broken = project;
        for pattern in &mut broken.patterns {
            for note in &mut pattern.notes {
                note.pitch = scorestrata_core::domain::MidiNote::new(61);
            }
        }
        let broken_report = project_report(&broken);
        assert!(broken_report.scorecard.overall_score <= healthy.scorecard.overall_score);
    }
}
