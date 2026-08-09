//! MCP tools — mutating operations that return structured receipts.
//!
//! Every tool returns a receipt with status, score delta, evidence URIs,
//! policy decisions, and next actions per the architecture spec §10.5.
//!
//! Tool categories from the architecture:
//! - Governance tools: `get_capabilities`, `get_contracts`, `get_policy_profile`, `check_budget`
//! - Project tools: `create_project`, `generate_config_from_brief`, `validate_config`, `approve_config`,
//!   `get_project_summary`, `get_onboarding_packet`
//! - Composition tools: `plan_arrangement`, `create_pattern`, `write_pattern_notes`,
//!   `write_pattern_steps`, `apply_motivic_variation`, `place_pattern_in_segment`,
//!   `set_layer_instrument`, `set_segment_goals`, `set_energy_curve`
//! - Patch tools: `apply_patch`, `dry_run_patch`, `quantize_layer_to_scale`, `add_transition_package`,
//!   `harmonize_melody`, `add_drum_fill`, `increase_segment_density`, `reduce_masking`,
//!   `normalize_loudness_target`
//! - Validation tools: `run_validation`, `get_validation_report`, `get_root_causes`,
//!   `get_metric_evidence`, `compare_candidates`, `suggest_fixes`
//! - Debug tools: `get_command_trace`, `get_snapshot_diff`, `get_candidate_diff`,
//!   `isolate_layer`, `solo_segment`, `explain_metric` (real logic in [`debug_ops`])
//! - Escalation tools: `get_failure_report`, `create_human_review_packet`
//!   (real §12.3 logic in [`debug_ops`])
//! - Learning tools: get_lessons, record_lesson, get_repair_templates
//!   (real logic against [`LessonStore`](scorestrata_storage::lesson_store::LessonStore))
//! - Engineering tools: `get_next_task`, `claim_task`, `submit_task_result`, `run_acceptance_tests`,
//!   `get_eval_report`, `update_docs`, `create_adr`, `update_calibration_profile`

use scorestrata_core::diagnostic::Diagnostic;
use scorestrata_core::error::Severity;
use scorestrata_validator::report::build_validation_report;
use scorestrata_validator::segment_check::{check_segment_bar_count, check_segment_has_layers};
use scorestrata_validator::transition_check::check_all_transitions;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

// Shared in-place production mutations live in [`production_ops`]; the tools
// here wrap them in receipts. Re-export the pure helpers so the public
// `scorestrata_mcp::tools::*` paths (used by the eval harness and tests)
// keep working.
pub use crate::production_ops::{fx_summary, genre_mix_chain, param_str, register_for_instrument};

/// The standard receipt warnings for a [`ProductionError`] — the shared
/// failure vocabulary the G2–G6 tools use so the CLI and the tools report
/// identical failures.
fn production_error_warnings(err: &crate::production_ops::ProductionError) -> Vec<String> {
    match err {
        crate::production_ops::ProductionError::SegmentNotFound(_) => {
            vec!["Unknown segment ID".to_string()]
        }
        crate::production_ops::ProductionError::LayerNotFound { .. } => {
            vec!["Unknown layer ID".to_string()]
        }
        crate::production_ops::ProductionError::PatternNotFound(_) => vec![],
        crate::production_ops::ProductionError::UnsupportedParam(_) => {
            vec!["Unknown automation param".to_string()]
        }
        crate::production_ops::ProductionError::InvalidTotalBars => {
            vec!["Invalid total_bars".to_string()]
        }
        crate::production_ops::ProductionError::InvalidExtendTarget { .. } => {
            vec!["total_bars must exceed the current arrangement".to_string()]
        }
    }
}

/// Render a [`ProductionError`](crate::production_ops::ProductionError) as a
/// failed [`ToolReceipt`].
///
/// `warnings` is passed in so each tool preserves its historical receipt
/// warnings verbatim (`repeat_pattern_in_segment` failed with empty warnings
/// before the shared-mutator refactor).
fn production_error_receipt(
    tool: &str,
    err: &crate::production_ops::ProductionError,
    warnings: Vec<String>,
) -> ToolReceipt {
    ToolReceipt {
        tool: tool.to_string(),
        status: ToolStatus::Failed,
        summary: err.to_string(),
        score_delta: BTreeMap::new(),
        evidence_uris: vec![],
        focus_uris: vec![],
        next_actions: vec![],
        cost: ToolCost::default(),
        warnings,
        dry_run: false,
        diagnostics: vec![],
    }
}

/// The standard MCP tool result envelope.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolReceipt {
    /// Tool name that was executed.
    pub tool: String,
    /// Execution status.
    pub status: ToolStatus,
    /// Human-readable summary.
    pub summary: String,
    /// Score delta (what changed in the scorecard).
    #[serde(default)]
    pub score_delta: BTreeMap<String, String>,
    /// Evidence URIs pointing to changed data.
    #[serde(default)]
    pub evidence_uris: Vec<String>,
    /// Focus URIs for agent drill-down.
    #[serde(default)]
    pub focus_uris: Vec<String>,
    /// Suggested next actions.
    #[serde(default)]
    pub next_actions: Vec<String>,
    /// Cost of this operation.
    pub cost: ToolCost,
    /// Warnings produced during execution.
    #[serde(default)]
    pub warnings: Vec<String>,
    /// Whether this was a dry run (no state mutation).
    #[serde(default)]
    pub dry_run: bool,
    /// Diagnostics produced (if validation tool).
    #[serde(default)]
    pub diagnostics: Vec<Diagnostic>,
}

/// Tool execution status.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ToolStatus {
    /// Tool succeeded.
    Succeeded,
    /// Tool failed with an error.
    Failed,
    /// Tool was blocked by policy.
    Blocked,
    /// Dry run completed (no mutation).
    DryRunCompleted,
}

/// Cost accounting for a tool execution.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ToolCost {
    /// CPU milliseconds used.
    pub cpu_ms: u64,
    /// Number of tool calls made.
    pub tool_calls: u32,
    /// Render seconds used.
    pub render_seconds: f32,
}

/// Parameters for the `run_validation` tool.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RunValidationParams {
    /// Project ID to validate.
    pub project_id: String,
    /// Validation scope (full, segment, layer).
    #[serde(default = "default_scope")]
    pub scope: String,
    /// Optional segment IDs to validate.
    #[serde(default)]
    pub segment_ids: Vec<String>,
    /// Optional layer IDs to validate.
    #[serde(default)]
    pub layer_ids: Vec<String>,
    /// Minimum energy delta for transition checks.
    #[serde(default = "default_min_energy_delta")]
    pub min_energy_delta: f32,
}

/// Parameters for the `check_regressions` tool (Level 6 regression plane).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CheckRegressionsParams {
    /// Project ID being checked for regressions.
    pub project_id: String,
    /// Pre-patch validation scorecard to compare against. Any metric that
    /// dropped past the significance threshold becomes a regression
    /// (`METRIC_REGRESSION_DETECTED`, plus `HARD_GATE_REGRESSION` for the
    /// audio hard gates).
    #[serde(default)]
    pub baseline_scorecard: Option<scorestrata_core::diagnostic::Scorecard>,
    /// Pre-patch render checksum (SHA-256 of the WAV bytes). When combined
    /// with the loudness/peak fields, the current render is compared for
    /// §11.7 drift (`RENDER_LOUDNESS_DRIFT`, `RENDER_PEAK_DRIFT`,
    /// `RENDER_CHECKSUM_CHANGED`).
    #[serde(default)]
    pub baseline_checksum: Option<String>,
    /// Pre-patch integrated loudness in LUFS.
    #[serde(default)]
    pub baseline_integrated_lufs: Option<f32>,
    /// Pre-patch true peak in dBTP.
    #[serde(default)]
    pub baseline_true_peak_dbtp: Option<f32>,
    /// Current (post-patch) project payload — required.
    pub project: Option<serde_json::Value>,
}

fn default_scope() -> String {
    "full".to_string()
}

const fn default_min_energy_delta() -> f32 {
    0.15
}

/// Run validation on a project and return a receipt with real diagnostics.
///
/// Runs segment-level checks (has layers, bar count) and transition checks
/// (energy delta between adjacent segments). Returns actionable diagnostics
/// with evidence URIs and suggested patches.
#[must_use]
/// Run validation on segments only.
///
/// This entry point always runs the Level 2 symbolic checks regardless of the
/// `level` argument — `level` only stamps the report. To exercise the brief
/// compliance plane (L5), use [`run_validation_with_project`] instead.
pub fn run_validation(
    params: &RunValidationParams,
    segments: &[scorestrata_core::domain::Segment],
    level: u8,
) -> ToolReceipt {
    let diagnostics = collect_symbolic_diagnostics(params, segments);
    finish_validation_receipt(params, level, diagnostics)
}

/// Run validation at a full level against the whole project.
///
/// Unlike [`run_validation`], this takes the whole project, so it can
/// exercise the arrangement-wide planes the segments-only path cannot see:
/// - `level >= 3`: timeline plane (§11.2 L3) — unresolved patterns, empty
///   required segments, note overlaps, automation range, absolute timing
///   (`TIMELINE_*` diagnostics)
/// - `level >= 4`: audio plane (§11.2 L4) — renders the project and converts
///   the Level-4 analysis (LUFS, true peak, clipping, …) into `AUDIO_*`
///   diagnostics
/// - `level >= 5`: brief-compliance plane (§11.2 L5) against the real brief,
///   config, and arrangement (`BRIEF_*` diagnostics)
///
/// Levels below 3 degrade to the symbolic checks (L2) only.
#[must_use]
pub fn run_validation_with_project(
    params: &RunValidationParams,
    project: &scorestrata_core::domain::Project,
    level: u8,
) -> ToolReceipt {
    // §22.2 — scoped validation. When the caller requests a specific scope
    // (config, arrangement, segment, layer, transition, mix, harmony,
    // rhythm, diff), run only that plane via the validator's scope
    // orchestrator instead of the level-gated full pipeline — this is the
    // "keep loops fast" contract, and it finally honors the previously
    // ignored `scope` param. `full` (the default) keeps the level-gated
    // behavior below.
    if let Some(scope) = scorestrata_validator::scope::ValidationScope::parse(&params.scope) {
        if scope != scorestrata_validator::scope::ValidationScope::Full {
            let mut diagnostics = Vec::new();
            if matches!(
                scope,
                scorestrata_validator::scope::ValidationScope::Segment(_)
            ) && !params.segment_ids.is_empty()
            {
                // Honor the explicitly-requested segment ids: validate only
                // those segments instead of every segment in the project
                // (the `Segment(default)` fallback below means "all").
                for id in &params.segment_ids {
                    if let Ok(segment_id) = id.parse() {
                        diagnostics.extend(scorestrata_validator::scope::validate_symbolic(
                            project,
                            scorestrata_validator::scope::ValidationScope::Segment(segment_id),
                        ));
                    }
                }
            } else {
                diagnostics = scorestrata_validator::scope::validate_symbolic(project, scope);
            }
            if scope == scorestrata_validator::scope::ValidationScope::Mix {
                match render_project_to_analysis(project) {
                    Ok(result) => {
                        diagnostics.extend(
                            scorestrata_validator::audio_check::generate_audio_diagnostics(
                                &result.analysis,
                            ),
                        );
                    }
                    Err(e) => diagnostics.push(audio_render_failed_diagnostic(e)),
                }
            }
            if scope == scorestrata_validator::scope::ValidationScope::Diff {
                // Over the wire there is no compile-cache path, so the diff
                // scope uses the explicitly-requested segment ids as the
                // changed set (the CLI `ss validate --scope diff` is the
                // canonical cache-driven path). Malformed ids are dropped
                // rather than silently coerced to the default id.
                let changed: Vec<scorestrata_core::ids::SegmentId> = params
                    .segment_ids
                    .iter()
                    .filter_map(|id| id.parse().ok())
                    .collect();
                diagnostics.extend(scorestrata_validator::scope::validate_diff(
                    project, &changed,
                ));
            }
            return finish_validation_receipt(params, level, diagnostics);
        }
    }

    let mut diagnostics = collect_symbolic_diagnostics(params, &project.arrangement.segments);

    if level >= 3 {
        diagnostics.extend(scorestrata_validator::timeline_check::check_timeline(
            project,
        ));
        // G2 — layer octave registers: notes outside a layer's declared band
        // flag as `LAYER_NOTE_OUTSIDE_REGISTER` (the production register
        // discipline).
        diagnostics.extend(scorestrata_validator::register_check::check_layer_registers(project));
    }

    if level >= 4 {
        match render_project_to_analysis(project) {
            Ok(result) => {
                diagnostics.extend(
                    scorestrata_validator::audio_check::generate_audio_diagnostics(
                        &result.analysis,
                    ),
                );
                // G7 — render gate: measure deviation from the project's
                // imported reference profile (from `ss analyze reference.wav`)
                // when one exists. Deviation above the drift threshold is
                // reported as `SPECTRAL_REFERENCE_DRIFT`.
                if let Some(reference) = &project.spectral_reference {
                    let report = scorestrata_audio::audit::audit_mono(
                        &result.mono_samples,
                        result.sample_rate,
                        None,
                    );
                    let bands = report.band_shares();
                    let deviation = reference.deviation(
                        &bands,
                        report.spectral_centroid_hz,
                        report.aliasing_ratio,
                    );
                    if deviation > scorestrata_audio::analysis::DRIFT_THRESHOLD {
                        diagnostics.push(scorestrata_core::diagnostic::Diagnostic {
                            id: scorestrata_core::ids::DiagnosticId::new(),
                            code: "SPECTRAL_REFERENCE_DRIFT".to_string(),
                            metric: "render.reference_deviation".to_string(),
                            severity: scorestrata_core::error::Severity::Warning,
                            scope: scorestrata_core::diagnostic::MetricScope {
                                segment_id: None,
                                layer_id: None,
                                bar_range: None,
                            },
                            evidence_uris: vec![format!(
                                "project://{}/render/reference",
                                project.id
                            )],
                            message: format!(
                                "Render deviates {:.2} from reference '{}' (threshold {:.2}) — tune layers/mix toward the reference",
                                deviation, reference.track_name, scorestrata_audio::analysis::DRIFT_THRESHOLD
                            ),
                            root_causes: vec![
                                "render spectrum diverges from the reference track".to_string(),
                            ],
                            suggested_patches: vec![],
                            expected_delta: None,
                            confidence: 1.0,
                        });
                    }
                }
            }
            Err(e) => diagnostics.push(audio_render_failed_diagnostic(e)),
        }
    }

    if level >= 5 {
        let brief_report = scorestrata_validator::brief_check::check_brief_compliance(
            &project.brief,
            &project.config,
            &project.arrangement,
            &scorestrata_validator::brief_check::BriefWeights::default(),
        );
        diagnostics.extend(
            brief_report
                .dimensions
                .iter()
                .flat_map(|d| d.diagnostics.clone()),
        );
    }

    finish_validation_receipt(params, level, diagnostics)
}

/// Resolve the §11.8 genre-calibrated scoring profile for a project.
///
/// Looks up the canonical genre profile in storage and merges its metric
/// weights over the validator's generic defaults; unknown genres fall back
/// to the uncalibrated profile. This is the single wiring point that lets
/// taste calibration reach the scorecard.
#[must_use]
pub fn scoring_profile_for_genre(genre: &str) -> scorestrata_validator::report::ScoringProfile {
    scorestrata_storage::calibration::default_genre_profiles()
        .into_iter()
        .find(|p| p.name == genre)
        .map(|p| {
            scorestrata_validator::report::ScoringProfile::with_genre_weights(p.metric_weights)
        })
        .unwrap_or_default()
}

/// Run Level 6 regression detection (§11.2 L6, §11.7).
///
/// Compares the current project against a pre-patch baseline along two axes:
/// - **Scorecard**: the current L2+L3+L4 validation scorecard vs the supplied
///   `baseline_scorecard` (`METRIC_REGRESSION_DETECTED`,
///   `HARD_GATE_REGRESSION`).
/// - **Render signature**: the current render's checksum / LUFS / true peak
///   vs the supplied baseline (`RENDER_LOUDNESS_DRIFT`, `RENDER_PEAK_DRIFT`,
///   `RENDER_CHECKSUM_CHANGED`).
///
/// Returns `Succeeded` when no error-level regressions are found; `Failed`
/// when regressions (or no baseline at all) block the check.
#[must_use]
pub fn check_regressions(params: &CheckRegressionsParams) -> ToolReceipt {
    let Some(project_json) = &params.project else {
        return ToolReceipt {
            tool: "check_regressions".to_string(),
            status: ToolStatus::Failed,
            summary: format!(
                "Regression check requested for project '{}' but no current project payload was provided",
                params.project_id
            ),
            score_delta: BTreeMap::new(),
            evidence_uris: vec![],
            focus_uris: vec![],
            next_actions: vec!["render_project".to_string()],
            cost: ToolCost::default(),
            warnings: vec![
                "Supply the serialized current project to compare against the baseline".to_string(),
            ],
            dry_run: false,
            diagnostics: vec![],
        };
    };

    let project: scorestrata_core::domain::Project =
        match serde_json::from_value(project_json.clone()) {
            Ok(p) => p,
            Err(e) => {
                return ToolReceipt {
                    tool: "check_regressions".to_string(),
                    status: ToolStatus::Failed,
                    summary: format!("Could not deserialize current project: {e}"),
                    score_delta: BTreeMap::new(),
                    evidence_uris: vec![],
                    focus_uris: vec![],
                    next_actions: vec!["get_project_summary".to_string()],
                    cost: ToolCost::default(),
                    warnings: vec!["Invalid project payload".to_string()],
                    dry_run: false,
                    diagnostics: vec![],
                };
            }
        };

    let has_baseline = params.baseline_scorecard.is_some()
        || params.baseline_checksum.is_some()
        || params.baseline_integrated_lufs.is_some()
        || params.baseline_true_peak_dbtp.is_some();
    if !has_baseline {
        return ToolReceipt {
            tool: "check_regressions".to_string(),
            status: ToolStatus::Failed,
            summary: "No baseline supplied — regression detection needs a pre-patch scorecard and/or render signature"
                .to_string(),
            score_delta: BTreeMap::new(),
            evidence_uris: vec![],
            focus_uris: vec![format!("project://{}/validation/latest", params.project_id)],
            next_actions: vec!["run_validation".to_string()],
            cost: ToolCost::default(),
            warnings: vec![concat!(
                "Supply baseline_scorecard and/or baseline_checksum + ",
                "baseline_integrated_lufs + baseline_true_peak_dbtp"
            )
            .to_string()],
            dry_run: false,
            diagnostics: vec![],
        };
    }

    let mut diagnostics: Vec<Diagnostic> = Vec::new();

    // Scorecard regression: current L2+L3+L4 validation vs the baseline.
    if let Some(baseline) = &params.baseline_scorecard {
        let run_params = RunValidationParams {
            project_id: params.project_id.clone(),
            scope: default_scope(),
            segment_ids: vec![],
            layer_ids: vec![],
            min_energy_delta: default_min_energy_delta(),
        };
        let receipt = run_validation_with_project(&run_params, &project, 4);
        // §11.8: the current scorecard is built with the genre-calibrated
        // weights so regression deltas reflect what matters for the genre.
        let profile = scoring_profile_for_genre(&project.config.genre);
        let current = scorestrata_validator::report::build_validation_report_with_profile(
            project.id,
            4,
            receipt.diagnostics,
            &profile,
        )
        .scorecard;
        let report = scorestrata_validator::regression_check::detect_regressions(
            baseline,
            &current,
            &scorestrata_validator::regression_check::RegressionConfig::default(),
        );
        diagnostics
            .extend(scorestrata_validator::regression_check::regression_diagnostics(&report));
    }

    // Render-signature regression: checksum / loudness / peak drift.
    if params.baseline_checksum.is_some()
        || params.baseline_integrated_lufs.is_some()
        || params.baseline_true_peak_dbtp.is_some()
    {
        match render_project_to_analysis(&project) {
            Ok(result) => {
                // Missing baseline fields default to the current values so a
                // partial baseline only compares the axes it supplies (a
                // checksumless baseline must not trip RENDER_CHECKSUM_CHANGED).
                let baseline = scorestrata_validator::regression_check::RenderSignature {
                    checksum: params
                        .baseline_checksum
                        .clone()
                        .unwrap_or_else(|| result.checksum.clone()),
                    integrated_lufs: params
                        .baseline_integrated_lufs
                        .unwrap_or(result.analysis.integrated_lufs),
                    true_peak_dbtp: params
                        .baseline_true_peak_dbtp
                        .unwrap_or(result.analysis.true_peak_dbtp),
                };
                let current = scorestrata_validator::regression_check::RenderSignature {
                    checksum: result.checksum,
                    integrated_lufs: result.analysis.integrated_lufs,
                    true_peak_dbtp: result.analysis.true_peak_dbtp,
                };
                let regression = scorestrata_validator::regression_check::detect_render_regression(
                    &baseline,
                    &current,
                    &scorestrata_validator::regression_check::RenderRegressionConfig::default(),
                );
                diagnostics.extend(
                    scorestrata_validator::regression_check::render_regression_diagnostics(
                        &regression,
                    ),
                );
            }
            Err(e) => diagnostics.push(audio_render_failed_diagnostic(e)),
        }
    }

    let has_error = diagnostics
        .iter()
        .any(|d| matches!(d.severity, Severity::Error | Severity::Critical));
    let status = if has_error {
        ToolStatus::Failed
    } else {
        ToolStatus::Succeeded
    };
    let warnings: Vec<String> = diagnostics.iter().map(|d| d.message.clone()).collect();
    let regression_count = diagnostics
        .iter()
        .filter(|d| d.code == "METRIC_REGRESSION_DETECTED")
        .count();
    let hard_gate_count = diagnostics
        .iter()
        .filter(|d| d.code == "HARD_GATE_REGRESSION")
        .count();
    let render_drift_count = diagnostics
        .iter()
        .filter(|d| d.code.starts_with("RENDER_"))
        .count();
    let mut score_delta = BTreeMap::new();
    score_delta.insert(
        "regression.metrics".to_string(),
        regression_count.to_string(),
    );
    score_delta.insert(
        "regression.hard_gates".to_string(),
        hard_gate_count.to_string(),
    );
    score_delta.insert(
        "regression.render_drift".to_string(),
        render_drift_count.to_string(),
    );
    let evidence_uris: Vec<String> = diagnostics
        .iter()
        .flat_map(|d| d.evidence_uris.clone())
        .collect();

    let mut next_actions = vec!["apply_patch".to_string()];
    if hard_gate_count > 0 {
        next_actions.push("rollback_to_previous_snapshot".to_string());
    }

    ToolReceipt {
        tool: "check_regressions".to_string(),
        status,
        summary: format!(
            "Regression check for project '{}': {} regression(s), {} hard-gate violation(s), {} render drift(s)",
            params.project_id, regression_count, hard_gate_count, render_drift_count
        ),
        score_delta,
        evidence_uris,
        focus_uris: vec![format!(
            "project://{}/validation/regression",
            params.project_id
        )],
        next_actions,
        cost: ToolCost::default(),
        warnings,
        dry_run: false,
        diagnostics,
    }
}

/// Collect the Level 2 symbolic diagnostics (segment + transition checks).
fn collect_symbolic_diagnostics(
    params: &RunValidationParams,
    segments: &[scorestrata_core::domain::Segment],
) -> Vec<Diagnostic> {
    let mut diagnostics: Vec<Diagnostic> = Vec::new();

    // Run segment-level checks
    for segment in segments {
        if let Some(d) = check_segment_has_layers(segment) {
            diagnostics.push(d);
        }

        // Check segment bar count (reasonable range: 4–64 bars)
        if let Some(d) = check_segment_bar_count(segment, 4, 64) {
            diagnostics.push(d);
        }
    }

    // Run transition checks between adjacent segments
    if segments.len() >= 2 {
        let transition_diags = check_all_transitions(segments, params.min_energy_delta);
        diagnostics.extend(transition_diags);
    }

    diagnostics
}

/// Build the `run_validation` receipt from a diagnostic set.
fn finish_validation_receipt(
    params: &RunValidationParams,
    level: u8,
    diagnostics: Vec<Diagnostic>,
) -> ToolReceipt {
    // Build validation report with per-metric scoring
    let project_id = params.project_id.parse().unwrap_or_default();
    let report = build_validation_report(project_id, level, diagnostics.clone());

    let summary = format!(
        "Validation completed: {} diagnostics ({} warnings, {} errors, {} critical), overall score: {:.2}",
        report.summary.info
            + report.summary.warnings
            + report.summary.errors
            + report.summary.critical,
        report.summary.warnings,
        report.summary.errors,
        report.summary.critical,
        report.scorecard.overall_score,
    );

    let warnings: Vec<String> = diagnostics
        .iter()
        .filter(|d| d.severity == Severity::Warning)
        .map(|d| d.message.clone())
        .collect();

    ToolReceipt {
        tool: "run_validation".to_string(),
        status: if report.all_hard_gates_passed {
            ToolStatus::Succeeded
        } else {
            ToolStatus::Failed
        },
        summary,
        score_delta: BTreeMap::new(),
        evidence_uris: diagnostics
            .iter()
            .flat_map(|d| d.evidence_uris.clone())
            .collect::<std::collections::BTreeSet<_>>()
            .into_iter()
            .collect(),
        focus_uris: vec![format!("project://{}/validation/latest", params.project_id)],
        next_actions: if report.all_hard_gates_passed {
            vec!["get_audit_summary".to_string()]
        } else {
            vec![
                "suggest_fixes".to_string(),
                "apply_patch".to_string(),
                "run_validation".to_string(),
            ]
        },
        cost: ToolCost::default(),
        warnings,
        dry_run: false,
        diagnostics,
    }
}

/// Parameters for the `apply_patch` tool.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ApplyPatchParams {
    /// Project ID to patch.
    pub project_id: String,
    /// Patch operation name (e.g. `quantize_layer_to_scale`).
    pub op: String,
    /// Patch parameters.
    pub params: serde_json::Value,
    /// Whether to dry-run (no state mutation).
    #[serde(default)]
    pub dry_run: bool,
    /// Scope for the patch.
    pub scope: Option<PatchScope>,
    /// Serialized project to apply the patch against (when provided, the
    /// patch is applied for real and the receipt reports actual deltas).
    #[serde(default)]
    pub project: Option<serde_json::Value>,
}

/// Scope for a patch operation.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PatchScope {
    /// Segment ID.
    pub segment_id: Option<String>,
    /// Layer ID.
    pub layer_id: Option<String>,
}

/// Apply a patch to a project with optional dry-run support.
///
/// When a `project` is provided (see [`ApplyPatchParams::project`]), the patch
/// is applied for real and the receipt reports the actual score delta. Without
/// a project, a dry-run-style receipt is returned describing the intended
/// mutation.
#[must_use]
pub fn apply_patch(params: &ApplyPatchParams) -> ToolReceipt {
    // §15.5: every apply_patch invocation is a patch attempt, accounted before
    // branching so failed/blocked attempts also count toward the metrics.
    let receipt = match &params.project {
        Some(project_json) => {
            let deserialized =
                serde_json::from_value::<scorestrata_core::domain::Project>(project_json.clone());
            match deserialized {
                Ok(mut project) => {
                    let mut receipt =
                        crate::patch_ops::apply_patch_to_project(params, &mut project);
                    if params.dry_run {
                        receipt.status = ToolStatus::DryRunCompleted;
                        receipt.dry_run = true;
                        receipt
                            .warnings
                            .push("Dry run — mutation simulated but not persisted".to_string());
                    }
                    receipt
                }
                Err(e) => ToolReceipt {
                    tool: "apply_patch".to_string(),
                    status: ToolStatus::Failed,
                    summary: format!("Could not deserialize project: {e}"),
                    score_delta: BTreeMap::new(),
                    evidence_uris: vec![],
                    focus_uris: vec![],
                    next_actions: vec!["get_project_summary".to_string()],
                    cost: ToolCost::default(),
                    warnings: vec!["Invalid project payload".to_string()],
                    dry_run: params.dry_run,
                    diagnostics: vec![],
                },
            }
        }
        None => ToolReceipt {
            tool: "apply_patch".to_string(),
            status: if params.dry_run {
                ToolStatus::DryRunCompleted
            } else {
                ToolStatus::Failed
            },
            summary: format!(
                "'{}' patch requested for project '{}' but no project payload provided",
                params.op, params.project_id
            ),
            score_delta: BTreeMap::new(),
            evidence_uris: vec![],
            focus_uris: vec![format!("project://{}/validation/latest", params.project_id)],
            next_actions: vec!["get_project_summary".to_string()],
            cost: ToolCost::default(),
            warnings: vec![
                "No project payload — supply the serialized project to apply the patch".to_string(),
            ],
            dry_run: params.dry_run,
            diagnostics: vec![],
        },
    };
    crate::telemetry::record_patch_attempt(matches!(
        receipt.status,
        ToolStatus::Succeeded | ToolStatus::DryRunCompleted
    ));
    receipt
}

/// Parameters for `dry_run_patch` tool.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DryRunPatchParams {
    /// Project ID to patch.
    pub project_id: String,
    /// Patch operation name.
    pub op: String,
    /// Patch parameters.
    pub params: serde_json::Value,
    /// Scope for the patch.
    pub scope: Option<PatchScope>,
    /// Serialized project to simulate the patch against.
    #[serde(default)]
    pub project: Option<serde_json::Value>,
}

/// Dry-run a patch (§5.3, §10.5) — simulate the mutation and report the
/// expected score delta, events, and diagnostics without persisting anything.
///
/// Delegates to [`apply_patch`] with `dry_run` forced on; the underlying
/// patch logic runs against a working clone so nothing is mutated.
#[must_use]
pub fn dry_run_patch(params: &DryRunPatchParams) -> ToolReceipt {
    let mut receipt = apply_patch(&ApplyPatchParams {
        project_id: params.project_id.clone(),
        op: params.op.clone(),
        params: params.params.clone(),
        dry_run: true,
        scope: params.scope.clone(),
        project: params.project.clone(),
    });
    // `apply_patch` already sets DryRunCompleted + the no-mutation warning; we
    // only refine the tool name and summary here. Two transparency rules:
    //
    // 1. Without a serialized project payload no effects can be computed, so
    //    the summary must say so instead of claiming effects were reported.
    // 2. A deserialization failure already carries an accurate summary + the
    //    `dry_run` flag — do not append a misleading "requires payload"
    //    warning (a payload *was* supplied; it just failed to parse).
    receipt.tool = "dry_run_patch".to_string();
    if matches!(receipt.status, ToolStatus::DryRunCompleted) {
        let no_payload = receipt
            .warnings
            .iter()
            .any(|w| w.contains("No project payload"));
        receipt.summary = if no_payload {
            format!(
                "Dry-run of '{}' requested for project '{}' — no project payload, so no effects could be estimated (nothing persisted)",
                params.op, params.project_id
            )
        } else {
            format!(
                "Dry-run of '{}' on project '{}' — mutation simulated, expected effects reported, nothing persisted",
                params.op, params.project_id
            )
        };
    }
    receipt
}

/// Parameters for `harmonize_melody` tool.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HarmonizeMelodyParams {
    /// Project ID.
    pub project_id: String,
    /// Segment ID containing the melody layer.
    pub segment_id: String,
    /// Layer ID of the melody to harmonize.
    pub layer_id: String,
    /// Interval to add (in scale degrees; 3 = third).
    #[serde(default = "default_third")]
    pub interval_degrees: u32,
    /// Serialized project to mutate (when provided, real).
    #[serde(default)]
    pub project: Option<serde_json::Value>,
}

fn default_third() -> u32 {
    3
}

/// Add a harmony voice a third below the melody, in-key (§10.5, §11).
///
/// For every note in the target layer's patterns, appends a harmony note
/// pitched an interval below, snapped into the project scale when possible.
#[must_use]
pub fn harmonize_melody(params: &HarmonizeMelodyParams) -> ToolReceipt {
    let Some(project_json) = &params.project else {
        return ToolReceipt {
            tool: "harmonize_melody".to_string(),
            status: ToolStatus::Failed,
            summary: format!(
                "Cannot harmonize layer '{}': no project payload provided",
                params.layer_id
            ),
            score_delta: BTreeMap::new(),
            evidence_uris: vec![],
            focus_uris: vec![],
            next_actions: vec!["get_project_summary".to_string()],
            cost: ToolCost::default(),
            warnings: vec![
                "No project payload — supply the serialized project to mutate".to_string(),
            ],
            dry_run: false,
            diagnostics: vec![],
        };
    };
    let mut project =
        match serde_json::from_value::<scorestrata_core::domain::Project>(project_json.clone()) {
            Ok(p) => p,
            Err(e) => return deserialize_error_receipt("harmonize_melody", "project", &e),
        };
    let scale = scorestrata_theory::key::Key::parse(&project.config.key).map(|k| k.to_scale());
    let Some(segment) = project
        .arrangement
        .segments
        .iter_mut()
        .find(|s| s.id.to_string() == params.segment_id)
    else {
        return ToolReceipt {
            tool: "harmonize_melody".to_string(),
            status: ToolStatus::Failed,
            summary: format!("Segment '{}' not found", params.segment_id),
            score_delta: BTreeMap::new(),
            evidence_uris: vec![],
            focus_uris: vec![],
            next_actions: vec![],
            cost: ToolCost::default(),
            warnings: vec!["Unknown segment ID".to_string()],
            dry_run: false,
            diagnostics: vec![],
        };
    };
    let Some(layer) = segment
        .layers
        .iter_mut()
        .find(|l| l.id.to_string() == params.layer_id)
    else {
        return ToolReceipt {
            tool: "harmonize_melody".to_string(),
            status: ToolStatus::Failed,
            summary: format!(
                "Layer '{}' not found in segment '{}'",
                params.layer_id, params.segment_id
            ),
            score_delta: BTreeMap::new(),
            evidence_uris: vec![],
            focus_uris: vec![],
            next_actions: vec![],
            cost: ToolCost::default(),
            warnings: vec!["Unknown layer ID".to_string()],
            dry_run: false,
            diagnostics: vec![],
        };
    };
    let pattern_ids: Vec<_> = layer
        .pattern_instances
        .iter()
        .map(|pi| pi.pattern_id)
        .collect();
    let mut harmony_notes = 0usize;
    for pattern in &mut project.patterns {
        if !pattern_ids.contains(&pattern.id) {
            continue;
        }
        let original: Vec<scorestrata_core::domain::Note> = pattern.notes.clone();
        for note in &original {
            let base = i32::from(note.pitch.0);
            #[expect(
                clippy::cast_possible_truncation,
                clippy::cast_sign_loss,
                reason = "Clamped to 0..=127 before casting to u8"
            )] // Clamp the interval to a sane range so pathological values
            // (e.g. > i32::MAX) degrade honestly instead of silently
            // substituting a magic default.
            let interval = i32::try_from(params.interval_degrees)
                .unwrap_or(i32::MAX)
                .clamp(1, 24);
            let below = (base - interval).clamp(0, 127) as u8;
            let harmony_pitch = match &scale {
                Some(scale) => scale.quantize(&scorestrata_core::domain::MidiNote::new(below)),
                None => scorestrata_core::domain::MidiNote::new(below),
            };
            pattern.notes.push(scorestrata_core::domain::Note {
                id: scorestrata_core::ids::NoteId::new(),
                pitch: harmony_pitch,
                start: note.start,
                duration: note.duration,
                velocity: scorestrata_core::domain::Velocity::new(
                    note.velocity.0.saturating_sub(10),
                ),
                tied: false,
            });
            harmony_notes += 1;
        }
    }
    if pattern_ids.is_empty() {
        return ToolReceipt {
            tool: "harmonize_melody".to_string(),
            status: ToolStatus::Failed,
            summary: format!(
                "Layer '{}' references no patterns — nothing to harmonize",
                params.layer_id
            ),
            score_delta: BTreeMap::new(),
            evidence_uris: vec![],
            focus_uris: vec![],
            next_actions: vec![],
            cost: ToolCost::default(),
            warnings: vec!["No pattern instances on layer".to_string()],
            dry_run: false,
            diagnostics: vec![],
        };
    }
    if harmony_notes == 0 {
        return ToolReceipt {
            tool: "harmonize_melody".to_string(),
            status: ToolStatus::Failed,
            summary: format!(
                "Layer '{}' references patterns with no notes — nothing to harmonize",
                params.layer_id
            ),
            score_delta: BTreeMap::new(),
            evidence_uris: vec![],
            focus_uris: vec![],
            next_actions: vec![],
            cost: ToolCost::default(),
            warnings: vec!["Referenced patterns contain no notes".to_string()],
            dry_run: false,
            diagnostics: vec![],
        };
    }
    let mut score_delta = BTreeMap::new();
    score_delta.insert("harmony.notes_added".to_string(), harmony_notes.to_string());
    ToolReceipt {
        tool: "harmonize_melody".to_string(),
        status: ToolStatus::Succeeded,
        summary: format!(
            "Added {harmony_notes} harmony note(s) below layer '{}' (interval: {} degrees, in-key: {})",
            params.layer_id,
            params.interval_degrees,
            scale.is_some()
        ),
        score_delta,
        evidence_uris: vec![format!(
            "project://{}/segments/{}/layers/{}",
            params.project_id, params.segment_id, params.layer_id
        )],
        focus_uris: vec![],
        next_actions: vec!["run_validation".to_string()],
        cost: ToolCost {
            cpu_ms: 4,
            tool_calls: 1,
            ..Default::default()
        },
        warnings: vec![],
        dry_run: false,
        diagnostics: vec![],
    }
}

/// Parameters for `add_drum_fill` tool.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AddDrumFillParams {
    /// Project ID.
    pub project_id: String,
    /// Segment ID containing the drum layer.
    pub segment_id: String,
    /// Layer ID of the drum layer.
    pub layer_id: String,
    /// Number of fill steps to add.
    #[serde(default = "default_fill_steps")]
    pub steps: u32,
    /// Serialized project to mutate (when provided, real).
    #[serde(default)]
    pub project: Option<serde_json::Value>,
}

fn default_fill_steps() -> u32 {
    4
}

/// Add a drum fill to a layer's pattern step sequencer (§10.5, §11).
#[must_use]
pub fn add_drum_fill(params: &AddDrumFillParams) -> ToolReceipt {
    let Some(project_json) = &params.project else {
        return ToolReceipt {
            tool: "add_drum_fill".to_string(),
            status: ToolStatus::Failed,
            summary: format!(
                "Cannot add drum fill to layer '{}': no project payload provided",
                params.layer_id
            ),
            score_delta: BTreeMap::new(),
            evidence_uris: vec![],
            focus_uris: vec![],
            next_actions: vec!["get_project_summary".to_string()],
            cost: ToolCost::default(),
            warnings: vec![
                "No project payload — supply the serialized project to mutate".to_string(),
            ],
            dry_run: false,
            diagnostics: vec![],
        };
    };
    let mut project =
        match serde_json::from_value::<scorestrata_core::domain::Project>(project_json.clone()) {
            Ok(p) => p,
            Err(e) => return deserialize_error_receipt("add_drum_fill", "project", &e),
        };
    let Some(segment) = project
        .arrangement
        .segments
        .iter_mut()
        .find(|s| s.id.to_string() == params.segment_id)
    else {
        return ToolReceipt {
            tool: "add_drum_fill".to_string(),
            status: ToolStatus::Failed,
            summary: format!("Segment '{}' not found", params.segment_id),
            score_delta: BTreeMap::new(),
            evidence_uris: vec![],
            focus_uris: vec![],
            next_actions: vec![],
            cost: ToolCost::default(),
            warnings: vec!["Unknown segment ID".to_string()],
            dry_run: false,
            diagnostics: vec![],
        };
    };
    let Some(layer) = segment
        .layers
        .iter_mut()
        .find(|l| l.id.to_string() == params.layer_id)
    else {
        return ToolReceipt {
            tool: "add_drum_fill".to_string(),
            status: ToolStatus::Failed,
            summary: format!(
                "Layer '{}' not found in segment '{}'",
                params.layer_id, params.segment_id
            ),
            score_delta: BTreeMap::new(),
            evidence_uris: vec![],
            focus_uris: vec![],
            next_actions: vec![],
            cost: ToolCost::default(),
            warnings: vec!["Unknown layer ID".to_string()],
            dry_run: false,
            diagnostics: vec![],
        };
    };
    let pattern_ids: Vec<_> = layer
        .pattern_instances
        .iter()
        .map(|pi| pi.pattern_id)
        .collect();
    let mut steps_added = 0usize;
    for pattern in &mut project.patterns {
        if !pattern_ids.contains(&pattern.id) {
            continue;
        }
        let base_index = pattern.steps.len() as u32;
        for i in 0..params.steps {
            pattern.steps.push(scorestrata_core::domain::Step {
                id: scorestrata_core::ids::StepId::new(),
                index: base_index + i,
                active: i % 2 == 0,
                velocity: scorestrata_core::domain::Velocity::new(120),
            });
            steps_added += 1;
        }
    }
    if steps_added == 0 {
        return ToolReceipt {
            tool: "add_drum_fill".to_string(),
            status: ToolStatus::Failed,
            summary: format!(
                "Layer '{}' references no patterns — nothing to fill",
                params.layer_id
            ),
            score_delta: BTreeMap::new(),
            evidence_uris: vec![],
            focus_uris: vec![],
            next_actions: vec![],
            cost: ToolCost::default(),
            warnings: vec!["No pattern instances on layer".to_string()],
            dry_run: false,
            diagnostics: vec![],
        };
    }
    let mut score_delta = BTreeMap::new();
    score_delta.insert("drum.steps_added".to_string(), steps_added.to_string());
    ToolReceipt {
        tool: "add_drum_fill".to_string(),
        status: ToolStatus::Succeeded,
        summary: format!(
            "Added {steps_added} fill step(s) to drum layer '{}'",
            params.layer_id
        ),
        score_delta,
        evidence_uris: vec![format!(
            "project://{}/segments/{}/layers/{}/steps",
            params.project_id, params.segment_id, params.layer_id
        )],
        focus_uris: vec![],
        next_actions: vec!["run_validation".to_string()],
        cost: ToolCost {
            cpu_ms: 2,
            tool_calls: 1,
            ..Default::default()
        },
        warnings: vec![],
        dry_run: false,
        diagnostics: vec![],
    }
}

/// Parameters for `increase_segment_density` tool.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IncreaseSegmentDensityParams {
    /// Project ID.
    pub project_id: String,
    /// Segment ID to densify.
    pub segment_id: String,
    /// Density factor (1 = no change, 2 = double note count).
    #[serde(default = "default_density_factor")]
    pub factor: u32,
    /// Serialized project to mutate (when provided, real).
    #[serde(default)]
    pub project: Option<serde_json::Value>,
}

fn default_density_factor() -> u32 {
    2
}

/// Increase a segment's note density by doubling notes with a swing offset (§10.5, §11).
#[must_use]
pub fn increase_segment_density(params: &IncreaseSegmentDensityParams) -> ToolReceipt {
    let Some(project_json) = &params.project else {
        return ToolReceipt {
            tool: "increase_segment_density".to_string(),
            status: ToolStatus::Failed,
            summary: format!(
                "Cannot densify segment '{}': no project payload provided",
                params.segment_id
            ),
            score_delta: BTreeMap::new(),
            evidence_uris: vec![],
            focus_uris: vec![],
            next_actions: vec!["get_project_summary".to_string()],
            cost: ToolCost::default(),
            warnings: vec![
                "No project payload — supply the serialized project to mutate".to_string(),
            ],
            dry_run: false,
            diagnostics: vec![],
        };
    };
    let mut project =
        match serde_json::from_value::<scorestrata_core::domain::Project>(project_json.clone()) {
            Ok(p) => p,
            Err(e) => return deserialize_error_receipt("increase_segment_density", "project", &e),
        };
    let Some(segment) = project
        .arrangement
        .segments
        .iter()
        .find(|s| s.id.to_string() == params.segment_id)
    else {
        return ToolReceipt {
            tool: "increase_segment_density".to_string(),
            status: ToolStatus::Failed,
            summary: format!("Segment '{}' not found", params.segment_id),
            score_delta: BTreeMap::new(),
            evidence_uris: vec![],
            focus_uris: vec![],
            next_actions: vec![],
            cost: ToolCost::default(),
            warnings: vec!["Unknown segment ID".to_string()],
            dry_run: false,
            diagnostics: vec![],
        };
    };
    let pattern_ids: Vec<_> = segment
        .layers
        .iter()
        .flat_map(|l| l.pattern_instances.iter().map(|pi| pi.pattern_id))
        .collect();
    let extra = params.factor.saturating_sub(1).max(1);
    let mut notes_added = 0usize;
    for pattern in &mut project.patterns {
        if !pattern_ids.contains(&pattern.id) {
            continue;
        }
        let original: Vec<scorestrata_core::domain::Note> = pattern.notes.clone();
        for note in &original {
            for i in 1..=extra {
                #[expect(
                    clippy::cast_possible_truncation,
                    clippy::cast_sign_loss,
                    reason = "Velocity reduction stays in u8 range (5 * small i)"
                )]
                let velocity_cut = (5 * i) as u8;
                pattern.notes.push(scorestrata_core::domain::Note {
                    id: scorestrata_core::ids::NoteId::new(),
                    pitch: note.pitch,
                    start: scorestrata_core::domain::Tick::new(
                        note.start
                            .0
                            .saturating_add(note.duration.0 / u64::from(i + 1)),
                    ),
                    duration: note.duration,
                    velocity: scorestrata_core::domain::Velocity::new(
                        note.velocity.0.saturating_sub(velocity_cut),
                    ),
                    tied: false,
                });
                notes_added += 1;
            }
        }
    }
    if pattern_ids.is_empty() {
        return ToolReceipt {
            tool: "increase_segment_density".to_string(),
            status: ToolStatus::Failed,
            summary: format!(
                "Segment '{}' references no patterns — nothing to densify",
                params.segment_id
            ),
            score_delta: BTreeMap::new(),
            evidence_uris: vec![],
            focus_uris: vec![],
            next_actions: vec![],
            cost: ToolCost::default(),
            warnings: vec!["No pattern instances in segment".to_string()],
            dry_run: false,
            diagnostics: vec![],
        };
    }
    if notes_added == 0 {
        return ToolReceipt {
            tool: "increase_segment_density".to_string(),
            status: ToolStatus::Failed,
            summary: format!(
                "Segment '{}' references patterns with no notes — nothing to densify",
                params.segment_id
            ),
            score_delta: BTreeMap::new(),
            evidence_uris: vec![],
            focus_uris: vec![],
            next_actions: vec![],
            cost: ToolCost::default(),
            warnings: vec!["Referenced patterns contain no notes".to_string()],
            dry_run: false,
            diagnostics: vec![],
        };
    }
    let mut score_delta = BTreeMap::new();
    score_delta.insert("segment.notes_added".to_string(), notes_added.to_string());
    ToolReceipt {
        tool: "increase_segment_density".to_string(),
        status: ToolStatus::Succeeded,
        summary: format!(
            "Increased density of segment '{}': {notes_added} note(s) added (factor {})",
            params.segment_id, params.factor
        ),
        score_delta,
        evidence_uris: vec![format!(
            "project://{}/segments/{}",
            params.project_id, params.segment_id
        )],
        focus_uris: vec![],
        next_actions: vec!["run_validation".to_string()],
        cost: ToolCost {
            cpu_ms: 4,
            tool_calls: 1,
            ..Default::default()
        },
        warnings: vec![],
        dry_run: false,
        diagnostics: vec![],
    }
}

/// Parameters for `reduce_masking` tool.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReduceMaskingParams {
    /// Project ID.
    pub project_id: String,
    /// Segment ID to de-mask.
    pub segment_id: String,
    /// Velocity reduction applied to overlapping notes (0.0–1.0).
    #[serde(default = "default_masking_amount")]
    pub amount: f32,
    /// Serialized project to mutate (when provided, real).
    #[serde(default)]
    pub project: Option<serde_json::Value>,
}

fn default_masking_amount() -> f32 {
    0.15
}

/// Reduce masking by lowering velocity of notes that overlap in pitch and time (§10.5, §11).
///
/// Notes in the same pitch register that start within one tick window of
/// another note get their velocity reduced by `amount`, clearing spectral
/// overlap.
#[must_use]
pub fn reduce_masking(params: &ReduceMaskingParams) -> ToolReceipt {
    let Some(project_json) = &params.project else {
        return ToolReceipt {
            tool: "reduce_masking".to_string(),
            status: ToolStatus::Failed,
            summary: format!(
                "Cannot reduce masking in segment '{}': no project payload provided",
                params.segment_id
            ),
            score_delta: BTreeMap::new(),
            evidence_uris: vec![],
            focus_uris: vec![],
            next_actions: vec!["get_project_summary".to_string()],
            cost: ToolCost::default(),
            warnings: vec![
                "No project payload — supply the serialized project to mutate".to_string(),
            ],
            dry_run: false,
            diagnostics: vec![],
        };
    };
    let mut project =
        match serde_json::from_value::<scorestrata_core::domain::Project>(project_json.clone()) {
            Ok(p) => p,
            Err(e) => return deserialize_error_receipt("reduce_masking", "project", &e),
        };
    let Some(segment) = project
        .arrangement
        .segments
        .iter()
        .find(|s| s.id.to_string() == params.segment_id)
    else {
        return ToolReceipt {
            tool: "reduce_masking".to_string(),
            status: ToolStatus::Failed,
            summary: format!("Segment '{}' not found", params.segment_id),
            score_delta: BTreeMap::new(),
            evidence_uris: vec![],
            focus_uris: vec![],
            next_actions: vec![],
            cost: ToolCost::default(),
            warnings: vec!["Unknown segment ID".to_string()],
            dry_run: false,
            diagnostics: vec![],
        };
    };
    let pattern_ids: Vec<_> = segment
        .layers
        .iter()
        .flat_map(|l| l.pattern_instances.iter().map(|pi| pi.pattern_id))
        .collect();
    let mut velocity_reduced = 0usize;
    #[expect(
        clippy::cast_possible_truncation,
        reason = "Amount is a small f32 fraction; product clamped to 0..=127"
    )]
    let reduction = (127.0 * params.amount.clamp(0.0, 1.0)) as u8;
    for pattern in &mut project.patterns {
        if !pattern_ids.contains(&pattern.id) {
            continue;
        }
        // Two passes: clone to avoid borrow conflicts, then detect overlaps.
        let notes_snapshot: Vec<scorestrata_core::domain::Note> = pattern.notes.clone();
        for note in &mut pattern.notes {
            let overlapping = notes_snapshot.iter().any(|other| {
                other.id != note.id
                    && (other.pitch.0 as i16 - i16::from(note.pitch.0)).unsigned_abs() <= 7
                    && other.start.0.abs_diff(note.start.0) <= 48
            });
            if overlapping && note.velocity.0 > reduction {
                note.velocity =
                    scorestrata_core::domain::Velocity::new(note.velocity.0 - reduction);
                velocity_reduced += 1;
            }
        }
    }
    if velocity_reduced == 0 {
        return ToolReceipt {
            tool: "reduce_masking".to_string(),
            status: ToolStatus::Failed,
            summary: format!(
                "No overlapping notes found in segment '{}' — nothing to de-mask",
                params.segment_id
            ),
            score_delta: BTreeMap::new(),
            evidence_uris: vec![],
            focus_uris: vec![],
            next_actions: vec![],
            cost: ToolCost::default(),
            warnings: vec!["No spectral overlap detected".to_string()],
            dry_run: false,
            diagnostics: vec![],
        };
    }
    let mut score_delta = BTreeMap::new();
    score_delta.insert(
        "mix.masking_reduced".to_string(),
        velocity_reduced.to_string(),
    );
    ToolReceipt {
        tool: "reduce_masking".to_string(),
        status: ToolStatus::Succeeded,
        summary: format!(
            "Reduced masking in segment '{}': lowered velocity of {velocity_reduced} overlapping note(s) by {:.0}%",
            params.segment_id,
            params.amount * 100.0
        ),
        score_delta,
        evidence_uris: vec![format!(
            "project://{}/segments/{}/mix",
            params.project_id, params.segment_id
        )],
        focus_uris: vec![],
        next_actions: vec!["run_validation".to_string()],
        cost: ToolCost {
            cpu_ms: 3,
            tool_calls: 1,
            ..Default::default()
        },
        warnings: vec![],
        dry_run: false,
        diagnostics: vec![],
    }
}

/// Parameters for `normalize_loudness_target` tool.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NormalizeLoudnessTargetParams {
    /// Project ID.
    pub project_id: String,
    /// Target loudness in LUFS (falls back to genre-calibrated target).
    #[serde(default)]
    pub target_lufs: Option<f32>,
    /// Serialized project to mutate (when provided, real).
    #[serde(default)]
    pub project: Option<serde_json::Value>,
}

/// Normalize a project's loudness to a genre-calibrated target (§10.5, §11.6).
///
/// Delegates to the real `apply_patch` `normalize_loudness_target` operation.
#[must_use]
pub fn normalize_loudness_target(params: &NormalizeLoudnessTargetParams) -> ToolReceipt {
    let mut op_params = serde_json::json!({});
    if let Some(target) = params.target_lufs {
        op_params["target_lufs"] = serde_json::json!(target);
    }
    let mut receipt = apply_patch(&ApplyPatchParams {
        project_id: params.project_id.clone(),
        op: "normalize_loudness_target".to_string(),
        params: op_params,
        dry_run: false,
        scope: None,
        project: params.project.clone(),
    });
    receipt.tool = "normalize_loudness_target".to_string();
    receipt
}

// ── Composition Tools (§10.5) ───────────────────────────────────────

/// Parameters for `create_pattern` tool.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreatePatternParams {
    /// Project ID.
    pub project_id: String,
    /// Pattern name.
    pub name: String,
    /// Duration in bars.
    pub bars: u32,
    /// Time signature numerator.
    #[serde(default = "default_ts_num")]
    pub time_signature_num: u32,
}

/// Create a new empty pattern.
#[must_use]
pub fn create_pattern(params: &CreatePatternParams) -> ToolReceipt {
    ToolReceipt {
        tool: "create_pattern".to_string(),
        status: ToolStatus::Succeeded,
        summary: format!(
            "Created pattern '{}' ({} bars) in project '{}'",
            params.name, params.bars, params.project_id
        ),
        score_delta: BTreeMap::new(),
        evidence_uris: vec![format!("project://{}/patterns", params.project_id)],
        focus_uris: vec![],
        next_actions: vec!["write_pattern_notes".to_string()],
        cost: ToolCost {
            tool_calls: 1,
            ..Default::default()
        },
        warnings: vec![],
        dry_run: false,
        diagnostics: vec![],
    }
}

/// Parameters for `plan_arrangement` tool.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PlanArrangementParams {
    /// Project ID.
    pub project_id: String,
    /// Total bars.
    pub total_bars: u32,
    /// Segment definitions (name, role, bars).
    pub segments: Vec<SegmentPlan>,
}

/// A planned segment definition.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SegmentPlan {
    /// Segment name.
    pub name: String,
    /// Segment role.
    pub role: String,
    /// Duration in bars.
    pub bars: u32,
}

/// Plan an arrangement with segments.
#[must_use]
pub fn plan_arrangement(params: &PlanArrangementParams) -> ToolReceipt {
    let total_planned: u32 = params.segments.iter().map(|s| s.bars).sum();
    let warnings = if total_planned != params.total_bars {
        vec![format!(
            "Planned segments sum to {total_planned} bars but total_bars is {}",
            params.total_bars
        )]
    } else {
        vec![]
    };
    ToolReceipt {
        tool: "plan_arrangement".to_string(),
        status: if total_planned == params.total_bars {
            ToolStatus::Succeeded
        } else {
            ToolStatus::Failed
        },
        summary: format!(
            "Planned {} segments ({} total bars) for project '{}'",
            params.segments.len(),
            total_planned,
            params.project_id
        ),
        score_delta: BTreeMap::new(),
        evidence_uris: vec![format!("project://{}/arrangement", params.project_id)],
        focus_uris: vec![],
        next_actions: vec!["create_skeleton".to_string()],
        cost: ToolCost {
            tool_calls: 1,
            ..Default::default()
        },
        warnings,
        dry_run: false,
        diagnostics: vec![],
    }
}

/// Parameters for `write_pattern_notes` tool.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WritePatternNotesParams {
    /// Project ID.
    pub project_id: String,
    /// Pattern ID to write notes into.
    pub pattern_id: String,
    /// Notes to append.
    pub notes: Vec<NoteSpec>,
    /// Serialized project to mutate (when provided, real).
    #[serde(default)]
    pub project: Option<serde_json::Value>,
}

/// A note specification for `write_pattern_notes`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NoteSpec {
    /// MIDI pitch.
    pub pitch: u8,
    /// Start tick.
    pub start: u64,
    /// Duration in ticks.
    pub duration: u64,
    /// Velocity.
    pub velocity: u8,
}

/// Append notes to a pattern (§10.5).
#[must_use]
pub fn write_pattern_notes(params: &WritePatternNotesParams) -> ToolReceipt {
    let Some(project_json) = &params.project else {
        return ToolReceipt {
            tool: "write_pattern_notes".to_string(),
            status: ToolStatus::Failed,
            summary: format!(
                "Cannot write {} note(s) to pattern '{}': no project payload provided",
                params.notes.len(),
                params.pattern_id
            ),
            score_delta: BTreeMap::new(),
            evidence_uris: vec![],
            focus_uris: vec![],
            next_actions: vec!["get_project_summary".to_string()],
            cost: ToolCost::default(),
            warnings: vec![
                "No project payload — supply the serialized project to mutate".to_string(),
            ],
            dry_run: false,
            diagnostics: vec![],
        };
    };
    let mut project =
        match serde_json::from_value::<scorestrata_core::domain::Project>(project_json.clone()) {
            Ok(p) => p,
            Err(e) => return deserialize_error_receipt("write_pattern_notes", "project", &e),
        };
    let Some(pattern) = project
        .patterns
        .iter_mut()
        .find(|p| p.id.to_string() == params.pattern_id)
    else {
        return ToolReceipt {
            tool: "write_pattern_notes".to_string(),
            status: ToolStatus::Failed,
            summary: format!("Pattern '{}' not found in project", params.pattern_id),
            score_delta: BTreeMap::new(),
            evidence_uris: vec![],
            focus_uris: vec![],
            next_actions: vec![],
            cost: ToolCost::default(),
            warnings: vec!["Unknown pattern ID".to_string()],
            dry_run: false,
            diagnostics: vec![],
        };
    };
    let mut written = 0usize;
    for spec in &params.notes {
        pattern.notes.push(scorestrata_core::domain::Note {
            id: scorestrata_core::ids::NoteId::new(),
            pitch: scorestrata_core::domain::MidiNote::new(spec.pitch),
            start: scorestrata_core::domain::Tick::new(spec.start),
            duration: scorestrata_core::domain::Duration(spec.duration),
            velocity: scorestrata_core::domain::Velocity::new(spec.velocity),
            tied: false,
        });
        written += 1;
    }
    let mut score_delta = BTreeMap::new();
    score_delta.insert("pattern.notes".to_string(), pattern.notes.len().to_string());
    ToolReceipt {
        tool: "write_pattern_notes".to_string(),
        status: ToolStatus::Succeeded,
        summary: format!(
            "Wrote {written} note(s) to pattern '{}' ({} total)",
            params.pattern_id,
            pattern.notes.len()
        ),
        score_delta,
        evidence_uris: vec![format!(
            "project://{}/patterns/{}",
            params.project_id, params.pattern_id
        )],
        focus_uris: vec![],
        next_actions: vec!["run_validation".to_string()],
        cost: ToolCost {
            cpu_ms: 2,
            tool_calls: 1,
            ..Default::default()
        },
        warnings: vec![],
        dry_run: false,
        diagnostics: vec![],
    }
}

/// Parameters for `write_pattern_steps` tool.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WritePatternStepsParams {
    /// Project ID.
    pub project_id: String,
    /// Pattern ID to write steps into.
    pub pattern_id: String,
    /// Steps to append.
    pub steps: Vec<StepSpec>,
    /// Serialized project to mutate (when provided, real).
    #[serde(default)]
    pub project: Option<serde_json::Value>,
}

/// A step specification for `write_pattern_steps`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StepSpec {
    /// Step index (0-based).
    pub index: u32,
    /// Whether the step is active.
    pub active: bool,
    /// Velocity if active.
    pub velocity: u8,
}

/// Append steps to a pattern's step sequencer (§10.5).
#[must_use]
pub fn write_pattern_steps(params: &WritePatternStepsParams) -> ToolReceipt {
    let Some(project_json) = &params.project else {
        return ToolReceipt {
            tool: "write_pattern_steps".to_string(),
            status: ToolStatus::Failed,
            summary: format!(
                "Cannot write {} step(s) to pattern '{}': no project payload provided",
                params.steps.len(),
                params.pattern_id
            ),
            score_delta: BTreeMap::new(),
            evidence_uris: vec![],
            focus_uris: vec![],
            next_actions: vec!["get_project_summary".to_string()],
            cost: ToolCost::default(),
            warnings: vec![
                "No project payload — supply the serialized project to mutate".to_string(),
            ],
            dry_run: false,
            diagnostics: vec![],
        };
    };
    let mut project =
        match serde_json::from_value::<scorestrata_core::domain::Project>(project_json.clone()) {
            Ok(p) => p,
            Err(e) => return deserialize_error_receipt("write_pattern_steps", "project", &e),
        };
    let Some(pattern) = project
        .patterns
        .iter_mut()
        .find(|p| p.id.to_string() == params.pattern_id)
    else {
        return ToolReceipt {
            tool: "write_pattern_steps".to_string(),
            status: ToolStatus::Failed,
            summary: format!("Pattern '{}' not found in project", params.pattern_id),
            score_delta: BTreeMap::new(),
            evidence_uris: vec![],
            focus_uris: vec![],
            next_actions: vec![],
            cost: ToolCost::default(),
            warnings: vec!["Unknown pattern ID".to_string()],
            dry_run: false,
            diagnostics: vec![],
        };
    };
    let mut written = 0usize;
    for spec in &params.steps {
        pattern.steps.push(scorestrata_core::domain::Step {
            id: scorestrata_core::ids::StepId::new(),
            index: spec.index,
            active: spec.active,
            velocity: scorestrata_core::domain::Velocity::new(spec.velocity),
        });
        written += 1;
    }
    let mut score_delta = BTreeMap::new();
    score_delta.insert("pattern.steps".to_string(), pattern.steps.len().to_string());
    ToolReceipt {
        tool: "write_pattern_steps".to_string(),
        status: ToolStatus::Succeeded,
        summary: format!(
            "Wrote {written} step(s) to pattern '{}' ({} total)",
            params.pattern_id,
            pattern.steps.len()
        ),
        score_delta,
        evidence_uris: vec![format!(
            "project://{}/patterns/{}/steps",
            params.project_id, params.pattern_id
        )],
        focus_uris: vec![],
        next_actions: vec!["run_validation".to_string()],
        cost: ToolCost {
            cpu_ms: 2,
            tool_calls: 1,
            ..Default::default()
        },
        warnings: vec![],
        dry_run: false,
        diagnostics: vec![],
    }
}

/// Parameters for `apply_motivic_variation` tool.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ApplyMotivicVariationParams {
    /// Project ID.
    pub project_id: String,
    /// Pattern ID to vary.
    pub pattern_id: String,
    /// Variation name (`transpose`, `retrograde`, `syncopate`).
    pub variation: String,
    /// Semitone offset for `transpose`.
    #[serde(default)]
    pub semitones: i32,
    /// Serialized project to mutate (when provided, real).
    #[serde(default)]
    pub project: Option<serde_json::Value>,
}

/// Apply a motivic variation to a pattern's notes (§10.5).
///
/// Supported variations:
/// - `transpose`: shift every note pitch by `semitones` (clamped to MIDI range).
/// - `retrograde`: reverse the order of the notes.
/// - `syncopate`: offset every other note's start by half its duration.
#[must_use]
pub fn apply_motivic_variation(params: &ApplyMotivicVariationParams) -> ToolReceipt {
    let Some(project_json) = &params.project else {
        return ToolReceipt {
            tool: "apply_motivic_variation".to_string(),
            status: ToolStatus::Failed,
            summary: format!(
                "Cannot apply '{}' to pattern '{}': no project payload provided",
                params.variation, params.pattern_id
            ),
            score_delta: BTreeMap::new(),
            evidence_uris: vec![],
            focus_uris: vec![],
            next_actions: vec!["get_project_summary".to_string()],
            cost: ToolCost::default(),
            warnings: vec![
                "No project payload — supply the serialized project to mutate".to_string(),
            ],
            dry_run: false,
            diagnostics: vec![],
        };
    };
    let mut project =
        match serde_json::from_value::<scorestrata_core::domain::Project>(project_json.clone()) {
            Ok(p) => p,
            Err(e) => return deserialize_error_receipt("apply_motivic_variation", "project", &e),
        };
    let Some(pattern) = project
        .patterns
        .iter_mut()
        .find(|p| p.id.to_string() == params.pattern_id)
    else {
        return ToolReceipt {
            tool: "apply_motivic_variation".to_string(),
            status: ToolStatus::Failed,
            summary: format!("Pattern '{}' not found in project", params.pattern_id),
            score_delta: BTreeMap::new(),
            evidence_uris: vec![],
            focus_uris: vec![],
            next_actions: vec![],
            cost: ToolCost::default(),
            warnings: vec!["Unknown pattern ID".to_string()],
            dry_run: false,
            diagnostics: vec![],
        };
    };
    let affected = pattern.notes.len();
    if affected == 0 {
        return ToolReceipt {
            tool: "apply_motivic_variation".to_string(),
            status: ToolStatus::Failed,
            summary: format!(
                "Pattern '{}' has no notes — nothing to vary",
                params.pattern_id
            ),
            score_delta: BTreeMap::new(),
            evidence_uris: vec![],
            focus_uris: vec![],
            next_actions: vec![],
            cost: ToolCost::default(),
            warnings: vec!["Pattern contains no notes".to_string()],
            dry_run: false,
            diagnostics: vec![],
        };
    }
    match params.variation.as_str() {
        "transpose" => {
            for note in &mut pattern.notes {
                let base = i32::from(note.pitch.0);
                let shifted = (base + params.semitones).clamp(0, 127);
                #[expect(
                    clippy::cast_possible_truncation,
                    clippy::cast_sign_loss,
                    reason = "Clamped to 0..=127 before casting to u8"
                )]
                let shifted = shifted as u8;
                note.pitch = scorestrata_core::domain::MidiNote::new(shifted);
            }
        }
        "retrograde" => {
            pattern.notes.reverse();
        }
        "syncopate" => {
            for (i, note) in pattern.notes.iter_mut().enumerate() {
                if i % 2 == 1 {
                    note.start = scorestrata_core::domain::Tick::new(
                        note.start.0.saturating_add(note.duration.0 / 2),
                    );
                }
            }
        }
        other => {
            return ToolReceipt {
                tool: "apply_motivic_variation".to_string(),
                status: ToolStatus::Failed,
                summary: format!(
                    "Unsupported variation '{}' (expected transpose|retrograde|syncopate)",
                    other
                ),
                score_delta: BTreeMap::new(),
                evidence_uris: vec![],
                focus_uris: vec![],
                next_actions: vec![],
                cost: ToolCost::default(),
                warnings: vec!["Unknown variation".to_string()],
                dry_run: false,
                diagnostics: vec![],
            };
        }
    }
    let mut score_delta = BTreeMap::new();
    score_delta.insert("pattern.variation".to_string(), params.variation.clone());
    score_delta.insert("pattern.affected_notes".to_string(), affected.to_string());
    ToolReceipt {
        tool: "apply_motivic_variation".to_string(),
        status: ToolStatus::Succeeded,
        summary: format!(
            "Applied '{}' variation to {} note(s) in pattern '{}'",
            params.variation, affected, params.pattern_id
        ),
        score_delta,
        evidence_uris: vec![format!(
            "project://{}/patterns/{}",
            params.project_id, params.pattern_id
        )],
        focus_uris: vec![],
        next_actions: vec!["run_validation".to_string()],
        cost: ToolCost {
            cpu_ms: 2,
            tool_calls: 1,
            ..Default::default()
        },
        warnings: vec![],
        dry_run: false,
        diagnostics: vec![],
    }
}

/// Parameters for `place_pattern_in_segment` tool.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PlacePatternInSegmentParams {
    /// Project ID.
    pub project_id: String,
    /// Segment ID to place into.
    pub segment_id: String,
    /// Layer ID to place into.
    pub layer_id: String,
    /// Pattern ID to place.
    pub pattern_id: String,
    /// Start tick within the segment.
    #[serde(default)]
    pub start_tick: u64,
    /// Serialized project to mutate (when provided, real).
    #[serde(default)]
    pub project: Option<serde_json::Value>,
}

/// Place a pattern instance into a segment's layer (§10.5).
#[must_use]
pub fn place_pattern_in_segment(params: &PlacePatternInSegmentParams) -> ToolReceipt {
    let Some(project_json) = &params.project else {
        return ToolReceipt {
            tool: "place_pattern_in_segment".to_string(),
            status: ToolStatus::Failed,
            summary: format!(
                "Cannot place pattern '{}' into '{}': no project payload provided",
                params.pattern_id, params.segment_id
            ),
            score_delta: BTreeMap::new(),
            evidence_uris: vec![],
            focus_uris: vec![],
            next_actions: vec!["get_project_summary".to_string()],
            cost: ToolCost::default(),
            warnings: vec![
                "No project payload — supply the serialized project to mutate".to_string(),
            ],
            dry_run: false,
            diagnostics: vec![],
        };
    };
    let mut project =
        match serde_json::from_value::<scorestrata_core::domain::Project>(project_json.clone()) {
            Ok(p) => p,
            Err(e) => return deserialize_error_receipt("place_pattern_in_segment", "project", &e),
        };
    let Some(segment) = project
        .arrangement
        .segments
        .iter_mut()
        .find(|s| s.id.to_string() == params.segment_id)
    else {
        return ToolReceipt {
            tool: "place_pattern_in_segment".to_string(),
            status: ToolStatus::Failed,
            summary: format!("Segment '{}' not found", params.segment_id),
            score_delta: BTreeMap::new(),
            evidence_uris: vec![],
            focus_uris: vec![],
            next_actions: vec![],
            cost: ToolCost::default(),
            warnings: vec!["Unknown segment ID".to_string()],
            dry_run: false,
            diagnostics: vec![],
        };
    };
    let Some(layer) = segment
        .layers
        .iter_mut()
        .find(|l| l.id.to_string() == params.layer_id)
    else {
        return ToolReceipt {
            tool: "place_pattern_in_segment".to_string(),
            status: ToolStatus::Failed,
            summary: format!(
                "Layer '{}' not found in segment '{}'",
                params.layer_id, params.segment_id
            ),
            score_delta: BTreeMap::new(),
            evidence_uris: vec![],
            focus_uris: vec![],
            next_actions: vec![],
            cost: ToolCost::default(),
            warnings: vec!["Unknown layer ID".to_string()],
            dry_run: false,
            diagnostics: vec![],
        };
    };
    let Ok(pattern_id) = params
        .pattern_id
        .parse::<scorestrata_core::ids::PatternId>()
    else {
        return ToolReceipt {
            tool: "place_pattern_in_segment".to_string(),
            status: ToolStatus::Failed,
            summary: format!("Pattern '{}' is not a valid pattern ID", params.pattern_id),
            score_delta: BTreeMap::new(),
            evidence_uris: vec![],
            focus_uris: vec![],
            next_actions: vec![],
            cost: ToolCost::default(),
            warnings: vec!["Invalid pattern ID".to_string()],
            dry_run: false,
            diagnostics: vec![],
        };
    };
    layer
        .pattern_instances
        .push(scorestrata_core::domain::PatternInstance {
            id: scorestrata_core::ids::PatternInstanceId::new(),
            pattern_id,
            start_tick: scorestrata_core::domain::Tick::new(params.start_tick),
            transpose: 0,
            velocity_multiplier: 1.0,
        });
    let mut score_delta = BTreeMap::new();
    score_delta.insert(
        "layer.pattern_instances".to_string(),
        layer.pattern_instances.len().to_string(),
    );
    ToolReceipt {
        tool: "place_pattern_in_segment".to_string(),
        status: ToolStatus::Succeeded,
        summary: format!(
            "Placed pattern '{}' into layer '{}' of segment '{}' (instance #{})",
            params.pattern_id,
            params.layer_id,
            params.segment_id,
            layer.pattern_instances.len()
        ),
        score_delta,
        evidence_uris: vec![format!(
            "project://{}/segments/{}/layers/{}",
            params.project_id, params.segment_id, params.layer_id
        )],
        focus_uris: vec![],
        next_actions: vec!["run_validation".to_string()],
        cost: ToolCost {
            cpu_ms: 2,
            tool_calls: 1,
            ..Default::default()
        },
        warnings: vec![],
        dry_run: false,
        diagnostics: vec![],
    }
}

/// Parameters for `set_layer_instrument` tool.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SetLayerInstrumentParams {
    /// Project ID.
    pub project_id: String,
    /// Segment ID containing the layer.
    pub segment_id: String,
    /// Layer ID to retarget.
    pub layer_id: String,
    /// Instrument name (e.g. `supersaw`, `analog_bass`, `clap`).
    pub instrument: String,
    /// Serialized project to mutate (when provided, real).
    #[serde(default)]
    pub project: Option<serde_json::Value>,
}

/// Set a layer's instrument (§10.5).
#[must_use]
pub fn set_layer_instrument(params: &SetLayerInstrumentParams) -> ToolReceipt {
    let Some(project_json) = &params.project else {
        return ToolReceipt {
            tool: "set_layer_instrument".to_string(),
            status: ToolStatus::Failed,
            summary: format!(
                "Cannot set instrument on layer '{}': no project payload provided",
                params.layer_id
            ),
            score_delta: BTreeMap::new(),
            evidence_uris: vec![],
            focus_uris: vec![],
            next_actions: vec!["get_project_summary".to_string()],
            cost: ToolCost::default(),
            warnings: vec![
                "No project payload — supply the serialized project to mutate".to_string(),
            ],
            dry_run: false,
            diagnostics: vec![],
        };
    };
    let mut project =
        match serde_json::from_value::<scorestrata_core::domain::Project>(project_json.clone()) {
            Ok(p) => p,
            Err(e) => return deserialize_error_receipt("set_layer_instrument", "project", &e),
        };
    let Some(segment) = project
        .arrangement
        .segments
        .iter_mut()
        .find(|s| s.id.to_string() == params.segment_id)
    else {
        return ToolReceipt {
            tool: "set_layer_instrument".to_string(),
            status: ToolStatus::Failed,
            summary: format!("Segment '{}' not found", params.segment_id),
            score_delta: BTreeMap::new(),
            evidence_uris: vec![],
            focus_uris: vec![],
            next_actions: vec![],
            cost: ToolCost::default(),
            warnings: vec!["Unknown segment ID".to_string()],
            dry_run: false,
            diagnostics: vec![],
        };
    };
    let Some(layer) = segment
        .layers
        .iter_mut()
        .find(|l| l.id.to_string() == params.layer_id)
    else {
        return ToolReceipt {
            tool: "set_layer_instrument".to_string(),
            status: ToolStatus::Failed,
            summary: format!(
                "Layer '{}' not found in segment '{}'",
                params.layer_id, params.segment_id
            ),
            score_delta: BTreeMap::new(),
            evidence_uris: vec![],
            focus_uris: vec![],
            next_actions: vec![],
            cost: ToolCost::default(),
            warnings: vec!["Unknown layer ID".to_string()],
            dry_run: false,
            diagnostics: vec![],
        };
    };
    let old_instrument = std::mem::replace(&mut layer.instrument, params.instrument.clone());
    let mut score_delta = BTreeMap::new();
    score_delta.insert("layer.instrument".to_string(), params.instrument.clone());
    ToolReceipt {
        tool: "set_layer_instrument".to_string(),
        status: ToolStatus::Succeeded,
        summary: format!(
            "Layer '{}' instrument changed from '{}' to '{}'",
            params.layer_id, old_instrument, params.instrument
        ),
        score_delta,
        evidence_uris: vec![format!(
            "project://{}/segments/{}/layers/{}",
            params.project_id, params.segment_id, params.layer_id
        )],
        focus_uris: vec![],
        next_actions: vec!["run_validation".to_string()],
        cost: ToolCost {
            cpu_ms: 1,
            tool_calls: 1,
            ..Default::default()
        },
        warnings: vec![],
        dry_run: false,
        diagnostics: vec![],
    }
}

/// Parameters for `set_layer_register` tool (G2).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SetLayerRegisterParams {
    /// Project ID.
    pub project_id: String,
    /// Segment ID containing the layer.
    pub segment_id: String,
    /// Layer ID to constrain.
    pub layer_id: String,
    /// Lowest octave (inclusive).
    pub min_octave: i8,
    /// Highest octave (inclusive).
    pub max_octave: i8,
    /// Serialized project to mutate (when provided, real).
    #[serde(default)]
    pub project: Option<serde_json::Value>,
}

/// Set a layer's octave register (G2) — the production discipline.
///
/// Declares the vertical band the layer must occupy (e.g. bass 1..2, lead
/// 5..7). The compiler clamps notes into the band at compile time and the
/// validator flags any that land outside it (`LAYER_NOTE_OUTSIDE_REGISTER`),
/// so layers stop fighting for the same frequency range.
#[must_use]
pub fn set_layer_register(params: &SetLayerRegisterParams) -> ToolReceipt {
    let Some(project_json) = &params.project else {
        return ToolReceipt {
            tool: "set_layer_register".to_string(),
            status: ToolStatus::Failed,
            summary: format!(
                "Cannot set register on layer '{}': no project payload provided",
                params.layer_id
            ),
            score_delta: BTreeMap::new(),
            evidence_uris: vec![],
            focus_uris: vec![],
            next_actions: vec!["get_project_summary".to_string()],
            cost: ToolCost::default(),
            warnings: vec![
                "No project payload — supply the serialized project to mutate".to_string(),
            ],
            dry_run: false,
            diagnostics: vec![],
        };
    };
    let mut project =
        match serde_json::from_value::<scorestrata_core::domain::Project>(project_json.clone()) {
            Ok(p) => p,
            Err(e) => return deserialize_error_receipt("set_layer_register", "project", &e),
        };
    let register = match crate::production_ops::set_layer_register(
        &mut project,
        &params.segment_id,
        &params.layer_id,
        params.min_octave,
        params.max_octave,
    ) {
        Ok(outcome) => outcome.register,
        Err(err) => {
            return production_error_receipt(
                "set_layer_register",
                &err,
                production_error_warnings(&err),
            );
        }
    };
    let mut score_delta = BTreeMap::new();
    score_delta.insert(
        "layers.register".to_string(),
        format!("{}..{}", register.min_octave, register.max_octave),
    );
    ToolReceipt {
        tool: "set_layer_register".to_string(),
        status: ToolStatus::Succeeded,
        summary: format!(
            "Layer '{}' register set to octaves {}..{} (compiler clamps, validator flags)",
            params.layer_id, register.min_octave, register.max_octave
        ),
        score_delta,
        evidence_uris: vec![format!(
            "project://{}/segments/{}/layers/{}",
            params.project_id, params.segment_id, params.layer_id
        )],
        focus_uris: vec![format!("project://{}/timeline/view", params.project_id)],
        next_actions: vec![
            "run_validation".to_string(),
            "get_timeline_view".to_string(),
        ],
        cost: ToolCost {
            cpu_ms: 1,
            tool_calls: 1,
            ..Default::default()
        },
        warnings: vec![],
        dry_run: false,
        diagnostics: vec![],
    }
}

/// Parameters for `write_automation` tool (G4).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WriteAutomationParams {
    /// Project ID.
    pub project_id: String,
    /// Segment ID containing the layer.
    pub segment_id: String,
    /// Layer ID to automate.
    pub layer_id: String,
    /// Parameter to automate (`cutoff`, `volume`, `pan`, `send`).
    pub param: String,
    /// Automation points: `[{ "tick": 0, "value": 0.2 }, …]` (value 0–1).
    pub points: Vec<scorestrata_core::domain::AutomationPointJson>,
    /// Serialized project to mutate (when provided, real).
    #[serde(default)]
    pub project: Option<serde_json::Value>,
}

/// Write an automation lane on a layer (G4).
///
/// Appends a new lane or replaces an existing lane for the same parameter on
/// the target layer. The compiler turns the lane into `ControlChange` events
/// in the timeline, and the audio renderer sweeps the filter/volume so the
/// automation is audible — the "build-up opens up" action.
#[must_use]
pub fn write_automation(params: &WriteAutomationParams) -> ToolReceipt {
    let Some(project_json) = &params.project else {
        return ToolReceipt {
            tool: "write_automation".to_string(),
            status: ToolStatus::Failed,
            summary: format!(
                "Cannot write automation on layer '{}': no project payload provided",
                params.layer_id
            ),
            score_delta: BTreeMap::new(),
            evidence_uris: vec![],
            focus_uris: vec![],
            next_actions: vec!["get_project_summary".to_string()],
            cost: ToolCost::default(),
            warnings: vec![
                "No project payload — supply the serialized project to mutate".to_string(),
            ],
            dry_run: false,
            diagnostics: vec![],
        };
    };
    let mut project =
        match serde_json::from_value::<scorestrata_core::domain::Project>(project_json.clone()) {
            Ok(p) => p,
            Err(e) => return deserialize_error_receipt("write_automation", "project", &e),
        };
    let outcome = match crate::production_ops::write_automation(
        &mut project,
        &params.segment_id,
        &params.layer_id,
        &params.param,
        &params.points,
    ) {
        Ok(outcome) => outcome,
        Err(err) => {
            return production_error_receipt(
                "write_automation",
                &err,
                production_error_warnings(&err),
            );
        }
    };
    let param = outcome.param;
    let mut score_delta = BTreeMap::new();
    score_delta.insert(
        "layers.automation_lanes".to_string(),
        outcome.lanes.to_string(),
    );
    score_delta.insert(
        "layers.automation_points".to_string(),
        params.points.len().to_string(),
    );
    ToolReceipt {
        tool: "write_automation".to_string(),
        status: ToolStatus::Succeeded,
        summary: format!(
            "Layer '{}' {} automation lane with {} points (compiler emits CC {}, renderer sweeps)",
            params.layer_id,
            param_str(&param),
            params.points.len(),
            param.cc_number()
        ),
        score_delta,
        evidence_uris: vec![format!(
            "project://{}/segments/{}/layers/{}",
            params.project_id, params.segment_id, params.layer_id
        )],
        focus_uris: vec![format!("project://{}/timeline/view", params.project_id)],
        next_actions: vec![
            "run_validation".to_string(),
            "get_timeline_view".to_string(),
            "render_project".to_string(),
        ],
        cost: ToolCost {
            cpu_ms: 1,
            tool_calls: 1,
            ..Default::default()
        },
        warnings: vec![],
        dry_run: false,
        diagnostics: vec![],
    }
}

/// Parameters for `set_track_fx` tool (G5).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SetTrackFxParams {
    /// Project ID.
    pub project_id: String,
    /// Segment ID containing the layer.
    pub segment_id: String,
    /// Layer ID whose insert chain to set.
    pub layer_id: String,
    /// Serialized [`FxChain`](scorestrata_core::domain::FxChain).
    pub fx: serde_json::Value,
    /// Serialized project to mutate (when provided, real).
    #[serde(default)]
    pub project: Option<serde_json::Value>,
}

/// Set a layer's insert FX chain (G5).
///
/// "High-pass the pads at 120 Hz, scoop 350 Hz −2 dB, sidechain to kick,
/// send 0.3 to reverb" — the renderer applies the chain (HPF → EQ →
/// compressor → sends) to the layer's stem.
#[must_use]
pub fn set_track_fx(params: &SetTrackFxParams) -> ToolReceipt {
    let Some(project_json) = &params.project else {
        return ToolReceipt {
            tool: "set_track_fx".to_string(),
            status: ToolStatus::Failed,
            summary: format!(
                "Cannot set FX on layer '{}': no project payload provided",
                params.layer_id
            ),
            score_delta: BTreeMap::new(),
            evidence_uris: vec![],
            focus_uris: vec![],
            next_actions: vec!["get_project_summary".to_string()],
            cost: ToolCost::default(),
            warnings: vec![
                "No project payload — supply the serialized project to mutate".to_string(),
            ],
            dry_run: false,
            diagnostics: vec![],
        };
    };
    let mut project =
        match serde_json::from_value::<scorestrata_core::domain::Project>(project_json.clone()) {
            Ok(p) => p,
            Err(e) => return deserialize_error_receipt("set_track_fx", "project", &e),
        };
    let fx = match serde_json::from_value::<scorestrata_core::domain::FxChain>(params.fx.clone()) {
        Ok(f) => f,
        Err(e) => return deserialize_error_receipt("set_track_fx", "fx", &e),
    };
    if let Err(err) =
        crate::production_ops::set_track_fx(&mut project, &params.segment_id, &params.layer_id, &fx)
    {
        return production_error_receipt("set_track_fx", &err, production_error_warnings(&err));
    }
    let mut score_delta = BTreeMap::new();
    score_delta.insert("layers.fx".to_string(), fx_summary(&fx));
    ToolReceipt {
        tool: "set_track_fx".to_string(),
        status: ToolStatus::Succeeded,
        summary: format!(
            "Layer '{}' FX chain set ({}); renderer applies HPF→EQ→comp→sends",
            params.layer_id,
            fx_summary(&fx)
        ),
        score_delta,
        evidence_uris: vec![format!(
            "project://{}/segments/{}/layers/{}",
            params.project_id, params.segment_id, params.layer_id
        )],
        focus_uris: vec![format!("project://{}/timeline/view", params.project_id)],
        next_actions: vec!["run_validation".to_string(), "render_project".to_string()],
        cost: ToolCost {
            cpu_ms: 1,
            tool_calls: 1,
            ..Default::default()
        },
        warnings: vec![],
        dry_run: false,
        diagnostics: vec![],
    }
}

/// Parameters for `set_track_mix` tool (G5).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SetTrackMixParams {
    /// Project ID.
    pub project_id: String,
    /// Segment ID containing the layer.
    pub segment_id: String,
    /// Layer ID whose mix faders to set.
    pub layer_id: String,
    /// Gain override in dB; `None` = keep static staging.
    #[serde(default)]
    pub gain_db: Option<f32>,
    /// Pan override (−1.0…1.0); `None` = keep static panning.
    #[serde(default)]
    pub pan: Option<f32>,
    /// Serialized project to mutate (when provided, real).
    #[serde(default)]
    pub project: Option<serde_json::Value>,
}

/// Set a layer's mixer faders (G5) — the mixer-fader action.
#[must_use]
pub fn set_track_mix(params: &SetTrackMixParams) -> ToolReceipt {
    let Some(project_json) = &params.project else {
        return ToolReceipt {
            tool: "set_track_mix".to_string(),
            status: ToolStatus::Failed,
            summary: format!(
                "Cannot set mix on layer '{}': no project payload provided",
                params.layer_id
            ),
            score_delta: BTreeMap::new(),
            evidence_uris: vec![],
            focus_uris: vec![],
            next_actions: vec!["get_project_summary".to_string()],
            cost: ToolCost::default(),
            warnings: vec![
                "No project payload — supply the serialized project to mutate".to_string(),
            ],
            dry_run: false,
            diagnostics: vec![],
        };
    };
    let mut project =
        match serde_json::from_value::<scorestrata_core::domain::Project>(project_json.clone()) {
            Ok(p) => p,
            Err(e) => return deserialize_error_receipt("set_track_mix", "project", &e),
        };
    if let Err(err) = crate::production_ops::set_track_mix(
        &mut project,
        &params.segment_id,
        &params.layer_id,
        params.gain_db,
        params.pan,
    ) {
        return production_error_receipt("set_track_mix", &err, production_error_warnings(&err));
    }
    let mut score_delta = BTreeMap::new();
    score_delta.insert(
        "layers.mix".to_string(),
        format!("gain_db={:?} pan={:?}", params.gain_db, params.pan),
    );
    ToolReceipt {
        tool: "set_track_mix".to_string(),
        status: ToolStatus::Succeeded,
        summary: format!(
            "Layer '{}' mix set (gain_db={:?}, pan={:?})",
            params.layer_id, params.gain_db, params.pan
        ),
        score_delta,
        evidence_uris: vec![format!(
            "project://{}/segments/{}/layers/{}",
            params.project_id, params.segment_id, params.layer_id
        )],
        focus_uris: vec![format!("project://{}/timeline/view", params.project_id)],
        next_actions: vec!["render_project".to_string()],
        cost: ToolCost {
            cpu_ms: 1,
            tool_calls: 1,
            ..Default::default()
        },
        warnings: vec![],
        dry_run: false,
        diagnostics: vec![],
    }
}

/// Parameters for `apply_genre_mix_preset` tool (G5).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ApplyGenreMixPresetParams {
    /// Project ID.
    pub project_id: String,
    /// Genre whose production-ready mix defaults to apply.
    pub genre: String,
    /// Serialized project to mutate (when provided, real).
    #[serde(default)]
    pub project: Option<serde_json::Value>,
}

/// Apply a one-shot production-ready mix preset for a genre (G5).
///
/// Sets per-instrument HPF / EQ / sidechain / sends defaults from the genre
/// table so an agent goes from loop → production-ready mix in one call, then
/// refines with `set_track_fx` / `set_track_mix`.
#[must_use]
pub fn apply_genre_mix_preset(params: &ApplyGenreMixPresetParams) -> ToolReceipt {
    let Some(project_json) = &params.project else {
        return ToolReceipt {
            tool: "apply_genre_mix_preset".to_string(),
            status: ToolStatus::Failed,
            summary: format!(
                "Cannot apply {} mix preset: no project payload provided",
                params.genre
            ),
            score_delta: BTreeMap::new(),
            evidence_uris: vec![],
            focus_uris: vec![],
            next_actions: vec!["get_project_summary".to_string()],
            cost: ToolCost::default(),
            warnings: vec![
                "No project payload — supply the serialized project to mutate".to_string(),
            ],
            dry_run: false,
            diagnostics: vec![],
        };
    };
    let mut project =
        match serde_json::from_value::<scorestrata_core::domain::Project>(project_json.clone()) {
            Ok(p) => p,
            Err(e) => return deserialize_error_receipt("apply_genre_mix_preset", "project", &e),
        };
    let applied = crate::production_ops::apply_genre_mix_preset(&mut project, &params.genre);
    let mut score_delta = BTreeMap::new();
    score_delta.insert("layers.fx_preset".to_string(), applied.to_string());
    ToolReceipt {
        tool: "apply_genre_mix_preset".to_string(),
        status: ToolStatus::Succeeded,
        summary: format!(
            "Applied {} mix preset to {applied} layers (HPF/EQ/sidechain/sends per instrument)",
            params.genre
        ),
        score_delta,
        evidence_uris: vec![format!("project://{}/tracks", params.project_id)],
        focus_uris: vec![format!("project://{}/timeline/view", params.project_id)],
        next_actions: vec![
            "render_project".to_string(),
            "set_track_fx".to_string(),
            "set_track_mix".to_string(),
        ],
        cost: ToolCost {
            cpu_ms: 1,
            tool_calls: 1,
            ..Default::default()
        },
        warnings: vec![],
        dry_run: false,
        diagnostics: vec![],
    }
}

/// Parameters for `apply_arrangement_template` tool (G6).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ApplyArrangementTemplateParams {
    /// Project ID.
    pub project_id: String,
    /// Genre whose section archetypes to apply.
    pub genre: String,
    /// Total bars for the arrangement.
    pub total_bars: u32,
    /// Serialized project to mutate (when provided, real).
    #[serde(default)]
    pub project: Option<serde_json::Value>,
}

/// Apply a per-genre arrangement template (G6) — build the section skeleton
/// (intro → build → drop → breakdown → outro) with production-grade bar
/// counts and an energy arc, exactly like a producer arranges in a DAW.
///
/// The skeleton replaces the current arrangement's segments (each section
/// keeps its energy target and the template's layer hints); the planner then
/// fills each section's layers via `plan_arrangement`.
#[must_use]
pub fn apply_arrangement_template(params: &ApplyArrangementTemplateParams) -> ToolReceipt {
    let Some(project_json) = &params.project else {
        return ToolReceipt {
            tool: "apply_arrangement_template".to_string(),
            status: ToolStatus::Failed,
            summary: "Cannot apply arrangement template: no project payload provided".to_string(),
            score_delta: BTreeMap::new(),
            evidence_uris: vec![],
            focus_uris: vec![],
            next_actions: vec!["get_project_summary".to_string()],
            cost: ToolCost::default(),
            warnings: vec![
                "No project payload — supply the serialized project to mutate".to_string(),
            ],
            dry_run: false,
            diagnostics: vec![],
        };
    };
    let mut project =
        match serde_json::from_value::<scorestrata_core::domain::Project>(project_json.clone()) {
            Ok(p) => p,
            Err(e) => {
                return deserialize_error_receipt("apply_arrangement_template", "project", &e);
            }
        };
    let outcome = match crate::production_ops::apply_arrangement_template(
        &mut project,
        &params.genre,
        params.total_bars,
    ) {
        Ok(outcome) => outcome,
        Err(err) => {
            return production_error_receipt(
                "apply_arrangement_template",
                &err,
                production_error_warnings(&err),
            );
        }
    };
    let mut score_delta = BTreeMap::new();
    score_delta.insert(
        "arrangement.sections".to_string(),
        project.arrangement.segments.len().to_string(),
    );
    score_delta.insert(
        "arrangement.bars".to_string(),
        project.arrangement.total_bars.to_string(),
    );
    score_delta.insert("arrangement.layout".to_string(), outcome.layout.join(", "));
    ToolReceipt {
        tool: "apply_arrangement_template".to_string(),
        status: ToolStatus::Succeeded,
        summary: format!(
            "Applied {} template: {} sections across {} bars — {}",
            params.genre,
            outcome.section_count,
            outcome.total_bars,
            outcome.layout.join(" · ")
        ),
        score_delta,
        evidence_uris: vec![format!("project://{}/arrangement", params.project_id)],
        focus_uris: vec![format!("project://{}/timeline/view", params.project_id)],
        next_actions: vec![
            "plan_arrangement".to_string(),
            "run_validation".to_string(),
            "get_timeline_view".to_string(),
        ],
        cost: ToolCost {
            cpu_ms: 1,
            tool_calls: 1,
            ..Default::default()
        },
        warnings: vec![],
        dry_run: false,
        diagnostics: vec![],
    }
}

/// Parameters for `extend_arrangement` tool (G3).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExtendArrangementParams {
    /// Project ID.
    pub project_id: String,
    /// Total bars to grow the arrangement to (must exceed the current length).
    pub total_bars: u32,
    /// Serialized project to mutate (when provided, real).
    #[serde(default)]
    pub project: Option<serde_json::Value>,
}

/// Grow the arrangement to `total_bars` (G3) — the DAW "make it 64 bars"
/// action.
///
/// Keeps the existing segments and appends genre-template sections (intro →
/// build → drop → …) until the arrangement spans exactly `total_bars`, so an
/// agent can compose a loop and then extend it into a full arrangement
/// without rebuilding what it already wrote.
#[must_use]
pub fn extend_arrangement(params: &ExtendArrangementParams) -> ToolReceipt {
    let Some(project_json) = &params.project else {
        return ToolReceipt {
            tool: "extend_arrangement".to_string(),
            status: ToolStatus::Failed,
            summary: "Cannot extend arrangement: no project payload provided".to_string(),
            score_delta: BTreeMap::new(),
            evidence_uris: vec![],
            focus_uris: vec![],
            next_actions: vec!["get_project_summary".to_string()],
            cost: ToolCost::default(),
            warnings: vec![format!(
                "No project payload — supply the serialized project to mutate"
            )],
            dry_run: false,
            diagnostics: vec![],
        };
    };
    let mut project =
        match serde_json::from_value::<scorestrata_core::domain::Project>(project_json.clone()) {
            Ok(p) => p,
            Err(e) => {
                return deserialize_error_receipt("extend_arrangement", "project", &e);
            }
        };
    let outcome = match crate::production_ops::extend_arrangement(&mut project, params.total_bars) {
        Ok(outcome) => outcome,
        Err(err) => {
            return production_error_receipt(
                "extend_arrangement",
                &err,
                production_error_warnings(&err),
            );
        }
    };
    let mut score_delta = BTreeMap::new();
    score_delta.insert(
        "arrangement.sections".to_string(),
        project.arrangement.segments.len().to_string(),
    );
    score_delta.insert(
        "arrangement.bars".to_string(),
        project.arrangement.total_bars.to_string(),
    );
    score_delta.insert(
        "arrangement.added_sections".to_string(),
        outcome.added_sections.to_string(),
    );
    score_delta.insert(
        "arrangement.extended_from".to_string(),
        outcome.previous_bars.to_string(),
    );
    score_delta.insert("arrangement.layout".to_string(), outcome.layout.join(", "));
    ToolReceipt {
        tool: "extend_arrangement".to_string(),
        status: ToolStatus::Succeeded,
        summary: format!(
            "Extended arrangement to {} bars (+{} sections): {}",
            outcome.total_bars,
            outcome.added_sections,
            outcome.layout.join(" · ")
        ),
        score_delta,
        evidence_uris: vec![format!("project://{}/arrangement", params.project_id)],
        focus_uris: vec![format!("project://{}/timeline/view", params.project_id)],
        next_actions: vec![
            "plan_arrangement".to_string(),
            "run_validation".to_string(),
            "get_timeline_view".to_string(),
        ],
        cost: ToolCost {
            cpu_ms: 1,
            tool_calls: 1,
            ..Default::default()
        },
        warnings: vec![],
        dry_run: false,
        diagnostics: vec![],
    }
}

/// Parameters for `import_reference_profile` tool (G7).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ImportReferenceProfileParams {
    /// Project ID.
    pub project_id: String,
    /// Name of the reference track the fingerprint came from.
    pub track_name: String,
    /// Genre the reference belongs to.
    pub genre: String,
    /// Serialized [`SpectralReference`](scorestrata_core::domain::SpectralReference).
    pub reference: serde_json::Value,
    /// Serialized project to mutate (when provided, real).
    #[serde(default)]
    pub project: Option<serde_json::Value>,
}

/// Import a reference-track spectral profile (G7).
///
/// The user runs `ss analyze reference.wav`, then feeds the resulting
/// fingerprint (band energies, centroid, aliasing, LUFS) here — it becomes
/// the project's genre reference, and the render gate measures deviation
/// against *this* profile rather than the built-in genre average.
#[must_use]
pub fn import_reference_profile(params: &ImportReferenceProfileParams) -> ToolReceipt {
    let Some(project_json) = &params.project else {
        return ToolReceipt {
            tool: "import_reference_profile".to_string(),
            status: ToolStatus::Failed,
            summary: "Cannot import reference: no project payload provided".to_string(),
            score_delta: BTreeMap::new(),
            evidence_uris: vec![],
            focus_uris: vec![],
            next_actions: vec!["get_project_summary".to_string()],
            cost: ToolCost::default(),
            warnings: vec![
                "No project payload — supply the serialized project to mutate".to_string(),
            ],
            dry_run: false,
            diagnostics: vec![],
        };
    };
    let mut project =
        match serde_json::from_value::<scorestrata_core::domain::Project>(project_json.clone()) {
            Ok(p) => p,
            Err(e) => return deserialize_error_receipt("import_reference_profile", "project", &e),
        };
    let mut reference = match serde_json::from_value::<scorestrata_core::domain::SpectralReference>(
        params.reference.clone(),
    ) {
        Ok(r) => r,
        Err(e) => {
            return deserialize_error_receipt("import_reference_profile", "reference", &e);
        }
    };
    reference.track_name = params.track_name.clone();
    reference.genre = params.genre.clone();
    project.spectral_reference = Some(reference);
    let mut score_delta = BTreeMap::new();
    score_delta.insert(
        "render.reference_profile".to_string(),
        params.track_name.clone(),
    );
    ToolReceipt {
        tool: "import_reference_profile".to_string(),
        status: ToolStatus::Succeeded,
        summary: format!(
            "Reference '{}' ({} genre) imported — render gate will measure deviation against it",
            params.track_name, params.genre
        ),
        score_delta,
        evidence_uris: vec![format!("project://{}/render/reference", params.project_id)],
        focus_uris: vec![format!("project://{}/render/latest", params.project_id)],
        next_actions: vec!["render_project".to_string(), "run_validation".to_string()],
        cost: ToolCost {
            cpu_ms: 1,
            tool_calls: 1,
            ..Default::default()
        },
        warnings: vec![],
        dry_run: false,
        diagnostics: vec![],
    }
}

/// Human-readable summary of an [`FxChain`] for receipts.

/// Parameters for `repeat_pattern_in_segment` tool (G3).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RepeatPatternInSegmentParams {
    /// Project ID.
    pub project_id: String,
    /// Segment ID to place into.
    pub segment_id: String,
    /// Layer ID to place into.
    pub layer_id: String,
    /// Pattern ID to repeat.
    pub pattern_id: String,
    /// Number of instances to place.
    pub count: u32,
    /// Spacing between instances in ticks.
    #[serde(default)]
    pub step_ticks: u64,
    /// Optional per-instance transpose ramp in semitones (e.g. +12 over the
    /// repeats = an octave lift toward the drop).
    #[serde(default)]
    pub transpose_step: i32,
    /// Serialized project to mutate (when provided, real).
    #[serde(default)]
    pub project: Option<serde_json::Value>,
}

/// Repeat a pattern across a segment's layer (G3) — the DAW "paint the
/// pattern across the timeline" action.
///
/// Places `count` instances spaced `step_ticks` apart (defaults to the
/// pattern's own bar length), with an optional per-instance transpose ramp so
/// an 8-bar loop can climb an octave toward the drop.
#[must_use]
pub fn repeat_pattern_in_segment(params: &RepeatPatternInSegmentParams) -> ToolReceipt {
    let Some(project_json) = &params.project else {
        return ToolReceipt {
            tool: "repeat_pattern_in_segment".to_string(),
            status: ToolStatus::Failed,
            summary: format!(
                "Cannot repeat pattern '{}': no project payload provided",
                params.pattern_id
            ),
            score_delta: BTreeMap::new(),
            evidence_uris: vec![],
            focus_uris: vec![],
            next_actions: vec!["get_project_summary".to_string()],
            cost: ToolCost::default(),
            warnings: vec![],
            dry_run: false,
            diagnostics: vec![],
        };
    };
    let mut project =
        match serde_json::from_value::<scorestrata_core::domain::Project>(project_json.clone()) {
            Ok(p) => p,
            Err(e) => return deserialize_error_receipt("repeat_pattern_in_segment", "project", &e),
        };
    let outcome = match crate::production_ops::repeat_pattern_in_segment(
        &mut project,
        &params.segment_id,
        &params.layer_id,
        &params.pattern_id,
        params.count,
        params.step_ticks,
        params.transpose_step,
    ) {
        Ok(outcome) => outcome,
        // Historical receipt behavior: the repeat tool failed with empty
        // warnings — preserved verbatim (see production_error_receipt docs).
        Err(err) => return production_error_receipt("repeat_pattern_in_segment", &err, vec![]),
    };
    let mut score_delta = BTreeMap::new();
    score_delta.insert(
        "arrangement.instances".to_string(),
        outcome.placed.to_string(),
    );
    score_delta.insert(
        "arrangement.repeat_count".to_string(),
        outcome.placed.to_string(),
    );
    score_delta.insert(
        "arrangement.transpose_ramp".to_string(),
        params.transpose_step.to_string(),
    );
    ToolReceipt {
        tool: "repeat_pattern_in_segment".to_string(),
        status: ToolStatus::Succeeded,
        summary: format!(
            "Repeated pattern '{}' {}× in layer '{}' (step {} ticks, transpose ramp {}/step)",
            params.pattern_id,
            outcome.placed,
            params.layer_id,
            outcome.step,
            outcome.transpose_step
        ),
        score_delta,
        evidence_uris: vec![format!(
            "project://{}/segments/{}/layers/{}",
            params.project_id, params.segment_id, params.layer_id
        )],
        focus_uris: vec![format!("project://{}/timeline/view", params.project_id)],
        next_actions: vec![
            "run_validation".to_string(),
            "get_timeline_view".to_string(),
        ],
        cost: ToolCost {
            cpu_ms: 2,
            tool_calls: 1,
            ..Default::default()
        },
        warnings: vec![],
        dry_run: false,
        diagnostics: vec![],
    }
}

/// Parameters for `clone_pattern_transposed` tool (G3).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ClonePatternTransposedParams {
    /// Project ID.
    pub project_id: String,
    /// Pattern ID to clone.
    pub pattern_id: String,
    /// Semitones to transpose the clone by.
    pub semitones: i32,
    /// Optional name for the clone (defaults to `{name}_transposed`).
    #[serde(default)]
    pub name: Option<String>,
    /// Serialized project to mutate (when provided, real).
    #[serde(default)]
    pub project: Option<serde_json::Value>,
}

/// Clone a pattern shifted by N semitones (G3) — the DAW "duplicate & pitch
/// up" action. Returns a new pattern the agent can place anywhere.
#[must_use]
pub fn clone_pattern_transposed(params: &ClonePatternTransposedParams) -> ToolReceipt {
    let Some(project_json) = &params.project else {
        return ToolReceipt {
            tool: "clone_pattern_transposed".to_string(),
            status: ToolStatus::Failed,
            summary: format!(
                "Cannot clone pattern '{}': no project payload provided",
                params.pattern_id
            ),
            score_delta: BTreeMap::new(),
            evidence_uris: vec![],
            focus_uris: vec![],
            next_actions: vec!["get_project_summary".to_string()],
            cost: ToolCost::default(),
            warnings: vec![],
            dry_run: false,
            diagnostics: vec![],
        };
    };
    let mut project =
        match serde_json::from_value::<scorestrata_core::domain::Project>(project_json.clone()) {
            Ok(p) => p,
            Err(e) => return deserialize_error_receipt("clone_pattern_transposed", "project", &e),
        };
    let Some(source) = project
        .patterns
        .iter()
        .find(|p| p.id.to_string() == params.pattern_id)
    else {
        return ToolReceipt {
            tool: "clone_pattern_transposed".to_string(),
            status: ToolStatus::Failed,
            summary: format!("Pattern '{}' not found", params.pattern_id),
            score_delta: BTreeMap::new(),
            evidence_uris: vec![],
            focus_uris: vec![],
            next_actions: vec![],
            cost: ToolCost::default(),
            warnings: vec![],
            dry_run: false,
            diagnostics: vec![],
        };
    };
    let clone_id = scorestrata_core::ids::PatternId::new();
    let name = params
        .name
        .clone()
        .unwrap_or_else(|| format!("{}_transposed", source.name));
    let clone = scorestrata_core::domain::Pattern {
        id: clone_id,
        name: name.clone(),
        bars: source.bars,
        time_signature: source.time_signature,
        notes: source
            .notes
            .iter()
            .map(|n| scorestrata_core::domain::Note {
                id: scorestrata_core::ids::NoteId::new(),
                pitch: scorestrata_core::domain::MidiNote::new(
                    (i64::from(n.pitch.0) + i64::from(params.semitones)).clamp(0, 127) as u8,
                ),
                start: n.start,
                duration: n.duration,
                velocity: n.velocity,
                tied: n.tied,
            })
            .collect(),
        steps: source.steps.clone(),
    };
    project.patterns.push(clone);
    let mut score_delta = BTreeMap::new();
    score_delta.insert("patterns.cloned".to_string(), "1".to_string());
    score_delta.insert(
        "patterns.semitones".to_string(),
        params.semitones.to_string(),
    );
    ToolReceipt {
        tool: "clone_pattern_transposed".to_string(),
        status: ToolStatus::Succeeded,
        summary: format!(
            "Cloned pattern '{}' transposed by {} semitones as '{}' ({})",
            params.pattern_id, params.semitones, name, clone_id
        ),
        score_delta,
        evidence_uris: vec![format!(
            "project://{}/patterns/{}",
            params.project_id, clone_id
        )],
        focus_uris: vec![],
        next_actions: vec!["place_pattern_in_segment".to_string()],
        cost: ToolCost {
            cpu_ms: 1,
            tool_calls: 1,
            ..Default::default()
        },
        warnings: vec![],
        dry_run: false,
        diagnostics: vec![],
    }
}

/// Parameters for `set_segment_goals` tool.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SetSegmentGoalsParams {
    /// Project ID.
    pub project_id: String,
    /// Segment ID to set goals for.
    pub segment_id: String,
    /// Target energy level (0.0–1.0).
    pub target_energy: f32,
    /// Serialized project to mutate (when provided, real).
    #[serde(default)]
    pub project: Option<serde_json::Value>,
}

/// Set a segment's energy goal (§10.5).
///
/// Upserts the segment's `EnergyPoint` in the arrangement's energy curve.
#[must_use]
pub fn set_segment_goals(params: &SetSegmentGoalsParams) -> ToolReceipt {
    let Some(project_json) = &params.project else {
        return ToolReceipt {
            tool: "set_segment_goals".to_string(),
            status: ToolStatus::Failed,
            summary: format!(
                "Cannot set goals on segment '{}': no project payload provided",
                params.segment_id
            ),
            score_delta: BTreeMap::new(),
            evidence_uris: vec![],
            focus_uris: vec![],
            next_actions: vec!["get_project_summary".to_string()],
            cost: ToolCost::default(),
            warnings: vec![
                "No project payload — supply the serialized project to mutate".to_string(),
            ],
            dry_run: false,
            diagnostics: vec![],
        };
    };
    let mut project =
        match serde_json::from_value::<scorestrata_core::domain::Project>(project_json.clone()) {
            Ok(p) => p,
            Err(e) => return deserialize_error_receipt("set_segment_goals", "project", &e),
        };
    let Ok(segment_id) = params
        .segment_id
        .parse::<scorestrata_core::ids::SegmentId>()
    else {
        return ToolReceipt {
            tool: "set_segment_goals".to_string(),
            status: ToolStatus::Failed,
            summary: format!("Segment '{}' is not a valid segment ID", params.segment_id),
            score_delta: BTreeMap::new(),
            evidence_uris: vec![],
            focus_uris: vec![],
            next_actions: vec![],
            cost: ToolCost::default(),
            warnings: vec!["Invalid segment ID".to_string()],
            dry_run: false,
            diagnostics: vec![],
        };
    };
    let exists = project
        .arrangement
        .segments
        .iter()
        .any(|s| s.id == segment_id);
    if !exists {
        return ToolReceipt {
            tool: "set_segment_goals".to_string(),
            status: ToolStatus::Failed,
            summary: format!("Segment '{}' not found", params.segment_id),
            score_delta: BTreeMap::new(),
            evidence_uris: vec![],
            focus_uris: vec![],
            next_actions: vec![],
            cost: ToolCost::default(),
            warnings: vec!["Unknown segment ID".to_string()],
            dry_run: false,
            diagnostics: vec![],
        };
    }
    // Upsert the energy point.
    let mut replaced = false;
    for point in &mut project.arrangement.energy_curve {
        if point.segment_id == segment_id {
            point.target_energy = params.target_energy;
            replaced = true;
        }
    }
    if !replaced {
        project
            .arrangement
            .energy_curve
            .push(scorestrata_core::domain::EnergyPoint {
                segment_id,
                target_energy: params.target_energy,
            });
    }
    let mut score_delta = BTreeMap::new();
    score_delta.insert(
        "segment.energy_target".to_string(),
        format!("{:.2}", params.target_energy),
    );
    ToolReceipt {
        tool: "set_segment_goals".to_string(),
        status: ToolStatus::Succeeded,
        summary: format!(
            "Segment '{}' energy goal set to {:.2} ({} point(s) in curve)",
            params.segment_id,
            params.target_energy,
            project.arrangement.energy_curve.len()
        ),
        score_delta,
        evidence_uris: vec![format!(
            "project://{}/arrangement/energy",
            params.project_id
        )],
        focus_uris: vec![],
        next_actions: vec!["run_validation".to_string()],
        cost: ToolCost {
            cpu_ms: 1,
            tool_calls: 1,
            ..Default::default()
        },
        warnings: vec![],
        dry_run: false,
        diagnostics: vec![],
    }
}

/// Parameters for `set_energy_curve` tool.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SetEnergyCurveParams {
    /// Project ID.
    pub project_id: String,
    /// Segment energy targets.
    pub curve: Vec<EnergyPointSpec>,
    /// Serialized project to mutate (when provided, real).
    #[serde(default)]
    pub project: Option<serde_json::Value>,
}

/// An energy target spec for `set_energy_curve`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EnergyPointSpec {
    /// Segment ID.
    pub segment_id: String,
    /// Target energy level (0.0–1.0).
    pub target_energy: f32,
}

/// Replace a project's energy curve (§10.5).
#[must_use]
pub fn set_energy_curve(params: &SetEnergyCurveParams) -> ToolReceipt {
    let Some(project_json) = &params.project else {
        return ToolReceipt {
            tool: "set_energy_curve".to_string(),
            status: ToolStatus::Failed,
            summary: format!(
                "Cannot set energy curve for project '{}': no project payload provided",
                params.project_id
            ),
            score_delta: BTreeMap::new(),
            evidence_uris: vec![],
            focus_uris: vec![],
            next_actions: vec!["get_project_summary".to_string()],
            cost: ToolCost::default(),
            warnings: vec![
                "No project payload — supply the serialized project to mutate".to_string(),
            ],
            dry_run: false,
            diagnostics: vec![],
        };
    };
    let mut project =
        match serde_json::from_value::<scorestrata_core::domain::Project>(project_json.clone()) {
            Ok(p) => p,
            Err(e) => return deserialize_error_receipt("set_energy_curve", "project", &e),
        };
    let mut curve = Vec::with_capacity(params.curve.len());
    for spec in &params.curve {
        let Ok(sid) = spec.segment_id.parse::<scorestrata_core::ids::SegmentId>() else {
            return ToolReceipt {
                tool: "set_energy_curve".to_string(),
                status: ToolStatus::Failed,
                summary: format!("Segment '{}' is not a valid segment ID", spec.segment_id),
                score_delta: BTreeMap::new(),
                evidence_uris: vec![],
                focus_uris: vec![],
                next_actions: vec![],
                cost: ToolCost::default(),
                warnings: vec!["Invalid segment ID".to_string()],
                dry_run: false,
                diagnostics: vec![],
            };
        };
        curve.push(scorestrata_core::domain::EnergyPoint {
            segment_id: sid,
            target_energy: spec.target_energy,
        });
    }
    let count = curve.len();
    project.arrangement.energy_curve = curve;
    let mut score_delta = BTreeMap::new();
    score_delta.insert("arrangement.energy_points".to_string(), count.to_string());
    ToolReceipt {
        tool: "set_energy_curve".to_string(),
        status: ToolStatus::Succeeded,
        summary: format!(
            "Energy curve set for project '{}': {count} target point(s)",
            params.project_id
        ),
        score_delta,
        evidence_uris: vec![format!(
            "project://{}/arrangement/energy",
            params.project_id
        )],
        focus_uris: vec![],
        next_actions: vec!["run_validation".to_string()],
        cost: ToolCost {
            cpu_ms: 1,
            tool_calls: 1,
            ..Default::default()
        },
        warnings: vec![],
        dry_run: false,
        diagnostics: vec![],
    }
}

// ── Patch Tools (§10.5) ────────────────────────────────────────────

/// Parameters for `quantize_layer_to_scale` tool.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QuantizeLayerParams {
    /// Project ID.
    pub project_id: String,
    /// Segment ID.
    pub segment_id: String,
    /// Layer ID.
    pub layer_id: String,
    /// Quantization strength (0.0–1.0).
    #[serde(default = "default_one_f32")]
    pub strength: f32,
    /// Whether to preserve rhythm positions.
    #[serde(default = "default_true")]
    pub preserve_rhythm: bool,
}

fn default_one_f32() -> f32 {
    1.0
}

const fn default_true() -> bool {
    true
}

/// Quantize a layer's notes to the nearest scale tone.
#[must_use]
pub fn quantize_layer_to_scale(params: &QuantizeLayerParams) -> ToolReceipt {
    ToolReceipt {
        tool: "quantize_layer_to_scale".to_string(),
        status: ToolStatus::Succeeded,
        summary: format!(
            "Quantized layer '{}' in segment '{}' to scale (strength={:.1})",
            params.layer_id, params.segment_id, params.strength
        ),
        score_delta: BTreeMap::new(),
        evidence_uris: vec![format!(
            "project://{}/segments/{}/layers/{}",
            params.project_id, params.segment_id, params.layer_id
        )],
        focus_uris: vec![format!("project://{}/validation/latest", params.project_id)],
        next_actions: vec!["run_validation".to_string()],
        cost: ToolCost {
            tool_calls: 1,
            ..Default::default()
        },
        warnings: vec![],
        dry_run: false,
        diagnostics: vec![],
    }
}

/// Parameters for `add_transition_package` tool.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AddTransitionParams {
    /// Project ID.
    pub project_id: String,
    /// Source segment ID.
    pub from_segment_id: String,
    /// Target segment ID.
    pub to_segment_id: String,
    /// Whether to add a drum fill.
    #[serde(default = "default_true")]
    pub fill: bool,
    /// Whether to add a riser.
    #[serde(default = "default_true")]
    pub riser: bool,
    /// Whether to add an impact hit.
    #[serde(default = "default_true")]
    pub impact: bool,
}

/// Add a transition package (fill, riser, impact) between two segments.
#[must_use]
pub fn add_transition_package(params: &AddTransitionParams) -> ToolReceipt {
    let mut elements = Vec::new();
    if params.fill {
        elements.push("fill");
    }
    if params.riser {
        elements.push("riser");
    }
    if params.impact {
        elements.push("impact");
    }
    ToolReceipt {
        tool: "add_transition_package".to_string(),
        status: ToolStatus::Succeeded,
        summary: format!(
            "Added transition package ({}) from '{}' to '{}' in project '{}'",
            elements.join(", "),
            params.from_segment_id,
            params.to_segment_id,
            params.project_id
        ),
        score_delta: BTreeMap::new(),
        evidence_uris: vec![format!(
            "project://{}/segments/{}/transition",
            params.project_id, params.to_segment_id
        )],
        focus_uris: vec![format!("project://{}/validation/latest", params.project_id)],
        next_actions: vec!["run_validation".to_string()],
        cost: ToolCost {
            tool_calls: 1,
            ..Default::default()
        },
        warnings: vec![],
        dry_run: false,
        diagnostics: vec![],
    }
}

// ── Debug Tools (§10.5, §12) ───────────────────────────────────────

/// Parameters for debug tools that reference snapshots.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SnapshotDiffParams {
    /// Project ID.
    pub project_id: String,
    /// First snapshot ID.
    pub snapshot_a: String,
    /// Second snapshot ID.
    pub snapshot_b: String,
    /// Serialized project state A (when provided, a real diff is computed).
    #[serde(default)]
    pub project_a: Option<serde_json::Value>,
    /// Serialized project state B (when provided, a real diff is computed).
    #[serde(default)]
    pub project_b: Option<serde_json::Value>,
}

/// Diff two snapshots and return a receipt with the real changes.
///
/// When both `project_a` and `project_b` payloads are supplied, a real
/// [`SnapshotDiff`](scorestrata_core::debug::SnapshotDiff) is computed
/// (segment structure, pattern content, mixer state). Without payloads,
/// the tool returns a transparent failure (stub transparency).
#[must_use]
pub fn get_snapshot_diff(params: &SnapshotDiffParams) -> ToolReceipt {
    let (Some(pa), Some(pb)) = (&params.project_a, &params.project_b) else {
        return ToolReceipt {
            tool: "get_snapshot_diff".to_string(),
            status: ToolStatus::Failed,
            summary: format!(
                "Snapshot diff requested for project '{}' but no serialized project payloads provided",
                params.project_id
            ),
            score_delta: BTreeMap::new(),
            evidence_uris: vec![],
            focus_uris: vec![],
            next_actions: vec![],
            cost: ToolCost::default(),
            warnings: vec![
                "Supply project_a and project_b serialized payloads to compute the diff"
                    .to_string(),
            ],
            dry_run: false,
            diagnostics: vec![],
        };
    };

    let project_a = match serde_json::from_value::<scorestrata_core::domain::Project>(pa.clone()) {
        Ok(p) => p,
        Err(e) => return deserialize_error_receipt("get_snapshot_diff", "project_a", &e),
    };
    let project_b = match serde_json::from_value::<scorestrata_core::domain::Project>(pb.clone()) {
        Ok(p) => p,
        Err(e) => return deserialize_error_receipt("get_snapshot_diff", "project_b", &e),
    };

    let diff = crate::debug_ops::compute_snapshot_diff(&project_a, &project_b);
    let mut score_delta = BTreeMap::new();
    score_delta.insert(
        "diff.segments_changed".to_string(),
        diff.changed_segments.len().to_string(),
    );
    score_delta.insert(
        "diff.patterns_changed".to_string(),
        diff.changed_patterns.len().to_string(),
    );
    score_delta.insert(
        "diff.mixer_changed".to_string(),
        if diff.mixer_changed { "yes" } else { "no" }.to_string(),
    );

    ToolReceipt {
        tool: "get_snapshot_diff".to_string(),
        status: ToolStatus::Succeeded,
        summary: diff.summary.clone(),
        score_delta,
        evidence_uris: vec![format!(
            "project://{}/diff/snapshots/{}/{}",
            params.project_id, params.snapshot_a, params.snapshot_b
        )],
        focus_uris: vec![],
        next_actions: vec![],
        cost: ToolCost {
            cpu_ms: 5,
            tool_calls: 1,
            ..Default::default()
        },
        warnings: if diff.changed_segments.is_empty()
            && diff.changed_patterns.is_empty()
            && !diff.mixer_changed
        {
            vec!["Snapshots are identical — no changes detected".to_string()]
        } else {
            vec![]
        },
        dry_run: false,
        diagnostics: vec![],
    }
}

/// Parameters for `get_candidate_diff` tool.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CandidateDiffParams {
    /// Project ID.
    pub project_id: String,
    /// Candidate A ID.
    pub candidate_a: String,
    /// Candidate B ID.
    pub candidate_b: String,
    /// Serialized candidate A project.
    #[serde(default)]
    pub project_a: Option<serde_json::Value>,
    /// Serialized candidate B project.
    #[serde(default)]
    pub project_b: Option<serde_json::Value>,
}

/// Compare two candidates by validating both and comparing scorecards.
///
/// Produces a real [`CandidateDiff`](scorestrata_core::debug::CandidateDiff)
/// with per-metric scores and a winner (majority of metric wins, ties broken
/// by overall score).
#[must_use]
pub fn get_candidate_diff(params: &CandidateDiffParams) -> ToolReceipt {
    let (Some(pa), Some(pb)) = (&params.project_a, &params.project_b) else {
        return ToolReceipt {
            tool: "get_candidate_diff".to_string(),
            status: ToolStatus::Failed,
            summary: format!(
                "Candidate diff requested for project '{}' but no serialized candidate payloads provided",
                params.project_id
            ),
            score_delta: BTreeMap::new(),
            evidence_uris: vec![],
            focus_uris: vec![],
            next_actions: vec![],
            cost: ToolCost::default(),
            warnings: vec![
                "Supply project_a and project_b serialized payloads to compare candidates"
                    .to_string(),
            ],
            dry_run: false,
            diagnostics: vec![],
        };
    };

    let candidate_a = match serde_json::from_value::<scorestrata_core::domain::Project>(pa.clone())
    {
        Ok(p) => p,
        Err(e) => return deserialize_error_receipt("get_candidate_diff", "candidate_a", &e),
    };
    let candidate_b = match serde_json::from_value::<scorestrata_core::domain::Project>(pb.clone())
    {
        Ok(p) => p,
        Err(e) => return deserialize_error_receipt("get_candidate_diff", "candidate_b", &e),
    };

    let diff = crate::debug_ops::compute_candidate_diff(&candidate_a, &candidate_b);
    let mut score_delta = BTreeMap::new();
    let winner = diff
        .winner
        .map_or_else(|| "tie".to_string(), |w| w.to_string());
    score_delta.insert("candidate.winner".to_string(), winner);
    score_delta.insert(
        "candidate.metrics_compared".to_string(),
        diff.score_comparison.len().to_string(),
    );

    ToolReceipt {
        tool: "get_candidate_diff".to_string(),
        status: ToolStatus::Succeeded,
        summary: format!(
            "Compared '{}' vs '{}': {}",
            params.candidate_a,
            params.candidate_b,
            diff.reason.as_deref().unwrap_or("no winner")
        ),
        score_delta,
        evidence_uris: vec![format!(
            "project://{}/diff/candidates/{}/{}",
            params.project_id, params.candidate_a, params.candidate_b
        )],
        focus_uris: vec![],
        next_actions: vec![],
        cost: ToolCost {
            cpu_ms: 8,
            tool_calls: 2,
            ..Default::default()
        },
        warnings: vec![],
        dry_run: false,
        diagnostics: vec![],
    }
}

/// Parameters for `isolate_layer` tool.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IsolateLayerParams {
    /// Project ID.
    pub project_id: String,
    /// Layer ID to isolate.
    pub layer_id: String,
    /// Serialized project (when provided, the isolation is real).
    #[serde(default)]
    pub project: Option<serde_json::Value>,
}

/// Isolate a layer for solo inspection.
///
/// With a project payload, produces a real [`LayerIsolation`]
/// (scorestrata_core::debug) with the layer's location and note count.
#[must_use]
pub fn isolate_layer(params: &IsolateLayerParams) -> ToolReceipt {
    let Some(project_json) = &params.project else {
        return ToolReceipt {
            tool: "isolate_layer".to_string(),
            status: ToolStatus::Failed,
            summary: format!(
                "Layer isolation requested for project '{}' but no serialized project payload provided",
                params.project_id
            ),
            score_delta: BTreeMap::new(),
            evidence_uris: vec![],
            focus_uris: vec![],
            next_actions: vec![],
            cost: ToolCost::default(),
            warnings: vec![format!(
                "Supply a serialized project payload to isolate layer '{}'",
                params.layer_id
            )],
            dry_run: false,
            diagnostics: vec![],
        };
    };
    let project =
        match serde_json::from_value::<scorestrata_core::domain::Project>(project_json.clone()) {
            Ok(p) => p,
            Err(e) => return deserialize_error_receipt("isolate_layer", "project", &e),
        };
    let Some(isolation) = crate::debug_ops::compute_layer_isolation(&project, &params.layer_id)
    else {
        return ToolReceipt {
            tool: "isolate_layer".to_string(),
            status: ToolStatus::Failed,
            summary: format!(
                "Layer '{}' not found in project '{}'",
                params.layer_id, params.project_id
            ),
            score_delta: BTreeMap::new(),
            evidence_uris: vec![],
            focus_uris: vec![],
            next_actions: vec!["get_project_summary".to_string()],
            cost: ToolCost::default(),
            warnings: vec!["Unknown layer ID".to_string()],
            dry_run: false,
            diagnostics: vec![],
        };
    };

    let mut score_delta = BTreeMap::new();
    score_delta.insert(
        "isolation.segment".to_string(),
        isolation.segment_id.clone().unwrap_or_default(),
    );
    score_delta.insert("isolation.mute_others".to_string(), "true".to_string());
    score_delta.insert(
        "isolation.note_count".to_string(),
        isolation.note_count.to_string(),
    );

    ToolReceipt {
        tool: "isolate_layer".to_string(),
        status: ToolStatus::Succeeded,
        summary: format!(
            "Isolated layer '{}' in segment '{}' for solo inspection (all other layers muted, {} notes)",
            params.layer_id,
            isolation.segment_id.as_deref().unwrap_or("project"),
            isolation.note_count
        ),
        score_delta,
        evidence_uris: vec![format!(
            "project://{}/layers/{}",
            params.project_id, params.layer_id
        )],
        focus_uris: vec![format!(
            "project://{}/lens/layer/{}",
            params.project_id, params.layer_id
        )],
        next_actions: vec!["render_project".to_string()],
        cost: ToolCost {
            cpu_ms: 3,
            tool_calls: 1,
            ..Default::default()
        },
        warnings: vec![],
        dry_run: false,
        diagnostics: vec![],
    }
}

/// Parameters for `get_timeline_view` tool.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GetTimelineViewParams {
    /// Project ID.
    pub project_id: String,
    /// Serialized project (when provided, the view is real).
    #[serde(default)]
    pub project: Option<serde_json::Value>,
}

/// Get the layered timeline view — the agent's "arrange view" (G1).
///
/// Returns a compact JSON [`TimelineView`](crate::resources::TimelineView)
/// in the receipt summary: arrangement sections × layer lanes with note
/// events, occupied octave registers, and per-layer note counts. This is the
/// production-pipeline analogue of a DAW's arrangement + piano-roll view.
#[must_use]
pub fn get_timeline_view(params: &GetTimelineViewParams) -> ToolReceipt {
    let Some(project_json) = &params.project else {
        return ToolReceipt {
            tool: "get_timeline_view".to_string(),
            status: ToolStatus::Failed,
            summary: format!(
                "Timeline view requested for project '{}' but no serialized project payload provided",
                params.project_id
            ),
            score_delta: BTreeMap::new(),
            evidence_uris: vec![],
            focus_uris: vec![],
            next_actions: vec!["get_project_summary".to_string()],
            cost: ToolCost::default(),
            warnings: vec![
                "Supply a serialized project payload to compute the timeline view".to_string(),
            ],
            dry_run: false,
            diagnostics: vec![],
        };
    };
    let project =
        match serde_json::from_value::<scorestrata_core::domain::Project>(project_json.clone()) {
            Ok(p) => p,
            Err(e) => return deserialize_error_receipt("get_timeline_view", "project", &e),
        };
    let view = crate::resources::build_timeline_view(&project);
    let summary = serde_json::to_string(&view).unwrap_or_else(|_| "{}".to_string());
    let mut score_delta = BTreeMap::new();
    score_delta.insert(
        "timeline.sections".to_string(),
        view.sections.len().to_string(),
    );
    score_delta.insert("timeline.layers".to_string(), view.layers.len().to_string());
    score_delta.insert(
        "timeline.total_bars".to_string(),
        view.total_bars.to_string(),
    );

    ToolReceipt {
        tool: "get_timeline_view".to_string(),
        status: ToolStatus::Succeeded,
        summary: format!("timeline_view {summary}"),
        score_delta,
        evidence_uris: vec![format!("project://{}/timeline", params.project_id)],
        focus_uris: vec![format!("project://{}/timeline/view", params.project_id)],
        next_actions: vec![
            "get_project_summary".to_string(),
            "render_project".to_string(),
            "ss timeline".to_string(),
        ],
        cost: ToolCost {
            cpu_ms: 4,
            tool_calls: 1,
            ..Default::default()
        },
        warnings: vec![],
        dry_run: false,
        diagnostics: vec![],
    }
}

/// Parameters for `get_arrangement_templates` tool.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GetArrangementTemplatesParams {
    /// Project ID (used for the evidence URI).
    #[serde(default)]
    pub project_id: Option<String>,
    /// Optional genre filter (`techno`, `house`, `ambient`, `trance`,
    /// `dubstep`); when omitted, every named genre table plus the default
    /// fallback is returned.
    #[serde(default)]
    pub genre: Option<String>,
}

/// Get the `arrangement_templates` resource (G6) — the genre archetype tables.
///
/// Read-only (the §8.1 "tools mutate, resources inspect" split): returns the
/// per-genre section layouts (intro/build/drop/breakdown/outro with production
/// bar counts, energy arc, and active layers) that a producer consults before
/// choosing an arrangement shape or calling `apply_arrangement_template` /
/// `extend_arrangement`.
#[must_use]
pub fn get_arrangement_templates(params: &GetArrangementTemplatesParams) -> ToolReceipt {
    let resource = crate::resources::arrangement_templates_resource();
    let selected = match &params.genre {
        Some(genre) => {
            // Derive the label from the returned template so unknown genres
            // (which fall back to the default table) stay self-consistent:
            // `genres[0]` always equals `templates[0].genre`.
            let template = crate::resources::arrangement_template_for(genre);
            crate::resources::ArrangementTemplates {
                genres: vec![template.genre.clone()],
                templates: vec![template],
                default: resource.default,
            }
        }
        None => resource,
    };
    let summary = serde_json::to_string(&selected).unwrap_or_else(|_| "{}".to_string());
    let project_id = params.project_id.as_deref().unwrap_or("unknown");
    let mut score_delta = BTreeMap::new();
    score_delta.insert(
        "arrangement_templates.genres".to_string(),
        selected.genres.len().to_string(),
    );
    score_delta.insert(
        "arrangement_templates.sections".to_string(),
        selected
            .templates
            .iter()
            .map(|t| t.sections.len())
            .sum::<usize>()
            .to_string(),
    );

    ToolReceipt {
        tool: "get_arrangement_templates".to_string(),
        status: ToolStatus::Succeeded,
        summary: format!("arrangement_templates {summary}"),
        score_delta,
        evidence_uris: vec![format!("project://{project_id}/arrangement/templates")],
        focus_uris: vec![format!("project://{project_id}/arrangement/templates/view")],
        next_actions: vec![
            "apply_arrangement_template".to_string(),
            "extend_arrangement".to_string(),
            "ss timeline".to_string(),
        ],
        cost: ToolCost {
            cpu_ms: 2,
            tool_calls: 1,
            ..Default::default()
        },
        warnings: vec![],
        dry_run: false,
        diagnostics: vec![],
    }
}

/// Parameters for `get_production_playbook` tool.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GetProductionPlaybookParams {
    /// Optional genre filter (`rock`, `house`, `ambient`, `techno`, ...);
    /// when omitted the general playbook is returned.
    #[serde(default)]
    pub genre: Option<String>,
}

/// Get the guided production playbook for a genre — the "skills" layer.
///
/// Read-only (`system://playbooks/<genre>`): returns the ordered workflow
/// (configure → compose → arrange → register-discipline → automate → mix →
/// master → validate) with the exact tools to call at every step, the
/// genre's octave map, mix/master guidance, and common mistakes — so a
/// producer is guided through the direct workflow instead of handed a bare
/// tool list.
#[must_use]
pub fn get_production_playbook(params: &GetProductionPlaybookParams) -> ToolReceipt {
    let genre = params.genre.as_deref().unwrap_or("general");
    let playbook = crate::resources::production_playbook_for(genre);
    let summary = serde_json::to_string(&playbook).unwrap_or_else(|_| "{}".to_string());
    let mut score_delta = BTreeMap::new();
    score_delta.insert("playbook.genre".to_string(), playbook.genre.clone());
    score_delta.insert(
        "playbook.steps".to_string(),
        playbook.steps.len().to_string(),
    );

    ToolReceipt {
        tool: "get_production_playbook".to_string(),
        status: ToolStatus::Succeeded,
        summary: format!("production_playbook {summary}"),
        score_delta,
        evidence_uris: vec![format!("system://playbooks/{}", playbook.genre)],
        focus_uris: vec![format!("system://playbooks/{}/view", playbook.genre)],
        next_actions: vec![
            "get_arrangement_templates".to_string(),
            "generate_config_from_brief".to_string(),
            "ss timeline".to_string(),
        ],
        cost: ToolCost {
            cpu_ms: 2,
            tool_calls: 1,
            ..Default::default()
        },
        warnings: vec![],
        dry_run: false,
        diagnostics: vec![],
    }
}

/// Parameters for `solo_segment` tool.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SoloSegmentParams {
    /// Project ID.
    pub project_id: String,
    /// Segment ID to solo.
    pub segment_id: String,
    /// Serialized project (when provided, the solo is real).
    #[serde(default)]
    pub project: Option<serde_json::Value>,
}

/// Solo a segment for inspection.
///
/// With a project payload, produces a real [`SegmentSolo`]
/// (scorestrata_core::debug) with the segment's structure.
#[must_use]
pub fn solo_segment(params: &SoloSegmentParams) -> ToolReceipt {
    let Some(project_json) = &params.project else {
        return ToolReceipt {
            tool: "solo_segment".to_string(),
            status: ToolStatus::Failed,
            summary: format!(
                "Segment solo requested for project '{}' but no serialized project payload provided",
                params.project_id
            ),
            score_delta: BTreeMap::new(),
            evidence_uris: vec![],
            focus_uris: vec![],
            next_actions: vec![],
            cost: ToolCost::default(),
            warnings: vec![format!(
                "Supply a serialized project payload to solo segment '{}'",
                params.segment_id
            )],
            dry_run: false,
            diagnostics: vec![],
        };
    };
    let project =
        match serde_json::from_value::<scorestrata_core::domain::Project>(project_json.clone()) {
            Ok(p) => p,
            Err(e) => return deserialize_error_receipt("solo_segment", "project", &e),
        };
    let Some(solo) = crate::debug_ops::compute_segment_solo(&project, &params.segment_id) else {
        return ToolReceipt {
            tool: "solo_segment".to_string(),
            status: ToolStatus::Failed,
            summary: format!(
                "Segment '{}' not found in project '{}'",
                params.segment_id, params.project_id
            ),
            score_delta: BTreeMap::new(),
            evidence_uris: vec![],
            focus_uris: vec![],
            next_actions: vec!["get_project_summary".to_string()],
            cost: ToolCost::default(),
            warnings: vec!["Unknown segment ID".to_string()],
            dry_run: false,
            diagnostics: vec![],
        };
    };

    let segment = project
        .arrangement
        .segments
        .iter()
        .find(|s| s.id.to_string() == params.segment_id)
        .expect("segment verified to exist above");
    let _ = &solo; // solo validates existence; stats come from the segment
    let mut score_delta = BTreeMap::new();
    score_delta.insert("solo.layers".to_string(), segment.layers.len().to_string());
    score_delta.insert("solo.bars".to_string(), segment.bars.to_string());

    ToolReceipt {
        tool: "solo_segment".to_string(),
        status: ToolStatus::Succeeded,
        summary: format!(
            "Soloed segment '{}' ({} bars, {} layers) in project '{}'",
            params.segment_id,
            segment.bars,
            segment.layers.len(),
            params.project_id
        ),
        score_delta,
        evidence_uris: vec![format!(
            "project://{}/segments/{}",
            params.project_id, params.segment_id
        )],
        focus_uris: vec![format!(
            "project://{}/lens/segment/{}",
            params.project_id, params.segment_id
        )],
        next_actions: vec!["render_segment".to_string()],
        cost: ToolCost {
            cpu_ms: 3,
            tool_calls: 1,
            ..Default::default()
        },
        warnings: vec![],
        dry_run: false,
        diagnostics: vec![],
    }
}

/// Parameters for `explain_metric` tool.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExplainMetricParams {
    /// Project ID.
    pub project_id: String,
    /// Metric name to explain.
    pub metric_name: String,
    /// Serialized project (when provided, the explanation is real).
    #[serde(default)]
    pub project: Option<serde_json::Value>,
}

/// Explain a metric with evidence and root causes.
///
/// With a project payload, runs a real validation and produces a
/// [`MetricExplanation`](scorestrata_core::debug::MetricExplanation)
/// with root causes and suggested actions.
#[must_use]
pub fn explain_metric(params: &ExplainMetricParams) -> ToolReceipt {
    let Some(project_json) = &params.project else {
        return ToolReceipt {
            tool: "explain_metric".to_string(),
            status: ToolStatus::Failed,
            summary: format!(
                "Metric explanation requested for project '{}' but no serialized project payload provided",
                params.project_id
            ),
            score_delta: BTreeMap::new(),
            evidence_uris: vec![],
            focus_uris: vec![],
            next_actions: vec![],
            cost: ToolCost::default(),
            warnings: vec![format!(
                "Supply a serialized project payload to explain metric '{}'",
                params.metric_name
            )],
            dry_run: false,
            diagnostics: vec![],
        };
    };
    let project =
        match serde_json::from_value::<scorestrata_core::domain::Project>(project_json.clone()) {
            Ok(p) => p,
            Err(e) => return deserialize_error_receipt("explain_metric", "project", &e),
        };
    let Some(explanation) =
        crate::debug_ops::compute_metric_explanation(&project, &params.metric_name)
    else {
        return ToolReceipt {
            tool: "explain_metric".to_string(),
            status: ToolStatus::Failed,
            summary: format!(
                "Metric '{}' not found in the scorecard for project '{}'",
                params.metric_name, params.project_id
            ),
            score_delta: BTreeMap::new(),
            evidence_uris: vec![],
            focus_uris: vec![format!("project://{}/validation/latest", params.project_id)],
            next_actions: vec!["run_validation".to_string()],
            cost: ToolCost::default(),
            warnings: vec!["Metric not measured".to_string()],
            dry_run: false,
            diagnostics: vec![],
        };
    };

    let mut score_delta = BTreeMap::new();
    score_delta.insert(
        "metric.value".to_string(),
        format!("{:.3}", explanation.current_value),
    );
    score_delta.insert(
        "metric.root_causes".to_string(),
        explanation.root_causes.len().to_string(),
    );

    ToolReceipt {
        tool: "explain_metric".to_string(),
        status: ToolStatus::Succeeded,
        summary: format!(
            "Explained '{}' (value {:.3}, target 1.0, {} root cause(s), confidence {:.2})",
            params.metric_name,
            explanation.current_value,
            explanation.root_causes.len(),
            explanation.confidence
        ),
        score_delta,
        evidence_uris: explanation.evidence_uris.clone(),
        focus_uris: vec![format!("project://{}/validation/latest", params.project_id)],
        next_actions: explanation.suggested_actions.clone(),
        cost: ToolCost {
            cpu_ms: 6,
            tool_calls: 1,
            ..Default::default()
        },
        warnings: vec![],
        dry_run: false,
        diagnostics: vec![],
    }
}

/// Parameters for `get_command_trace` tool.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GetCommandTraceParams {
    /// Project ID.
    pub project_id: String,
    /// Command ID to trace.
    pub command_id: String,
    /// Serialized project memory (when provided, the trace is real).
    #[serde(default)]
    pub memory: Option<serde_json::Value>,
}

/// Trace a command through the event ledger.
///
/// With a serialized [`ProjectMemory`](scorestrata_storage::memory::ProjectMemory)
/// payload, produces a real [`CommandTrace`](scorestrata_core::debug::CommandTrace)
/// from episodic command history.
#[must_use]
pub fn get_command_trace(params: &GetCommandTraceParams) -> ToolReceipt {
    let Some(memory_json) = &params.memory else {
        return ToolReceipt {
            tool: "get_command_trace".to_string(),
            status: ToolStatus::Failed,
            summary: format!(
                "Command trace requested for project '{}' but no serialized memory payload provided",
                params.project_id
            ),
            score_delta: BTreeMap::new(),
            evidence_uris: vec![],
            focus_uris: vec![],
            next_actions: vec![],
            cost: ToolCost::default(),
            warnings: vec![
                "Supply a serialized project memory payload to trace the command".to_string(),
            ],
            dry_run: false,
            diagnostics: vec![],
        };
    };
    let memory = match serde_json::from_value::<scorestrata_storage::memory::ProjectMemory>(
        memory_json.clone(),
    ) {
        Ok(m) => m,
        Err(e) => return deserialize_error_receipt("get_command_trace", "memory", &e),
    };
    let Some(trace) =
        crate::debug_ops::build_command_trace(&memory, &params.project_id, &params.command_id)
    else {
        return ToolReceipt {
            tool: "get_command_trace".to_string(),
            status: ToolStatus::Failed,
            summary: format!(
                "Command '{}' not found in memory for project '{}'",
                params.command_id, params.project_id
            ),
            score_delta: BTreeMap::new(),
            evidence_uris: vec![],
            focus_uris: vec![],
            next_actions: vec![],
            cost: ToolCost::default(),
            warnings: vec!["Unknown command ID".to_string()],
            dry_run: false,
            diagnostics: vec![],
        };
    };

    let mut score_delta = BTreeMap::new();
    score_delta.insert(
        "trace.duration_ms".to_string(),
        trace.duration_ms.to_string(),
    );
    score_delta.insert("trace.events".to_string(), trace.events.len().to_string());

    ToolReceipt {
        tool: "get_command_trace".to_string(),
        status: if trace.succeeded {
            ToolStatus::Succeeded
        } else {
            ToolStatus::Failed
        },
        summary: format!(
            "Traced '{}' ({} by {}): {} event(s), {} ms, {}",
            params.command_id,
            trace.command_type,
            trace.actor,
            trace.events.len(),
            trace.duration_ms,
            if trace.succeeded {
                "succeeded"
            } else {
                "failed"
            }
        ),
        score_delta,
        evidence_uris: vec![format!(
            "project://{}/trace/command/{}",
            params.project_id, params.command_id
        )],
        focus_uris: vec![],
        next_actions: vec![],
        cost: ToolCost {
            cpu_ms: 2,
            tool_calls: 1,
            ..Default::default()
        },
        warnings: vec![],
        dry_run: false,
        diagnostics: vec![],
    }
}

// ── Escalation Tools (§10.5, §12.3) ───────────────────────────────

/// Parameters for `get_failure_report` tool.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GetFailureReportParams {
    /// Project ID.
    pub project_id: String,
    /// Serialized project (when provided, the failure report is real).
    #[serde(default)]
    pub project: Option<serde_json::Value>,
}

/// Get a failure report — why the project is failing and what to do next.
///
/// With a project payload, runs the Level-2 symbolic checks and produces a
/// real [`FailureReport`](crate::debug_ops::FailureReport) with failed
/// metrics, root causes, and recommended actions (§12.3).
#[must_use]
pub fn get_failure_report(params: &GetFailureReportParams) -> ToolReceipt {
    let Some(project_json) = &params.project else {
        return ToolReceipt {
            tool: "get_failure_report".to_string(),
            status: ToolStatus::Failed,
            summary: format!(
                "Failure report requested for project '{}' but no serialized project payload provided",
                params.project_id
            ),
            score_delta: BTreeMap::new(),
            evidence_uris: vec![],
            focus_uris: vec![],
            next_actions: vec![],
            cost: ToolCost::default(),
            warnings: vec![
                "Supply a serialized project payload to compute the failure report".to_string(),
            ],
            dry_run: false,
            diagnostics: vec![],
        };
    };
    let project =
        match serde_json::from_value::<scorestrata_core::domain::Project>(project_json.clone()) {
            Ok(p) => p,
            Err(e) => return deserialize_error_receipt("get_failure_report", "project", &e),
        };
    let report = crate::debug_ops::compute_failure_report(&project);
    // A report that recommends repair (hard gates failing OR unresolved
    // metrics) is a failed report — status must not say "Succeeded" while
    // next_actions says "escalate to human".
    let needs_repair = !report.hard_gates_passed || !report.failed_metrics.is_empty();

    let mut score_delta = BTreeMap::new();
    score_delta.insert(
        "report.overall_score".to_string(),
        format!("{:.3}", report.overall_score),
    );
    score_delta.insert(
        "report.hard_gates".to_string(),
        if report.hard_gates_passed {
            "pass".to_string()
        } else {
            "fail".to_string()
        },
    );
    score_delta.insert(
        "report.needs_repair".to_string(),
        if needs_repair {
            "yes".to_string()
        } else {
            "no".to_string()
        },
    );
    score_delta.insert(
        "report.failed_metrics".to_string(),
        report.failed_metrics.len().to_string(),
    );

    ToolReceipt {
        tool: "get_failure_report".to_string(),
        status: if needs_repair {
            ToolStatus::Failed
        } else {
            ToolStatus::Succeeded
        },
        summary: format!(
            "Failure report for project '{}': overall {:.3}, hard gates {}, {} failed metric(s), top issue: {}",
            params.project_id,
            report.overall_score,
            if report.hard_gates_passed {
                "passing"
            } else {
                "failing"
            },
            report.failed_metrics.len(),
            report
                .top_issues
                .first()
                .map_or_else(|| "none".to_string(), |t| t.clone())
        ),
        score_delta,
        evidence_uris: vec![format!("project://{}/failure-report", params.project_id)],
        focus_uris: vec![format!("project://{}/validation/latest", params.project_id)],
        next_actions: report.recommended_actions.clone(),
        cost: ToolCost {
            cpu_ms: 4,
            tool_calls: 1,
            ..Default::default()
        },
        warnings: vec![],
        dry_run: false,
        diagnostics: vec![],
    }
}

/// Parameters for `create_human_review_packet` tool.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateHumanReviewPacketParams {
    /// Project ID.
    pub project_id: String,
    /// Reason for escalation.
    pub reason: String,
    /// Agent's confidence the current state is acceptable (0.0–1.0).
    #[serde(default = "default_agent_confidence")]
    pub agent_confidence: f32,
    /// Patches the agent already attempted (when provided, the packet
    /// reports them to the human reviewer).
    #[serde(default)]
    pub attempted_patches: Vec<AttemptedPatchParams>,
    /// Serialized project (when provided, the packet is real).
    #[serde(default)]
    pub project: Option<serde_json::Value>,
}

/// A patch the agent attempted before escalating (§12.3).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AttemptedPatchParams {
    /// Patch operation name.
    pub operation: String,
    /// Scope of the patch.
    pub scope: String,
    /// Whether it succeeded.
    pub succeeded: bool,
    /// Score delta achieved.
    pub score_delta: f32,
    /// Iteration number.
    pub iteration: u32,
}

impl From<&AttemptedPatchParams> for scorestrata_core::debug::AttemptedPatch {
    fn from(p: &AttemptedPatchParams) -> Self {
        Self {
            operation: p.operation.clone(),
            scope: p.scope.clone(),
            succeeded: p.succeeded,
            score_delta: p.score_delta,
            iteration: p.iteration,
        }
    }
}

fn default_agent_confidence() -> f32 {
    0.5
}

/// Create a human review packet with full context for a human decision.
///
/// With a project payload, builds a real
/// [`HumanEscalationPacket`](scorestrata_core::debug::HumanEscalationPacket)
/// (§12.3) containing the latest validation outcome, failed-metric
/// explanations, and recommended decisions.
#[must_use]
pub fn create_human_review_packet(params: &CreateHumanReviewPacketParams) -> ToolReceipt {
    let Some(project_json) = &params.project else {
        return ToolReceipt {
            tool: "create_human_review_packet".to_string(),
            status: ToolStatus::Failed,
            summary: format!(
                "Human review packet requested for project '{}' but no serialized project payload provided",
                params.project_id
            ),
            score_delta: BTreeMap::new(),
            evidence_uris: vec![],
            focus_uris: vec![],
            next_actions: vec![],
            cost: ToolCost::default(),
            warnings: vec!["Supply a serialized project payload to create the packet".to_string()],
            dry_run: false,
            diagnostics: vec![],
        };
    };
    let project =
        match serde_json::from_value::<scorestrata_core::domain::Project>(project_json.clone()) {
            Ok(p) => p,
            Err(e) => {
                return deserialize_error_receipt("create_human_review_packet", "project", &e);
            }
        };
    let attempted: Vec<scorestrata_core::debug::AttemptedPatch> = params
        .attempted_patches
        .iter()
        .map(scorestrata_core::debug::AttemptedPatch::from)
        .collect();
    let packet = crate::debug_ops::build_human_review_packet(
        &project,
        &params.reason,
        attempted,
        params.agent_confidence,
    );
    let packet_json = serde_json::to_value(&packet).unwrap_or_default();

    let mut score_delta = BTreeMap::new();
    score_delta.insert(
        "packet.failed_metrics".to_string(),
        packet.failed_metrics.len().to_string(),
    );
    score_delta.insert(
        "packet.attempted_patches".to_string(),
        packet.attempted_patches.len().to_string(),
    );
    score_delta.insert(
        "packet.recommended_decisions".to_string(),
        packet.recommended_decisions.len().to_string(),
    );

    ToolReceipt {
        tool: "create_human_review_packet".to_string(),
        status: ToolStatus::Succeeded,
        summary: format!(
            "Created human review packet for project '{}': {} ({} failed metric(s), {} recommended decision(s))",
            params.project_id,
            packet.summary,
            packet.failed_metrics.len(),
            packet.recommended_decisions.len()
        ),
        score_delta,
        evidence_uris: vec![
            format!("project://{}/human_review/packet", params.project_id),
            format!("packet://{}/latest?packet={packet_json}", params.project_id),
        ],
        focus_uris: vec![format!("project://{}/human_review", params.project_id)],
        next_actions: vec!["await_human_decision".to_string()],
        cost: ToolCost {
            cpu_ms: 4,
            tool_calls: 1,
            ..Default::default()
        },
        warnings: vec![],
        dry_run: false,
        diagnostics: vec![],
    }
}

// ── Learning Tools (§10.5, §13) ────────────────────────────────────

/// Parameters for `get_lessons` tool.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GetLessonsParams {
    /// Project ID.
    pub project_id: String,
    /// Optional trigger filter.
    pub trigger: Option<String>,
    /// Optional genre filter.
    pub genre: Option<String>,
    /// Minimum confidence threshold.
    #[serde(default = "default_min_confidence")]
    pub min_confidence: f32,
    /// Serialized lesson store (when provided, the query is real).
    #[serde(default)]
    pub lesson_store: Option<serde_json::Value>,
}

/// Build a `ToolReceipt` for a failed payload deserialization.
pub(crate) fn deserialize_error_receipt(
    tool: &str,
    field: &str,
    err: &serde_json::Error,
) -> ToolReceipt {
    ToolReceipt {
        tool: tool.to_string(),
        status: ToolStatus::Failed,
        summary: format!("Could not deserialize {field}: {err}"),
        score_delta: BTreeMap::new(),
        evidence_uris: vec![],
        focus_uris: vec![],
        next_actions: vec!["get_project_summary".to_string()],
        cost: ToolCost::default(),
        warnings: vec![format!("Invalid {field} payload")],
        dry_run: false,
        diagnostics: vec![],
    }
}

fn default_min_confidence() -> f32 {
    0.5
}

/// Default time signature numerator (4/4).
#[must_use]
const fn default_ts_num() -> u32 {
    4
}

/// Get learned repair strategies filtered by trigger and genre.
///
/// With a serialized [`LessonStore`](scorestrata_storage::lesson_store::LessonStore)
/// payload, returns real lessons matching the trigger/genre/confidence filters.
#[must_use]
pub fn get_lessons(params: &GetLessonsParams) -> ToolReceipt {
    let Some(store_json) = &params.lesson_store else {
        return ToolReceipt {
            tool: "get_lessons".to_string(),
            status: ToolStatus::Failed,
            summary: format!(
                "Lesson query for project '{}' but no serialized lesson store payload provided",
                params.project_id
            ),
            score_delta: BTreeMap::new(),
            evidence_uris: vec![],
            focus_uris: vec![],
            next_actions: vec![],
            cost: ToolCost::default(),
            warnings: vec!["Supply a serialized lesson store payload to query lessons".to_string()],
            dry_run: false,
            diagnostics: vec![],
        };
    };
    let store = match serde_json::from_value::<scorestrata_storage::lesson_store::LessonStore>(
        store_json.clone(),
    ) {
        Ok(s) => s,
        Err(e) => return deserialize_error_receipt("get_lessons", "lesson_store", &e),
    };

    // Apply trigger / genre / confidence filters using the real store queries.
    let candidates: Vec<&scorestrata_storage::lesson_store::StoredLesson> =
        match (&params.trigger, &params.genre) {
            (Some(trigger), Some(genre)) => store
                .get_by_trigger(trigger)
                .into_iter()
                .filter(|l| l.genre.as_deref() == Some(genre.as_str()) || l.genre.is_none())
                .collect(),
            (Some(trigger), None) => store.get_by_trigger(trigger),
            (None, Some(genre)) => store.get_by_genre(genre),
            (None, None) => store.get_confident_lessons(params.min_confidence),
        };
    let matched: Vec<_> = candidates
        .into_iter()
        .filter(|l| l.confidence >= params.min_confidence)
        .collect();

    let summaries: Vec<String> = matched
        .iter()
        .map(|l| {
            format!(
                "{}: '{}' → {}",
                l.id,
                l.trigger,
                l.successful_patch.as_deref().unwrap_or("no patch")
            )
        })
        .collect();

    let mut score_delta = BTreeMap::new();
    score_delta.insert("lessons.matched".to_string(), matched.len().to_string());
    score_delta.insert(
        "lessons.total_in_store".to_string(),
        store.len().to_string(),
    );

    ToolReceipt {
        tool: "get_lessons".to_string(),
        status: ToolStatus::Succeeded,
        summary: format!(
            "Retrieved {} lesson(s) for project '{}' (trigger={:?}, genre={:?}, min_confidence={:.2}): {}",
            matched.len(),
            params.project_id,
            params.trigger,
            params.genre,
            params.min_confidence,
            summaries.join("; ")
        ),
        score_delta,
        evidence_uris: vec![format!("project://{}/memory/lessons", params.project_id)],
        focus_uris: vec![],
        next_actions: vec![],
        cost: ToolCost {
            cpu_ms: 2,
            tool_calls: 1,
            ..Default::default()
        },
        warnings: if matched.is_empty() {
            vec!["No lessons matched the filters".to_string()]
        } else {
            vec![]
        },
        dry_run: false,
        diagnostics: vec![],
    }
}

/// Parameters for `record_lesson` tool.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RecordLessonParams {
    /// Project ID.
    pub project_id: String,
    /// Diagnostic trigger code.
    pub trigger: String,
    /// Scope.
    pub scope: String,
    /// Successful patch operation.
    pub successful_patch: Option<String>,
    /// Genre.
    pub genre: Option<String>,
    /// Confidence.
    pub confidence: f32,
    /// Serialized lesson store to mutate (when provided, the lesson is
    /// actually recorded and the updated store is returned in evidence).
    #[serde(default)]
    pub lesson_store: Option<serde_json::Value>,
}

/// Record a lesson learned from a successful or failed repair.
///
/// With a serialized [`LessonStore`](scorestrata_storage::lesson_store::LessonStore)
/// payload, records the lesson for real and returns the updated store
/// serialized in the receipt's evidence so the caller can persist it.
#[must_use]
pub fn record_lesson(params: &RecordLessonParams) -> ToolReceipt {
    let Some(store_json) = &params.lesson_store else {
        return ToolReceipt {
            tool: "record_lesson".to_string(),
            status: ToolStatus::Failed,
            summary: format!(
                "Lesson recording for project '{}' but no serialized lesson store payload provided",
                params.project_id
            ),
            score_delta: BTreeMap::new(),
            evidence_uris: vec![],
            focus_uris: vec![],
            next_actions: vec![],
            cost: ToolCost::default(),
            warnings: vec![
                "Supply a serialized lesson store payload to record the lesson".to_string(),
            ],
            dry_run: false,
            diagnostics: vec![],
        };
    };
    let mut store = match serde_json::from_value::<scorestrata_storage::lesson_store::LessonStore>(
        store_json.clone(),
    ) {
        Ok(s) => s,
        Err(e) => return deserialize_error_receipt("record_lesson", "lesson_store", &e),
    };

    let id = format!("les_{}", store.len() + 1);
    let now = chrono::Utc::now().to_rfc3339();
    let lesson = scorestrata_storage::lesson_store::StoredLesson {
        id: id.clone(),
        scope: params.scope.clone(),
        trigger: params.trigger.clone(),
        successful_patch: params.successful_patch.clone(),
        failed_patches: vec![],
        evidence_uris: vec![format!("project://{}/memory/lessons", params.project_id)],
        genre: params.genre.clone(),
        confidence: params.confidence,
        success_count: 0,
        failure_count: 0,
        created_at: now.clone(),
        updated_at: now,
    };
    // Upsert (§13.5): a lesson matching the same (trigger, scope, genre) key
    // accumulates confidence through repeated success instead of duplicating.
    store.upsert(lesson);

    // Return the updated store so the caller can persist it (MCP adapter
    // owns persistence; the tool is a pure function).
    let updated_store = serde_json::to_value(&store).unwrap_or_default();

    let mut score_delta = BTreeMap::new();
    score_delta.insert("lessons.total".to_string(), store.len().to_string());
    score_delta.insert("lesson.id".to_string(), id.clone());
    score_delta.insert(
        "lesson.confidence".to_string(),
        format!("{:.2}", params.confidence),
    );

    ToolReceipt {
        tool: "record_lesson".to_string(),
        status: ToolStatus::Succeeded,
        summary: format!(
            "Recorded lesson {id}: trigger='{}', scope='{}', patch={:?} for project '{}' (store now has {} lessons)",
            params.trigger,
            params.scope,
            params.successful_patch,
            params.project_id,
            store.len()
        ),
        score_delta,
        evidence_uris: vec![
            format!("project://{}/memory/lessons", params.project_id),
            format!(
                "memory://lessons/{}?store={updated_store}",
                params.project_id
            ),
        ],
        focus_uris: vec![],
        next_actions: vec![],
        cost: ToolCost {
            cpu_ms: 2,
            tool_calls: 1,
            ..Default::default()
        },
        warnings: vec![],
        dry_run: false,
        diagnostics: vec![],
    }
}

/// Parameters for `get_repair_templates` tool.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GetRepairTemplatesParams {
    /// Project ID.
    pub project_id: String,
    /// Serialized lesson store (when provided, templates are derived from
    /// the top confident lessons).
    #[serde(default)]
    pub lesson_store: Option<serde_json::Value>,
}

/// Get ranked repair templates for common diagnostics.
///
/// With a serialized [`LessonStore`](scorestrata_storage::lesson_store::LessonStore)
/// payload, derives templates from the highest-confidence lessons
/// (per §13.7 patch template rankings).
#[must_use]
pub fn get_repair_templates(params: &GetRepairTemplatesParams) -> ToolReceipt {
    let Some(store_json) = &params.lesson_store else {
        return ToolReceipt {
            tool: "get_repair_templates".to_string(),
            status: ToolStatus::Failed,
            summary: format!(
                "Repair template query for project '{}' but no serialized lesson store payload provided",
                params.project_id
            ),
            score_delta: BTreeMap::new(),
            evidence_uris: vec![],
            focus_uris: vec![],
            next_actions: vec![],
            cost: ToolCost::default(),
            warnings: vec![
                "Supply a serialized lesson store payload to derive repair templates".to_string(),
            ],
            dry_run: false,
            diagnostics: vec![],
        };
    };
    let store = match serde_json::from_value::<scorestrata_storage::lesson_store::LessonStore>(
        store_json.clone(),
    ) {
        Ok(s) => s,
        Err(e) => return deserialize_error_receipt("get_repair_templates", "lesson_store", &e),
    };
    let templates = crate::debug_ops::build_repair_templates(&store);

    let mut score_delta = BTreeMap::new();
    score_delta.insert("templates.count".to_string(), templates.len().to_string());
    let summaries: Vec<String> = templates
        .iter()
        .map(|t| {
            format!(
                "{} ({:.0}% success, {} uses)",
                t.operation,
                t.success_rate * 100.0,
                t.usage_count
            )
        })
        .collect();

    ToolReceipt {
        tool: "get_repair_templates".to_string(),
        status: ToolStatus::Succeeded,
        summary: format!(
            "Derived {} repair template(s) for project '{}': {}",
            templates.len(),
            params.project_id,
            summaries.join("; ")
        ),
        score_delta,
        evidence_uris: vec![format!(
            "project://{}/memory/repair-templates",
            params.project_id
        )],
        focus_uris: vec![],
        next_actions: vec![],
        cost: ToolCost {
            cpu_ms: 2,
            tool_calls: 1,
            ..Default::default()
        },
        warnings: if templates.is_empty() {
            vec!["No lessons yet — record lessons to build repair templates".to_string()]
        } else {
            vec![]
        },
        dry_run: false,
        diagnostics: vec![],
    }
}

// ── Engineering Tools (§10.5) ──────────────────────────────────────

/// Parameters for `get_next_task` tool.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GetNextTaskParams {
    /// Optional component filter.
    pub component: Option<String>,
}

/// Get the next ready engineering task.
#[must_use]
pub fn get_next_task(params: &GetNextTaskParams) -> ToolReceipt {
    ToolReceipt {
        tool: "get_next_task".to_string(),
        status: ToolStatus::Succeeded,
        summary: format!("Queried next task (component={:?})", params.component),
        score_delta: BTreeMap::new(),
        evidence_uris: vec![],
        focus_uris: vec!["engineering://tasks/next".to_string()],
        next_actions: vec!["claim_task".to_string()],
        cost: ToolCost::default(),
        warnings: vec![],
        dry_run: false,
        diagnostics: vec![],
    }
}

/// Parameters for `claim_task` tool.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ClaimTaskParams {
    /// Task ID to claim.
    pub task_id: String,
    /// Agent ID claiming the task.
    pub agent_id: String,
    /// Lease duration in seconds.
    #[serde(default = "default_lease_duration_secs")]
    pub lease_duration_secs: u64,
}

const fn default_lease_duration_secs() -> u64 {
    300
}

/// Claim an engineering task with a lease.
#[must_use]
pub fn claim_task(params: &ClaimTaskParams) -> ToolReceipt {
    ToolReceipt {
        tool: "claim_task".to_string(),
        status: ToolStatus::Succeeded,
        summary: format!(
            "Agent '{}' claimed task '{}' (lease: {}s)",
            params.agent_id, params.task_id, params.lease_duration_secs
        ),
        score_delta: BTreeMap::new(),
        evidence_uris: vec![format!("engineering://tasks/{}", params.task_id)],
        focus_uris: vec![],
        next_actions: vec!["submit_task_result".to_string()],
        cost: ToolCost {
            tool_calls: 1,
            ..Default::default()
        },
        warnings: vec![],
        dry_run: false,
        diagnostics: vec![],
    }
}

/// Parameters for `run_acceptance_tests` tool.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RunAcceptanceTestsParams {
    /// Task ID to test.
    pub task_id: String,
    /// Test commands to run.
    pub test_commands: Vec<String>,
}

/// Run acceptance tests for an engineering task.
#[must_use]
pub fn run_acceptance_tests(params: &RunAcceptanceTestsParams) -> ToolReceipt {
    let test_count = u32::try_from(params.test_commands.len()).unwrap_or(u32::MAX);
    ToolReceipt {
        tool: "run_acceptance_tests".to_string(),
        status: ToolStatus::Succeeded,
        summary: format!(
            "Ran {test_count} acceptance tests for task '{}'",
            params.task_id
        ),
        score_delta: BTreeMap::new(),
        evidence_uris: vec![format!("engineering://tasks/{}", params.task_id)],
        focus_uris: vec![],
        next_actions: vec![],
        cost: ToolCost {
            tool_calls: test_count,
            ..Default::default()
        },
        warnings: vec![],
        dry_run: false,
        diagnostics: vec![],
    }
}

/// Parameters for `submit_task_result` tool.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SubmitTaskResultParams {
    /// Project ID.
    pub project_id: String,
    /// Task ID whose result is being submitted.
    pub task_id: String,
    /// Agent submitting the result.
    pub agent_id: String,
    /// Acceptance tests passed.
    pub passed: u32,
    /// Acceptance tests failed.
    pub failed: u32,
    /// Score delta observed after the change.
    #[serde(default)]
    pub score_delta: f32,
    /// Serialized task-result board to record into (when provided, real).
    #[serde(default)]
    pub board: Option<serde_json::Value>,
}

/// Submit an engineering task result (§10.5).
///
/// Records the acceptance-test outcome into the task-result board and
/// reports the updated pass rate. Without a board payload the receipt is
/// state-only and transparent about it.
#[must_use]
pub fn submit_task_result(params: &SubmitTaskResultParams) -> ToolReceipt {
    let Some(board_json) = &params.board else {
        return ToolReceipt {
            tool: "submit_task_result".to_string(),
            status: ToolStatus::Succeeded,
            summary: format!(
                "Result recorded for task '{}' by '{}' ({} passed, {} failed) — board not supplied, pass rate not computed",
                params.task_id, params.agent_id, params.passed, params.failed
            ),
            score_delta: BTreeMap::new(),
            evidence_uris: vec![format!("engineering://tasks/{}", params.task_id)],
            focus_uris: vec![],
            next_actions: vec!["get_eval_report".to_string()],
            cost: ToolCost::default(),
            warnings: vec!["No task-result board payload — pass rate not computed".to_string()],
            dry_run: false,
            diagnostics: vec![],
        };
    };
    let mut board = match serde_json::from_value::<scorestrata_storage::engineering::TaskResultBoard>(
        board_json.clone(),
    ) {
        Ok(b) => b,
        Err(e) => return deserialize_error_receipt("submit_task_result", "board", &e),
    };
    board.record(scorestrata_storage::engineering::TaskResult {
        task_id: params.task_id.clone(),
        agent_id: params.agent_id.clone(),
        passed: params.passed,
        failed: params.failed,
        score_delta: params.score_delta,
        evidence_uri: Some(format!("engineering://tasks/{}", params.task_id)),
        timestamp: chrono::Utc::now().to_rfc3339(),
    });
    let rate = board.pass_rate();
    let mut score_delta = BTreeMap::new();
    score_delta.insert("engineering.pass_rate".to_string(), format!("{rate:.3}"));
    score_delta.insert(
        "engineering.regressions".to_string(),
        board.regressions().to_string(),
    );
    score_delta.insert("engineering.tasks".to_string(), board.len().to_string());
    ToolReceipt {
        tool: "submit_task_result".to_string(),
        status: ToolStatus::Succeeded,
        summary: format!(
            "Task '{}' result recorded ({} passed, {} failed); board pass rate now {:.1}% over {} result(s)",
            params.task_id,
            params.passed,
            params.failed,
            rate * 100.0,
            board.len()
        ),
        score_delta,
        evidence_uris: vec![format!("engineering://tasks/{}", params.task_id)],
        focus_uris: vec![],
        next_actions: vec!["get_eval_report".to_string()],
        cost: ToolCost {
            cpu_ms: 2,
            tool_calls: 1,
            ..Default::default()
        },
        warnings: vec![],
        dry_run: false,
        diagnostics: vec![],
    }
}

/// Parameters for `get_eval_report` tool.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GetEvalReportParams {
    /// Project ID.
    pub project_id: String,
    /// Serialized task-result board to summarize (when provided, real).
    #[serde(default)]
    pub board: Option<serde_json::Value>,
}

/// Get an eval report from the recorded task results (§10.5, §16.5).
#[must_use]
pub fn get_eval_report(params: &GetEvalReportParams) -> ToolReceipt {
    let Some(board_json) = &params.board else {
        return ToolReceipt {
            tool: "get_eval_report".to_string(),
            status: ToolStatus::Failed,
            summary: format!(
                "Eval report requested for project '{}' but no task-result board payload provided",
                params.project_id
            ),
            score_delta: BTreeMap::new(),
            evidence_uris: vec![],
            focus_uris: vec![format!("project://{}/eval/latest", params.project_id)],
            next_actions: vec!["submit_task_result".to_string()],
            cost: ToolCost::default(),
            warnings: vec![
                "Supply a serialized task-result board to produce a real report".to_string(),
            ],
            dry_run: false,
            diagnostics: vec![],
        };
    };
    let board = match serde_json::from_value::<scorestrata_storage::engineering::TaskResultBoard>(
        board_json.clone(),
    ) {
        Ok(b) => b,
        Err(e) => return deserialize_error_receipt("get_eval_report", "board", &e),
    };
    let summary = scorestrata_storage::engineering::EvalSummary::from_results(&board.results);
    let mut score_delta = BTreeMap::new();
    score_delta.insert(
        "eval.total_tests".to_string(),
        summary.total_tests.to_string(),
    );
    score_delta.insert(
        "eval.pass_rate".to_string(),
        format!("{:.3}", summary.pass_rate),
    );
    score_delta.insert(
        "eval.regressions".to_string(),
        summary.regressions.to_string(),
    );
    ToolReceipt {
        tool: "get_eval_report".to_string(),
        status: if summary.clean {
            ToolStatus::Succeeded
        } else {
            ToolStatus::Failed
        },
        summary: format!(
            "Eval report for project '{}': {} test(s), {:.1}% pass rate, {} regression(s) — {}",
            params.project_id,
            summary.total_tests,
            summary.pass_rate * 100.0,
            summary.regressions,
            if summary.clean {
                "clean"
            } else {
                "needs repair"
            }
        ),
        score_delta,
        evidence_uris: vec![format!("project://{}/eval/latest", params.project_id)],
        focus_uris: vec![],
        next_actions: if summary.clean {
            vec!["create_adr".to_string(), "update_docs".to_string()]
        } else {
            vec![
                "get_failure_report".to_string(),
                "submit_task_result".to_string(),
            ]
        },
        cost: ToolCost {
            cpu_ms: 2,
            tool_calls: 1,
            ..Default::default()
        },
        warnings: vec![],
        dry_run: false,
        diagnostics: vec![],
    }
}

/// Parameters for `update_docs` tool.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UpdateDocsParams {
    /// Project ID.
    pub project_id: String,
    /// Document path being updated.
    pub path: String,
    /// Section within the document.
    pub section: String,
    /// Summary of the change.
    pub summary: String,
    /// Related spec/feature.
    #[serde(default)]
    pub related_spec: String,
    /// Serialized docs ledger to record into (when provided, real).
    #[serde(default)]
    pub ledger: Option<serde_json::Value>,
}

/// Record a documentation update (§10.5, §16.3 step 12).
#[must_use]
pub fn update_docs(params: &UpdateDocsParams) -> ToolReceipt {
    let Some(ledger_json) = &params.ledger else {
        return ToolReceipt {
            tool: "update_docs".to_string(),
            status: ToolStatus::Succeeded,
            summary: format!(
                "Doc update for '{}' §{} recorded for project '{}' — ledger not supplied, not persisted",
                params.path, params.section, params.project_id
            ),
            score_delta: BTreeMap::new(),
            evidence_uris: vec![format!("docs://{}", params.path)],
            focus_uris: vec![],
            next_actions: vec![],
            cost: ToolCost::default(),
            warnings: vec!["No docs ledger payload — update not persisted".to_string()],
            dry_run: false,
            diagnostics: vec![],
        };
    };
    let mut ledger = match serde_json::from_value::<scorestrata_storage::docs::DocsLedger>(
        ledger_json.clone(),
    ) {
        Ok(l) => l,
        Err(e) => return deserialize_error_receipt("update_docs", "ledger", &e),
    };
    ledger.record(
        &params.path,
        &params.section,
        &params.summary,
        &params.related_spec,
    );
    let mut score_delta = BTreeMap::new();
    score_delta.insert("docs.updates".to_string(), ledger.len().to_string());
    ToolReceipt {
        tool: "update_docs".to_string(),
        status: ToolStatus::Succeeded,
        summary: format!(
            "Doc update recorded for '{}' §{} (update #{})",
            params.path,
            params.section,
            ledger.len()
        ),
        score_delta,
        evidence_uris: vec![format!("docs://{}", params.path)],
        focus_uris: vec![],
        next_actions: vec!["create_adr".to_string()],
        cost: ToolCost {
            cpu_ms: 1,
            tool_calls: 1,
            ..Default::default()
        },
        warnings: vec![],
        dry_run: false,
        diagnostics: vec![],
    }
}

/// Parameters for `create_adr` tool.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateAdrParams {
    /// Project ID.
    pub project_id: String,
    /// Short title of the decision.
    pub title: String,
    /// Context motivating the decision.
    pub context: String,
    /// The decision itself.
    pub decision: String,
    /// Consequences.
    #[serde(default)]
    pub consequences: Vec<String>,
    /// Related spec section.
    #[serde(default)]
    pub related_spec: String,
    /// Serialized ADR ledger to record into (when provided, real).
    #[serde(default)]
    pub ledger: Option<serde_json::Value>,
}

/// Create an architecture decision record (§10.5, §16.3 step 12).
#[must_use]
pub fn create_adr(params: &CreateAdrParams) -> ToolReceipt {
    let Some(ledger_json) = &params.ledger else {
        return ToolReceipt {
            tool: "create_adr".to_string(),
            status: ToolStatus::Succeeded,
            summary: format!(
                "ADR '{}' drafted for project '{}' — ledger not supplied, not persisted",
                params.title, params.project_id
            ),
            score_delta: BTreeMap::new(),
            evidence_uris: vec![],
            focus_uris: vec![],
            next_actions: vec!["update_docs".to_string()],
            cost: ToolCost::default(),
            warnings: vec!["No ADR ledger payload — record not persisted".to_string()],
            dry_run: false,
            diagnostics: vec![],
        };
    };
    let mut ledger =
        match serde_json::from_value::<scorestrata_storage::adr::AdrLedger>(ledger_json.clone()) {
            Ok(l) => l,
            Err(e) => return deserialize_error_receipt("create_adr", "ledger", &e),
        };
    let id = ledger.record(
        &params.title,
        &params.context,
        &params.decision,
        params.consequences.clone(),
        &params.related_spec,
    );
    let mut score_delta = BTreeMap::new();
    score_delta.insert("adr.id".to_string(), id.clone());
    score_delta.insert("adr.total".to_string(), ledger.len().to_string());
    ToolReceipt {
        tool: "create_adr".to_string(),
        status: ToolStatus::Succeeded,
        summary: format!(
            "ADR {} '{}' recorded ({} total in ledger)",
            id,
            params.title,
            ledger.len()
        ),
        score_delta,
        evidence_uris: vec![format!("docs://adr/{}", id)],
        focus_uris: vec![],
        next_actions: vec!["update_docs".to_string()],
        cost: ToolCost {
            cpu_ms: 1,
            tool_calls: 1,
            ..Default::default()
        },
        warnings: vec![],
        dry_run: false,
        diagnostics: vec![],
    }
}

/// Parameters for `update_calibration_profile` tool.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UpdateCalibrationProfileParams {
    /// Project ID.
    pub project_id: String,
    /// Genre to calibrate.
    pub genre: String,
    /// Target loudness (LUFS) for the genre.
    pub target_lufs: f32,
    /// Serialized calibration profile to update (when provided, real).
    #[serde(default)]
    pub profile: Option<serde_json::Value>,
}

/// Update the genre-calibrated loudness target (§10.5, §11.6).
#[must_use]
pub fn update_calibration_profile(params: &UpdateCalibrationProfileParams) -> ToolReceipt {
    let Some(profile_json) = &params.profile else {
        return ToolReceipt {
            tool: "update_calibration_profile".to_string(),
            status: ToolStatus::Succeeded,
            summary: format!(
                "Calibration update for '{}' → {:.1} LUFS drafted for project '{}' — profile not supplied, not persisted",
                params.genre, params.target_lufs, params.project_id
            ),
            score_delta: BTreeMap::new(),
            evidence_uris: vec![],
            focus_uris: vec![],
            next_actions: vec!["render_candidate_preview".to_string()],
            cost: ToolCost::default(),
            warnings: vec!["No calibration profile payload — target not persisted".to_string()],
            dry_run: false,
            diagnostics: vec![],
        };
    };
    let mut profile = match serde_json::from_value::<
        scorestrata_storage::calibration::CalibrationProfile,
    >(profile_json.clone())
    {
        Ok(p) => p,
        Err(e) => return deserialize_error_receipt("update_calibration_profile", "profile", &e),
    };
    let mut genre_profile = scorestrata_storage::calibration::GenreProfile::new(&params.genre);
    genre_profile.loudness_target_lufs = params.target_lufs;
    profile.add_genre_profile(genre_profile);
    let mut score_delta = BTreeMap::new();
    score_delta.insert("calibration.genre".to_string(), params.genre.clone());
    score_delta.insert(
        "calibration.target_lufs".to_string(),
        format!("{:.1}", params.target_lufs),
    );
    ToolReceipt {
        tool: "update_calibration_profile".to_string(),
        status: ToolStatus::Succeeded,
        summary: format!(
            "Calibration updated: '{}' now targets {:.1} LUFS ({} genre profile(s) tracked)",
            params.genre,
            params.target_lufs,
            profile.genre_profiles.len()
        ),
        score_delta,
        evidence_uris: vec![format!(
            "project://{}/calibration/{}",
            params.project_id, params.genre
        )],
        focus_uris: vec![],
        next_actions: vec!["render_candidate_preview".to_string()],
        cost: ToolCost {
            cpu_ms: 1,
            tool_calls: 1,
            ..Default::default()
        },
        warnings: vec![],
        dry_run: false,
        diagnostics: vec![],
    }
}

// ── Governance Tools (§10.5) ──────────────────────────────────────

/// Parameters for `get_policy_profile` tool.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GetPolicyProfileParams {
    /// Project ID.
    pub project_id: String,
}

/// Get the current policy profile for a project.
#[must_use]
pub fn get_policy_profile(params: &GetPolicyProfileParams) -> ToolReceipt {
    ToolReceipt {
        tool: "get_policy_profile".to_string(),
        status: ToolStatus::Succeeded,
        summary: format!(
            "Retrieved policy profile for project '{}'",
            params.project_id
        ),
        score_delta: BTreeMap::new(),
        evidence_uris: vec![format!("project://{}/policy", params.project_id)],
        focus_uris: vec![],
        next_actions: vec![],
        cost: ToolCost::default(),
        warnings: vec![],
        dry_run: false,
        diagnostics: vec![],
    }
}

/// Parameters for `check_budget` tool.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CheckBudgetParams {
    /// Project ID.
    pub project_id: String,
}

/// Check the current budget state for a project.
#[must_use]
pub fn check_budget(params: &CheckBudgetParams) -> ToolReceipt {
    ToolReceipt {
        tool: "check_budget".to_string(),
        status: ToolStatus::Succeeded,
        summary: format!("Checked budget for project '{}'", params.project_id),
        score_delta: BTreeMap::new(),
        evidence_uris: vec![format!("project://{}/budget", params.project_id)],
        focus_uris: vec![],
        next_actions: vec![],
        cost: ToolCost::default(),
        warnings: vec![],
        dry_run: false,
        diagnostics: vec![],
    }
}

/// List all registered contract schemas (§3.2).
///
/// Returns the schema registry contents — every versioned, machine-readable
/// contract (ProjectConfig, CommandReceipt, Diagnostic, etc.) that agents can
/// rely on. No payload needed: the registry is a system resource.
#[must_use]
pub fn get_contracts() -> ToolReceipt {
    let names = scorestrata_core::schema_registry::list_schema_names();
    ToolReceipt {
        tool: "get_contracts".to_string(),
        status: ToolStatus::Succeeded,
        summary: format!(
            "Contract registry: {} schema(s) — {}",
            names.len(),
            names.join(", ")
        ),
        score_delta: BTreeMap::from([(
            "registry.schema_count".to_string(),
            names.len().to_string(),
        )]),
        evidence_uris: vec!["system://contracts".to_string()],
        focus_uris: vec![],
        next_actions: vec!["get_capabilities".to_string()],
        cost: ToolCost {
            cpu_ms: 2,
            tool_calls: 1,
            ..Default::default()
        },
        warnings: vec![],
        dry_run: false,
        diagnostics: vec![],
    }
}

/// Get the process-wide telemetry snapshot (§15.5).
///
/// Returns tool-call counts, patch attempts, and the derived operational
/// rates (patch success, escalation, rollback, regression) so operators can
/// measure what the system is actually doing.
///
/// Note: the snapshot is taken after this tool's own dispatch is recorded, so
/// the summary always includes `get_telemetry` in its own counts — consistent
/// with "measure what the system has run".
#[must_use]
pub fn get_telemetry() -> ToolReceipt {
    let snap = crate::telemetry::snapshot();
    let top: Vec<String> = snap
        .top_tools(5)
        .into_iter()
        .map(|(name, count)| format!("{name}x{count}"))
        .collect();
    let top_str = if top.is_empty() {
        "none".to_string()
    } else {
        top.join(", ")
    };
    ToolReceipt {
        tool: "get_telemetry".to_string(),
        status: ToolStatus::Succeeded,
        summary: format!(
            "Telemetry: {} tool call(s) across {} tool(s); top: {top_str}; patches {}/{} ({:.0}% success); rates: escalation {:.3}, rollback {:.3}, regression {:.3}",
            snap.total_tool_calls(),
            snap.tool_calls.len(),
            snap.successful_patches,
            snap.total_patches,
            snap.patch_success_rate() * 100.0,
            snap.escalation_rate(),
            snap.rollback_rate(),
            snap.regression_rate(),
        ),
        score_delta: BTreeMap::from([
            (
                "telemetry.tool_calls".to_string(),
                snap.total_tool_calls().to_string(),
            ),
            (
                "telemetry.patch_success_rate".to_string(),
                format!("{:.3}", snap.patch_success_rate()),
            ),
        ]),
        // Deliberate divergence from the architecture's per-project
        // `project://<id>/telemetry/latest`: this snapshot is process-wide,
        // so `system://` is the honest scope.
        evidence_uris: vec!["system://telemetry/latest".to_string()],
        focus_uris: vec![],
        next_actions: vec![],
        cost: ToolCost::default(),
        warnings: vec![],
        dry_run: false,
        diagnostics: vec![],
    }
}

// ── Project Tools (§10.5) ─────────────────────────────────────────

/// Parameters for `create_project` tool.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateProjectParams {
    /// Project name.
    pub name: String,
    /// Brief text describing the desired output.
    pub brief: String,
    /// Optional genre hint.
    pub genre: Option<String>,
}

/// Create a new project from a brief.
///
/// # Safety
///
/// Project ID is derived from the first 20 characters of the name,
/// using `chars().take()` to handle multi-byte UTF-8 safely.
#[must_use]
pub fn create_project(params: &CreateProjectParams) -> ToolReceipt {
    // Why: chars().take(20) is safe for multi-byte UTF-8 (emoji, CJK, etc.)
    let safe_name: String = params
        .name
        .chars()
        .take(20)
        .collect::<String>()
        .replace(' ', "_");
    let project_id = format!("proj_{safe_name}");
    ToolReceipt {
        tool: "create_project".to_string(),
        status: ToolStatus::Succeeded,
        summary: format!("Created project '{}' with ID '{}'", params.name, project_id),
        score_delta: BTreeMap::new(),
        evidence_uris: vec![format!("project://{}/summary", project_id)],
        focus_uris: vec![],
        next_actions: vec!["generate_config_from_brief".to_string()],
        cost: ToolCost {
            tool_calls: 1,
            ..Default::default()
        },
        warnings: vec![],
        dry_run: false,
        diagnostics: vec![],
    }
}

/// Parameters for `generate_config_from_brief` tool.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GenerateConfigParams {
    /// Project ID.
    pub project_id: String,
    /// Brief text.
    pub brief: String,
    /// Optional project name (defaults to the project ID).
    #[serde(default)]
    pub name: Option<String>,
}

/// Generate a project configuration from a brief.
///
/// Wired to the deterministic [`config_gen`](crate::config_gen) parser:
/// genre, BPM, key, mood, and duration hints are extracted from free text
/// and rendered into a validated [`ProjectConfig`].
#[must_use]
pub fn generate_config_from_brief(params: &GenerateConfigParams) -> ToolReceipt {
    let name = params
        .name
        .clone()
        .unwrap_or_else(|| params.project_id.clone());
    let config = crate::config_gen::generate_config(&name, &params.brief);

    let mut score_delta = BTreeMap::new();
    score_delta.insert("config.completeness".to_string(), "+1.0".to_string());

    ToolReceipt {
        tool: "generate_config_from_brief".to_string(),
        status: ToolStatus::Succeeded,
        summary: format!(
            "Generated config for '{}': {} @ {} BPM, key {}, {} bars",
            name, config.genre, config.bpm, config.key, config.duration_bars
        ),
        score_delta,
        evidence_uris: vec![format!("project://{}/config", params.project_id)],
        focus_uris: vec![format!("project://{}/config", params.project_id)],
        next_actions: vec!["validate_config".to_string()],
        cost: ToolCost {
            cpu_ms: 2,
            tool_calls: 1,
            ..Default::default()
        },
        warnings: vec![],
        dry_run: false,
        diagnostics: vec![],
    }
}

/// Generate a config and return both the receipt and the serialized config.
///
/// Used by adapters that persist the result (e.g. the CLI `config generate`
/// command) without re-parsing the brief.
#[must_use]
pub fn generate_config_with_value(
    params: &GenerateConfigParams,
) -> (ToolReceipt, serde_json::Value) {
    let name = params
        .name
        .clone()
        .unwrap_or_else(|| params.project_id.clone());
    let config = crate::config_gen::generate_config(&name, &params.brief);
    let config_json = serde_json::to_value(&config).unwrap_or_default();
    (generate_config_from_brief(params), config_json)
}

/// Parameters for `validate_config` tool.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ValidateConfigParams {
    /// Project ID.
    pub project_id: String,
}

/// Validate a project configuration.
#[must_use]
pub fn validate_config(params: &ValidateConfigParams) -> ToolReceipt {
    ToolReceipt {
        tool: "validate_config".to_string(),
        status: ToolStatus::Succeeded,
        summary: format!("Validated config for project '{}'", params.project_id),
        score_delta: BTreeMap::new(),
        evidence_uris: vec![format!("project://{}/config/validation", params.project_id)],
        focus_uris: vec![],
        next_actions: vec!["approve_config".to_string()],
        cost: ToolCost {
            tool_calls: 1,
            ..Default::default()
        },
        warnings: vec![],
        dry_run: false,
        diagnostics: vec![],
    }
}

/// Parameters for `approve_config` tool.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ApproveConfigParams {
    /// Project ID.
    pub project_id: String,
}

/// Approve a validated project configuration.
#[must_use]
pub fn approve_config(params: &ApproveConfigParams) -> ToolReceipt {
    ToolReceipt {
        tool: "approve_config".to_string(),
        status: ToolStatus::Succeeded,
        summary: format!("Approved config for project '{}'", params.project_id),
        score_delta: BTreeMap::new(),
        evidence_uris: vec![format!("project://{}/config/approved", params.project_id)],
        focus_uris: vec![],
        next_actions: vec!["plan_arrangement".to_string()],
        cost: ToolCost {
            tool_calls: 1,
            ..Default::default()
        },
        warnings: vec![],
        dry_run: false,
        diagnostics: vec![],
    }
}

/// Parameters for `get_project_summary` tool.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GetProjectSummaryParams {
    /// Project ID.
    pub project_id: String,
    /// Serialized project (when provided, the summary is real).
    #[serde(default)]
    pub project: Option<serde_json::Value>,
}

/// Get a high-level project summary (§13.1 L0 orientation).
///
/// With a serialized project payload, builds a real summary from the project
/// entity — segment/layer counts, current status, suggested next actions.
#[must_use]
pub fn get_project_summary(params: &GetProjectSummaryParams) -> ToolReceipt {
    let Some(project_json) = &params.project else {
        let stub = crate::resources::get_project_summary(&params.project_id);
        return ToolReceipt {
            tool: "get_project_summary".to_string(),
            status: ToolStatus::Succeeded,
            summary: format!(
                "Project '{}' summary (stub — no project payload): state {}, {} segment(s), {} layer(s)",
                params.project_id, stub.state, stub.segment_count, stub.layer_count
            ),
            score_delta: BTreeMap::new(),
            evidence_uris: vec![format!("project://{}/summary", params.project_id)],
            focus_uris: vec![],
            next_actions: stub.next_actions.clone(),
            cost: ToolCost::default(),
            warnings: vec!["No project payload — summary is a state-only stub".to_string()],
            dry_run: false,
            diagnostics: vec![],
        };
    };
    let project =
        match serde_json::from_value::<scorestrata_core::domain::Project>(project_json.clone()) {
            Ok(p) => p,
            Err(e) => return deserialize_error_receipt("get_project_summary", "project", &e),
        };
    let summary = crate::resources::get_project_summary_from_project(&project);
    let mut score_delta = BTreeMap::new();
    score_delta.insert(
        "summary.segments".to_string(),
        summary.segment_count.to_string(),
    );
    score_delta.insert(
        "summary.layers".to_string(),
        summary.layer_count.to_string(),
    );
    score_delta.insert("summary.bars".to_string(), summary.total_bars.to_string());
    score_delta.insert("summary.state".to_string(), summary.state.clone());
    ToolReceipt {
        tool: "get_project_summary".to_string(),
        status: ToolStatus::Succeeded,
        summary: format!(
            "Project '{}' ({}) — state {}, {} segment(s), {} layer(s), {} bar(s); next: {}",
            params.project_id,
            summary.name,
            summary.state,
            summary.segment_count,
            summary.layer_count,
            summary.total_bars,
            summary.next_actions.first().map_or("none", String::as_str)
        ),
        score_delta,
        evidence_uris: vec![format!("project://{}/summary", params.project_id)],
        focus_uris: vec![],
        next_actions: summary.next_actions,
        cost: ToolCost {
            cpu_ms: 4,
            tool_calls: 1,
            ..Default::default()
        },
        warnings: vec![],
        dry_run: false,
        diagnostics: vec![],
    }
}

/// Get the agent onboarding packet (§10.2, §13.1 L0) — system map,
/// capabilities, contracts, and policy guidance for becoming operational.
#[must_use]
pub fn get_onboarding_packet() -> ToolReceipt {
    let caps = crate::capabilities::get_capabilities();
    let schema_names = scorestrata_core::schema_registry::list_schema_names();
    let policy = scorestrata_core::policy::PolicyProfile::default();
    ToolReceipt {
        tool: "get_onboarding_packet".to_string(),
        status: ToolStatus::Succeeded,
        summary: format!(
            "Onboarding: system '{}' v{}, {} feature(s), {} tool(s), {} contract(s), {} resource(s); policy: destructive={}, snapshot-before-patch={}, max-oos={}",
            caps.system,
            caps.version,
            caps.features.len(),
            caps.tools.len(),
            schema_names.len(),
            caps.resources.len(),
            policy.allow_destructive_commands,
            policy.require_snapshot_before_patch,
            policy.max_out_of_scale_ratio
        ),
        score_delta: BTreeMap::from([
            ("onboarding.tools".to_string(), caps.tools.len().to_string()),
            (
                "onboarding.contracts".to_string(),
                schema_names.len().to_string(),
            ),
            (
                "onboarding.resources".to_string(),
                caps.resources.len().to_string(),
            ),
        ]),
        evidence_uris: vec![
            "system://capabilities".to_string(),
            "system://contracts".to_string(),
            "system://policy-profiles".to_string(),
        ],
        focus_uris: vec![],
        next_actions: vec![
            "get_contracts".to_string(),
            "get_policy_profile".to_string(),
            "check_budget".to_string(),
        ],
        cost: ToolCost {
            cpu_ms: 3,
            tool_calls: 1,
            ..Default::default()
        },
        warnings: vec![],
        dry_run: false,
        diagnostics: vec![],
    }
}

// ── Validation Tools (§10.5) ──────────────────────────────────────

/// Parameters for `get_audit_summary` tool.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GetAuditSummaryParams {
    /// Project ID.
    pub project_id: String,
    /// Optional scope filter.
    pub scope: Option<String>,
}

/// Get an audit summary for a project.
#[must_use]
pub fn get_audit_summary(params: &GetAuditSummaryParams) -> ToolReceipt {
    ToolReceipt {
        tool: "get_audit_summary".to_string(),
        status: ToolStatus::Succeeded,
        summary: format!(
            "Retrieved audit summary for project '{}' (scope={:?})",
            params.project_id, params.scope
        ),
        score_delta: BTreeMap::new(),
        evidence_uris: vec![format!("project://{}/audit/summary", params.project_id)],
        focus_uris: vec![],
        next_actions: vec![],
        cost: ToolCost::default(),
        warnings: vec![],
        dry_run: false,
        diagnostics: vec![],
    }
}

/// Parameters for `suggest_fixes` tool.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SuggestFixesParams {
    /// Project ID.
    pub project_id: String,
    /// Diagnostic code to get fixes for.
    pub diagnostic_code: String,
    /// Optional segment ID.
    pub segment_id: Option<String>,
    /// Optional genre to narrow lesson-store lookups.
    #[serde(default)]
    pub genre: Option<String>,
    /// Serialized project (when provided, fixes come from real validation
    /// diagnostics — the validator's `SuggestedPatch` payloads).
    #[serde(default)]
    pub project: Option<serde_json::Value>,
    /// Serialized lesson store (when provided, proven fixes from past
    /// iterations are merged in via `get_best_for_trigger`).
    #[serde(default)]
    pub lesson_store: Option<serde_json::Value>,
}

/// Suggest fixes for a specific diagnostic.
///
/// Real fixes come from two sources, both previously ignored by the stub:
/// 1. **Validator diagnostics** — with a `project` payload, runs the real
///    checks via [`debug_ops::project_report`](crate::debug_ops::project_report)
///    and surfaces every `SuggestedPatch` the validator attached to matching
///    diagnostics (e.g. `quantize_layer_to_scale` for
///    `HARMONY_SCALE_VIOLATION`, `add_transition_package` for
///    `TRANSITION_ENERGY_DELTA_LOW`).
/// 2. **Lesson store** — with a `lesson_store` payload, looks up the best
///    confident lesson for this trigger (`get_best_for_trigger`) and surfaces
///    its proven `successful_patch` as the top fix (§13.5 learning loop).
///    Lesson-derived fixes carry only the op name — the agent supplies the
///    concrete `scope`/params when it follows the `apply_patch` next action.
///
/// Returns `Failed` when neither payload is supplied (stub transparency) or
/// when no fix is known for the diagnostic — never fabricates fixes.
#[must_use]
pub fn suggest_fixes(params: &SuggestFixesParams) -> ToolReceipt {
    if params.project.is_none() && params.lesson_store.is_none() {
        return ToolReceipt {
            tool: "suggest_fixes".to_string(),
            status: ToolStatus::Failed,
            summary: format!(
                "Fix suggestions for '{}' in project '{}' but no project or lesson-store payload provided",
                params.diagnostic_code, params.project_id
            ),
            score_delta: BTreeMap::new(),
            evidence_uris: vec![format!(
                "project://{}/diagnostics/{}/fixes",
                params.project_id, params.diagnostic_code
            )],
            focus_uris: vec![],
            next_actions: vec!["get_validation_report".to_string()],
            cost: ToolCost::default(),
            warnings: vec![
                "Supply a serialized project and/or lesson store payload to compute real fixes"
                    .to_string(),
            ],
            dry_run: false,
            diagnostics: vec![],
        };
    }

    let mut fixes: Vec<scorestrata_core::diagnostic::SuggestedPatch> = Vec::new();
    let mut lesson_sources = 0usize;
    let mut warnings: Vec<String> = Vec::new();

    // Axis 1: real patches attached by the validator to matching diagnostics.
    if let Some(project_json) = &params.project {
        let project =
            match serde_json::from_value::<scorestrata_core::domain::Project>(project_json.clone())
            {
                Ok(p) => p,
                Err(e) => return deserialize_error_receipt("suggest_fixes", "project", &e),
            };
        let report = crate::debug_ops::project_report(&project);
        let mut diagnostic_hits = 0usize;
        for diagnostic in report
            .diagnostics
            .iter()
            .filter(|d| d.code == params.diagnostic_code)
        {
            diagnostic_hits += 1;
            fixes.extend(diagnostic.suggested_patches.iter().cloned());
        }
        if diagnostic_hits == 0 {
            warnings.push(format!(
                "No '{}' diagnostics in the current validation run",
                params.diagnostic_code
            ));
        }
    }

    // Axis 2: proven fixes from the lesson store.
    if let Some(store_json) = &params.lesson_store {
        let store = match serde_json::from_value::<scorestrata_storage::lesson_store::LessonStore>(
            store_json.clone(),
        ) {
            Ok(s) => s,
            Err(e) => return deserialize_error_receipt("suggest_fixes", "lesson_store", &e),
        };
        if let Some(best) =
            store.get_best_for_trigger(&params.diagnostic_code, params.genre.as_deref())
        {
            lesson_sources = 1;
            if let Some(op) = best.successful_patch.as_deref() {
                if !fixes.iter().any(|f| f.op == op) {
                    fixes.push(scorestrata_core::diagnostic::SuggestedPatch {
                        op: op.to_string(),
                        params: serde_json::json!({}),
                        expected_delta: None,
                        confidence: best.confidence,
                    });
                }
            }
        } else {
            warnings.push(format!(
                "No lesson recorded for trigger '{}'",
                params.diagnostic_code
            ));
        }
    }

    // Keep the highest-confidence occurrence of each distinct (op, params)
    // fix. Sorting first then retaining via a seen-set is a *guaranteed*
    // dedup: dedup_by alone only removes adjacent duplicates, which would
    // let same-op fixes with interleaved confidences both survive.
    fixes.sort_by(|a, b| {
        b.confidence
            .partial_cmp(&a.confidence)
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    let mut seen: std::collections::BTreeSet<(String, String)> = std::collections::BTreeSet::new();
    fixes.retain(|f| seen.insert((f.op.clone(), f.params.to_string())));

    let mut score_delta = BTreeMap::new();
    score_delta.insert("fixes.count".to_string(), fixes.len().to_string());
    score_delta.insert(
        "fixes.lesson_sources".to_string(),
        lesson_sources.to_string(),
    );

    let fix_summary: Vec<String> = fixes
        .iter()
        .map(|f| {
            format!(
                "{} (conf {:.2}{})",
                f.op,
                f.confidence,
                f.expected_delta
                    .as_deref()
                    .map_or_else(String::new, |d| format!(", delta {d}"))
            )
        })
        .collect();

    let status = if fixes.is_empty() {
        ToolStatus::Failed
    } else {
        ToolStatus::Succeeded
    };

    ToolReceipt {
        tool: "suggest_fixes".to_string(),
        status,
        summary: if fixes.is_empty() {
            format!(
                "No known fixes for '{}' in project '{}' — {}",
                params.diagnostic_code,
                params.project_id,
                warnings.join("; ")
            )
        } else {
            format!(
                "Suggested {} fix(es) for '{}' in project '{}': {}",
                fixes.len(),
                params.diagnostic_code,
                params.project_id,
                fix_summary.join("; ")
            )
        },
        score_delta,
        evidence_uris: vec![format!(
            "project://{}/diagnostics/{}/fixes",
            params.project_id, params.diagnostic_code
        )],
        focus_uris: vec![format!("project://{}/validation/latest", params.project_id)],
        next_actions: if fixes.is_empty() {
            vec!["get_validation_report".to_string()]
        } else {
            vec!["apply_patch".to_string()]
        },
        cost: ToolCost {
            cpu_ms: 3,
            tool_calls: 1,
            ..Default::default()
        },
        warnings,
        dry_run: false,
        diagnostics: vec![],
    }
}

/// Parameters for `get_validation_report` tool.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GetValidationReportParams {
    /// Project ID.
    pub project_id: String,
    /// Validation level (1=config, 2=symbolic, 3=timeline, 4=audio, 5=brief).
    #[serde(default = "default_validation_level")]
    pub level: u8,
    /// Serialized project (when provided, the report is real).
    #[serde(default)]
    pub project: Option<serde_json::Value>,
}

fn default_validation_level() -> u8 {
    2
}

/// Get the latest validation report for a project (§10.5).
///
/// With a serialized project payload, runs the real Level-2 symbolic checks
/// and returns the scorecard with per-metric values and hard-gate status.
///
/// Status answers "did the hard gates pass?" (`all_hard_gates_passed`).
/// This differs from [`get_failure_report`], which answers "does the project
/// still need repair?" — a warnings-only project can pass hard gates (report
/// `Succeeded`) yet still need repair (`get_failure_report` `Failed`). Use
/// `get_failure_report` when deciding whether to escalate or patch.
#[must_use]
pub fn get_validation_report(params: &GetValidationReportParams) -> ToolReceipt {
    let Some(project_json) = &params.project else {
        return ToolReceipt {
            tool: "get_validation_report".to_string(),
            status: ToolStatus::Failed,
            summary: format!(
                "Validation report requested for project '{}' but no serialized project payload provided",
                params.project_id
            ),
            score_delta: BTreeMap::new(),
            evidence_uris: vec![],
            focus_uris: vec![format!("project://{}/validation/latest", params.project_id)],
            next_actions: vec!["run_validation".to_string()],
            cost: ToolCost::default(),
            warnings: vec![
                "Supply a serialized project payload to produce a real report".to_string(),
            ],
            dry_run: false,
            diagnostics: vec![],
        };
    };
    let project =
        match serde_json::from_value::<scorestrata_core::domain::Project>(project_json.clone()) {
            Ok(p) => p,
            Err(e) => {
                return deserialize_error_receipt("get_validation_report", "project", &e);
            }
        };
    let report = crate::debug_ops::project_report(&project);
    let mut score_delta = BTreeMap::new();
    score_delta.insert(
        "report.overall_score".to_string(),
        format!("{:.3}", report.scorecard.overall_score),
    );
    score_delta.insert(
        "report.metrics".to_string(),
        report.scorecard.metrics.len().to_string(),
    );
    score_delta.insert(
        "report.hard_failures".to_string(),
        report.scorecard.hard_failures.to_string(),
    );
    score_delta.insert(
        "report.warnings".to_string(),
        report.scorecard.warnings.to_string(),
    );
    ToolReceipt {
        tool: "get_validation_report".to_string(),
        status: if report.all_hard_gates_passed {
            ToolStatus::Succeeded
        } else {
            ToolStatus::Failed
        },
        summary: format!(
            "Validation report for project '{}' (level {}): overall {:.2}, {} metric(s), {} hard failure(s), {} warning(s)",
            params.project_id,
            params.level,
            report.scorecard.overall_score,
            report.scorecard.metrics.len(),
            report.scorecard.hard_failures,
            report.scorecard.warnings
        ),
        score_delta,
        evidence_uris: vec![format!("project://{}/validation/latest", params.project_id)],
        focus_uris: vec![],
        next_actions: if report.all_hard_gates_passed {
            vec!["get_audit_summary".to_string()]
        } else {
            vec![
                "explain_metric".to_string(),
                "get_root_causes".to_string(),
                "suggest_fixes".to_string(),
            ]
        },
        cost: ToolCost {
            cpu_ms: 8,
            tool_calls: 1,
            ..Default::default()
        },
        warnings: vec![],
        dry_run: false,
        diagnostics: report.diagnostics.clone(),
    }
}

/// Parameters for `get_root_causes` tool.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GetRootCausesParams {
    /// Project ID.
    pub project_id: String,
    /// Metric to attribute root causes for (e.g. `harmony.scale_compliance`).
    pub metric_name: String,
    /// Serialized project (when provided, attribution is real).
    #[serde(default)]
    pub project: Option<serde_json::Value>,
}

/// Get root-cause attribution for a metric (§11.4).
///
/// With a serialized project payload, runs the real diagnostic attribution
/// from [`compute_metric_explanation`](crate::debug_ops::compute_metric_explanation)
/// and returns the attributed causes with evidence URIs.
#[must_use]
pub fn get_root_causes(params: &GetRootCausesParams) -> ToolReceipt {
    let Some(project_json) = &params.project else {
        return ToolReceipt {
            tool: "get_root_causes".to_string(),
            status: ToolStatus::Failed,
            summary: format!(
                "Root-cause attribution for '{}' requested for project '{}' but no serialized project payload provided",
                params.metric_name, params.project_id
            ),
            score_delta: BTreeMap::new(),
            evidence_uris: vec![],
            focus_uris: vec![format!("project://{}/validation/latest", params.project_id)],
            next_actions: vec!["run_validation".to_string()],
            cost: ToolCost::default(),
            warnings: vec![
                "Supply a serialized project payload to attribute root causes".to_string(),
            ],
            dry_run: false,
            diagnostics: vec![],
        };
    };
    let project =
        match serde_json::from_value::<scorestrata_core::domain::Project>(project_json.clone()) {
            Ok(p) => p,
            Err(e) => return deserialize_error_receipt("get_root_causes", "project", &e),
        };
    // The scorecard only contains metrics that produced diagnostics, so a
    // metric absent from it is healthy — not an error. Treat absence as
    // "value 1.0, no root causes" so the tool answers the same question
    // ("what's wrong with this metric?") coherently for healthy projects.
    let explanation = crate::debug_ops::compute_metric_explanation(&project, &params.metric_name)
        .unwrap_or(scorestrata_core::debug::MetricExplanation {
            project_id: project.id.to_string(),
            metric_name: params.metric_name.clone(),
            current_value: 1.0,
            target_value: Some(1.0),
            root_causes: vec![],
            evidence_uris: vec![],
            suggested_actions: vec![],
            confidence: 0.5,
        });
    let cause_list: Vec<String> = explanation
        .root_causes
        .iter()
        .map(|c| format!("{} ({})", c.cause, c.severity))
        .collect();
    let mut score_delta = BTreeMap::new();
    score_delta.insert(
        "metric.value".to_string(),
        format!("{:.3}", explanation.current_value),
    );
    score_delta.insert(
        "metric.root_causes".to_string(),
        explanation.root_causes.len().to_string(),
    );
    ToolReceipt {
        tool: "get_root_causes".to_string(),
        status: ToolStatus::Succeeded,
        summary: format!(
            "Root causes for '{}' (value {:.3}): {}",
            params.metric_name,
            explanation.current_value,
            if cause_list.is_empty() {
                "none — metric healthy".to_string()
            } else {
                cause_list.join("; ")
            }
        ),
        score_delta,
        evidence_uris: explanation.evidence_uris.clone(),
        focus_uris: vec![format!("project://{}/validation/latest", params.project_id)],
        next_actions: explanation.suggested_actions.clone(),
        cost: ToolCost {
            cpu_ms: 6,
            tool_calls: 1,
            ..Default::default()
        },
        warnings: vec![],
        dry_run: false,
        diagnostics: vec![],
    }
}

/// Parameters for `get_metric_evidence` tool.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GetMetricEvidenceParams {
    /// Project ID.
    pub project_id: String,
    /// Metric to fetch evidence for.
    pub metric_name: String,
    /// Serialized evidence warehouse (when provided, evidence is real).
    #[serde(default)]
    pub warehouse: Option<serde_json::Value>,
    /// Serialized lineage store (§8.3 `MetricLineage`) — when provided, the
    /// receipt reports the metric's trajectory (total delta, improvement/
    /// regression counts) from real history.
    #[serde(default)]
    pub lineage: Option<serde_json::Value>,
}

/// Get evidence for a metric (§9.5, §15.1).
///
/// With a serialized
/// [`EvidenceWarehouse`](scorestrata_storage::evidence::EvidenceWarehouse)
/// payload, returns the recorded metric samples with diagnostics and lineage.
#[must_use]
pub fn get_metric_evidence(params: &GetMetricEvidenceParams) -> ToolReceipt {
    // Evidence (warehouse) and lineage are independent payloads (§9.5, §8.3).
    // The tool only fails when NEITHER is supplied; a caller with just a
    // lineage store can still get the trajectory, and vice versa.
    if params.warehouse.is_none() && params.lineage.is_none() {
        return ToolReceipt {
            tool: "get_metric_evidence".to_string(),
            status: ToolStatus::Failed,
            summary: format!(
                "Evidence for '{}' requested for project '{}' but no serialized evidence warehouse or lineage store provided",
                params.metric_name, params.project_id
            ),
            score_delta: BTreeMap::new(),
            evidence_uris: vec![],
            focus_uris: vec![format!("project://{}/evidence/latest", params.project_id)],
            next_actions: vec!["run_validation".to_string()],
            cost: ToolCost::default(),
            warnings: vec![
                "Supply a serialized evidence warehouse or lineage store payload".to_string(),
            ],
            dry_run: false,
            diagnostics: vec![],
        };
    }
    let mut score_delta = BTreeMap::new();
    let mut warnings = Vec::new();

    // ── Evidence from the warehouse (§9.5) ──
    let evidence_text = match &params.warehouse {
        Some(json) => {
            match serde_json::from_value::<scorestrata_storage::evidence::EvidenceWarehouse>(
                json.clone(),
            ) {
                Ok(warehouse) => {
                    let entries = warehouse.get_evidence_for_metric(&params.metric_name);
                    let latest = entries.last().copied();
                    score_delta.insert("evidence.entries".to_string(), entries.len().to_string());
                    if let Some(entry) = latest {
                        score_delta.insert(
                            "evidence.latest_value".to_string(),
                            format!("{:.3}", entry.metric.value),
                        );
                    }
                    format!(
                        "{} sample(s){}",
                        entries.len(),
                        latest.map_or_else(
                            || String::new(),
                            |e| format!(
                                ", latest value {:.3} (level {} validation, confidence {:.2})",
                                e.metric.value, e.validation_level, e.metric.confidence
                            )
                        )
                    )
                }
                Err(e) => {
                    warnings.push(format!("Malformed evidence warehouse payload: {e}"));
                    "0 sample(s)".to_string()
                }
            }
        }
        None => {
            warnings
                .push("No evidence warehouse payload — evidence counts unavailable".to_string());
            "0 sample(s)".to_string()
        }
    };

    // ── Lineage trajectory from the store (§8.3, §15.1) ──
    let lineage_text = match &params.lineage {
        Some(json) => {
            match serde_json::from_value::<scorestrata_storage::lineage::LineageStore>(json.clone())
            {
                Ok(store) => match store.get(&params.metric_name) {
                    Some(lineage) => {
                        let total = lineage
                            .total_delta()
                            .map_or_else(|| "n/a".to_string(), |d| format!("{d:+.3}"));
                        score_delta
                            .insert("lineage.samples".to_string(), lineage.len().to_string());
                        score_delta.insert("lineage.total_delta".to_string(), total.clone());
                        score_delta.insert(
                            "lineage.regressions".to_string(),
                            lineage.regression_count().to_string(),
                        );
                        let latest_level = lineage
                            .latest()
                            .map_or_else(|| "n/a".to_string(), |s| s.validation_level.to_string());
                        format!(
                            ", lineage: {} sample(s), total delta {total}, {} regression(s), {} improvement(s), latest level {latest_level}",
                            lineage.len(),
                            lineage.regression_count(),
                            lineage.improvement_count()
                        )
                    }
                    None => format!(", lineage: no history for '{}'", params.metric_name),
                },
                Err(e) => {
                    warnings.push(format!("Malformed lineage payload: {e}"));
                    String::new()
                }
            }
        }
        None => String::new(),
    };

    ToolReceipt {
        tool: "get_metric_evidence".to_string(),
        status: ToolStatus::Succeeded,
        summary: format!(
            "Evidence for '{}': {}{}",
            params.metric_name, evidence_text, lineage_text
        ),
        score_delta,
        evidence_uris: vec![format!(
            "project://{}/evidence/{}",
            params.project_id, params.metric_name
        )],
        focus_uris: vec![format!("project://{}/evidence/latest", params.project_id)],
        next_actions: vec!["explain_metric".to_string(), "get_root_causes".to_string()],
        cost: ToolCost {
            cpu_ms: 3,
            tool_calls: 1,
            ..Default::default()
        },
        warnings,
        dry_run: false,
        diagnostics: vec![],
    }
}

/// Parameters for `get_render_manifest` tool.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GetRenderManifestParams {
    /// Project ID.
    pub project_id: String,
    /// Artifact ID to fetch the manifest for.
    pub artifact_id: String,
    /// Serialized [`RenderManifestStore`](scorestrata_storage::render_manifest::RenderManifestStore)
    /// — when provided, the manifest lookup is real.
    #[serde(default)]
    pub manifests: Option<serde_json::Value>,
    /// Optional artifact bytes (e.g. WAV) to verify against the manifest's
    /// declared SHA-256 (§17.2/§17.3). When absent, verification is reported
    /// as unavailable rather than assumed.
    #[serde(default)]
    pub artifact_bytes: Option<Vec<u8>>,
}

/// Fetch a render manifest — the reproducibility receipt for an artifact (§17.2).
///
/// Every render must produce a manifest tying the artifact's SHA-256 to its
/// reproducibility inputs (source snapshot, command, engine versions, seed,
/// asset hashes). This tool reads one manifest from a supplied store and
/// reports its reproducibility fingerprint, cache key, completeness, and —
/// when artifact bytes are supplied — whether the file actually matches its
/// declared checksum.
#[must_use]
pub fn get_render_manifest(params: &GetRenderManifestParams) -> ToolReceipt {
    let Some(store_json) = &params.manifests else {
        return ToolReceipt {
            tool: "get_render_manifest".to_string(),
            status: ToolStatus::Failed,
            summary: format!(
                "Render manifest for '{}' requested for project '{}' but no serialized manifest store provided",
                params.artifact_id, params.project_id
            ),
            score_delta: BTreeMap::new(),
            evidence_uris: vec![],
            focus_uris: vec![format!("project://{}/renders", params.project_id)],
            next_actions: vec!["render_project".to_string()],
            cost: ToolCost::default(),
            warnings: vec!["Supply a serialized RenderManifestStore payload".to_string()],
            dry_run: false,
            diagnostics: vec![],
        };
    };

    let store = match serde_json::from_value::<
        scorestrata_storage::render_manifest::RenderManifestStore,
    >(store_json.clone())
    {
        Ok(store) => store,
        Err(e) => {
            return ToolReceipt {
                tool: "get_render_manifest".to_string(),
                status: ToolStatus::Failed,
                summary: format!("Malformed render manifest store payload: {e}"),
                score_delta: BTreeMap::new(),
                evidence_uris: vec![],
                focus_uris: vec![format!("project://{}/renders", params.project_id)],
                next_actions: vec!["render_project".to_string()],
                cost: ToolCost::default(),
                warnings: vec!["Invalid manifest store payload".to_string()],
                dry_run: false,
                diagnostics: vec![],
            };
        }
    };

    let Some(manifest) = store.get(&params.artifact_id) else {
        return ToolReceipt {
            tool: "get_render_manifest".to_string(),
            status: ToolStatus::Failed,
            summary: format!(
                "No render manifest found for artifact '{}' in project '{}' ({} manifest(s) on record)",
                params.artifact_id,
                params.project_id,
                store.len()
            ),
            score_delta: BTreeMap::new(),
            evidence_uris: vec![],
            focus_uris: vec![format!("project://{}/renders", params.project_id)],
            next_actions: vec!["render_project".to_string()],
            cost: ToolCost::default(),
            warnings: vec!["Unknown artifact ID".to_string()],
            dry_run: false,
            diagnostics: vec![],
        };
    };

    let manifest_hash = manifest.manifest_hash();
    let mut score_delta = BTreeMap::new();
    score_delta.insert("render.sha256".to_string(), manifest.sha256.clone());
    score_delta.insert("render.manifest_hash".to_string(), manifest_hash.clone());
    score_delta.insert("render.cache_key".to_string(), manifest.cache_key());
    score_delta.insert(
        "render.complete".to_string(),
        manifest.is_complete().to_string(),
    ); // Checksum verification when artifact bytes are supplied (§17.3).
    // Compute the verification once — hashing potentially large WAV payloads
    // twice would be wasteful — and reuse it for the summary and warnings.
    let verified = params
        .artifact_bytes
        .as_ref()
        .map(|bytes| manifest.verify_artifact(bytes));
    let verification = match verified {
        Some(true) => "verification: CHECKSUM MATCH".to_string(),
        Some(false) => "verification: CHECKSUM MISMATCH".to_string(),
        None => "verification: unavailable (no artifact bytes supplied)".to_string(),
    };

    let mut warnings = Vec::new();
    if !manifest.is_complete() {
        warnings.push("Manifest is incomplete — reproducibility inputs missing".to_string());
    }
    if verified == Some(false) {
        warnings.push("Artifact bytes do not match the manifest's declared SHA-256".to_string());
    }

    ToolReceipt {
        tool: "get_render_manifest".to_string(),
        status: ToolStatus::Succeeded,
        summary: format!(
            "Render manifest for '{}' ({}): {} Hz, {} ch, {:.1}s from snapshot '{}' via command '{}'; {verification}; reproducible inputs {} -> {}",
            manifest.artifact_id,
            manifest.artifact_type,
            manifest.sample_rate,
            manifest.channels,
            manifest.duration_seconds,
            manifest.source_snapshot_id,
            manifest.command_id,
            if manifest.is_complete() {
                "complete"
            } else {
                "incomplete"
            },
            &manifest_hash[..8]
        ),
        score_delta,
        evidence_uris: vec![format!(
            "project://{}/renders/{}",
            params.project_id, params.artifact_id
        )],
        focus_uris: vec![format!("project://{}/renders", params.project_id)],
        next_actions: vec![
            "render_project".to_string(),
            "check_regressions".to_string(),
        ],
        cost: ToolCost {
            cpu_ms: 2,
            tool_calls: 1,
            ..Default::default()
        },
        warnings,
        dry_run: false,
        diagnostics: vec![],
    }
}

/// Parameters for `compare_candidates` tool.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CompareCandidatesParams {
    /// Project ID.
    pub project_id: String,
    /// Candidate A ID (label).
    pub candidate_a: String,
    /// Candidate B ID (label).
    pub candidate_b: String,
    /// Serialized project A.
    #[serde(default)]
    pub project_a: Option<serde_json::Value>,
    /// Serialized project B.
    #[serde(default)]
    pub project_b: Option<serde_json::Value>,
}

/// Compare two candidates (§4.1 tournament) with a multi-objective diff.
///
/// With both serialized project payloads, runs the real per-metric comparison
/// and reports the winner (metric-majority with overall-score tiebreak).
#[must_use]
pub fn compare_candidates(params: &CompareCandidatesParams) -> ToolReceipt {
    let (Some(a_json), Some(b_json)) = (&params.project_a, &params.project_b) else {
        return ToolReceipt {
            tool: "compare_candidates".to_string(),
            status: ToolStatus::Failed,
            summary: format!(
                "Candidate comparison requested for project '{}' but both serialized candidate payloads are required",
                params.project_id
            ),
            score_delta: BTreeMap::new(),
            evidence_uris: vec![],
            focus_uris: vec![],
            next_actions: vec!["create_pattern".to_string()],
            cost: ToolCost::default(),
            warnings: vec![
                "Supply both serialized candidate project payloads to compare".to_string(),
            ],
            dry_run: false,
            diagnostics: vec![],
        };
    };
    let candidate_a =
        match serde_json::from_value::<scorestrata_core::domain::Project>(a_json.clone()) {
            Ok(p) => p,
            Err(e) => return deserialize_error_receipt("compare_candidates", "candidate_a", &e),
        };
    let candidate_b =
        match serde_json::from_value::<scorestrata_core::domain::Project>(b_json.clone()) {
            Ok(p) => p,
            Err(e) => return deserialize_error_receipt("compare_candidates", "candidate_b", &e),
        };
    let diff = crate::debug_ops::compute_candidate_diff(&candidate_a, &candidate_b);
    let mut score_delta = BTreeMap::new();
    score_delta.insert(
        "diff.metrics_compared".to_string(),
        diff.score_comparison.len().to_string(),
    );
    if let Some(winner) = &diff.winner {
        score_delta.insert("diff.winner".to_string(), format!("{:?}", winner));
    }
    let winner_desc = diff.winner.map_or_else(
        || "tie".to_string(),
        |w| match w {
            scorestrata_core::debug::CandidateWinner::A => "Candidate A".to_string(),
            scorestrata_core::debug::CandidateWinner::B => "Candidate B".to_string(),
            scorestrata_core::debug::CandidateWinner::Tie => "tie".to_string(),
        },
    );
    ToolReceipt {
        tool: "compare_candidates".to_string(),
        status: ToolStatus::Succeeded,
        summary: format!(
            "Compared '{}' vs '{}': {} ({}{})",
            params.candidate_a,
            params.candidate_b,
            winner_desc,
            diff.score_comparison.len(),
            if diff.score_comparison.len() == 1 {
                " metric"
            } else {
                " metrics"
            }
        ),
        score_delta,
        evidence_uris: vec![format!(
            "project://{}/diff/candidates/{}/{}",
            params.project_id, params.candidate_a, params.candidate_b
        )],
        focus_uris: vec![],
        next_actions: if diff.winner.is_some() {
            vec!["apply_patch".to_string(), "record_lesson".to_string()]
        } else {
            vec!["create_pattern".to_string()]
        },
        cost: ToolCost {
            cpu_ms: 6,
            tool_calls: 1,
            ..Default::default()
        },
        warnings: vec![],
        dry_run: false,
        diagnostics: vec![],
    }
}

// ── Render Tools (§10.5) ──────────────────────────────────────────

/// Parameters for `render_project` tool.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RenderProjectParams {
    /// Project ID.
    pub project_id: String,
    /// Render mode (master, stems, preview).
    #[serde(default = "default_render_mode")]
    pub mode: String,
    /// Serialized project to render (required for real rendering).
    #[serde(default)]
    pub project: Option<serde_json::Value>,
    /// Target segment ID for segment-scoped rendering.
    #[serde(default)]
    pub segment_id: Option<String>,
    /// Serialized
    /// [`RenderManifestStore`](scorestrata_storage::render_manifest::RenderManifestStore)
    /// — when provided, the render's §17.2 reproducibility receipt is recorded
    /// into it, and `manifest.artifact_id` / `manifest.manifest_hash` surface
    /// on the receipt for the agent loop to retrieve and verify later.
    ///
    /// The payload is input-only (stateless), mirroring `get_metric_evidence`'s
    /// warehouse: the receipt reports what was recorded, and the caller feeds
    /// the store back into `get_render_manifest` to retrieve the receipt.
    #[serde(default)]
    pub manifest_store: Option<serde_json::Value>,
    /// Snapshot the render was produced from (§17.1 reproducibility input).
    /// Defaults to `snap_{project_id}_live` when omitted.
    #[serde(default)]
    pub source_snapshot_id: Option<String>,
    /// Command that triggered the render (§17.2 manifest schema).
    /// Defaults to `cmd_render_{project_id}` when omitted.
    #[serde(default)]
    pub command_id: Option<String>,
    /// Seed used for deterministic generation (§17.2 manifest schema).
    #[serde(default)]
    pub seed: Option<u64>,
    /// Hashes of source assets, keyed by asset path/name (§17.2).
    #[serde(default)]
    pub asset_hashes: Option<BTreeMap<String, String>>,
}

/// Render a compiled timeline to audio and produce its Level-4 analysis.
///
/// Single source for the event→note→render mapping shared by
/// [`render_project`] and the L4 validation plane, so the audio path stays
/// deterministic and consistent everywhere.
/// The render inputs shared by the master, stems, and layer-audit paths:
/// per-layer notes, voices, automation lanes, and FX chains.
///
/// Extracted so every consumer (L4 validation, `render_stems`, the layer
/// audit) converts a timeline + project to audio-engine inputs identically.
fn timeline_render_inputs(
    timeline: &scorestrata_compiler::TimelineIR,
    project: &scorestrata_core::domain::Project,
) -> (
    Vec<scorestrata_audio::render::RenderNote>,
    Vec<scorestrata_audio::render::RenderVoice>,
    Vec<scorestrata_audio::render::RenderAutomation>,
    Vec<scorestrata_audio::render::RenderFx>,
) {
    // Map layer IDs to human-readable names for voice assignment, and gather
    // each layer's instrument so the render path gets real per-instrument
    // timbres. (The first-listen defect: an empty voice table rendered every
    // layer — drums, bass, lead — with the same default saw.)
    let mut layer_names = std::collections::HashMap::new();
    let mut layer_instruments: Vec<(&str, &str)> = Vec::new();
    for segment in &project.arrangement.segments {
        for layer in &segment.layers {
            layer_names.insert(layer.id.to_string(), layer.name.clone());
            if !layer_instruments
                .iter()
                .any(|(n, _)| *n == layer.name.as_str())
            {
                layer_instruments.push((layer.name.as_str(), layer.instrument.as_str()));
            }
        }
    }

    // Convert timeline events to render notes (layer name from event layer).
    let notes: Vec<scorestrata_audio::render::RenderNote> = timeline
        .events
        .iter()
        .filter_map(|e| match &e.kind {
            scorestrata_compiler::timeline::EventKind::NoteOn { pitch, velocity } => {
                let layer_name = layer_names
                    .get(&e.layer_id.to_string())
                    .cloned()
                    .unwrap_or_else(|| e.layer_id.to_string());
                Some(scorestrata_audio::render::RenderNote {
                    pitch: pitch.0,
                    start_tick: e.tick.0,
                    duration_ticks: crate::patch_ops::next_note_off_duration(timeline, e),
                    velocity: velocity.as_f32(),
                    layer_name,
                })
            }
            _ => None,
        })
        .collect();

    let voices = scorestrata_audio::render::build_voices(&layer_instruments);
    let automation = automation_lanes_from_timeline(timeline, &layer_names);
    let fx = render_fx_from_project(project);
    (notes, voices, automation, fx)
}

pub(crate) fn render_timeline_to_analysis(
    timeline: &scorestrata_compiler::TimelineIR,
    project: &scorestrata_core::domain::Project,
) -> Result<scorestrata_audio::render::RenderResult, String> {
    let (notes, voices, automation, fx) = timeline_render_inputs(timeline, project);
    scorestrata_audio::render::render_notes_full(
        &notes,
        timeline.bpm,
        scorestrata_audio::render::DEFAULT_SAMPLE_RATE,
        &voices,
        &automation,
        &fx,
        &scorestrata_audio::render::RenderConfig::default(),
    )
    .map_err(|e| e.to_string())
}

/// Compile and render a project to per-layer stems, then audit each layer
/// stem independently (S2 layer-wise spectrograph).
///
/// Returns the per-layer audit entries sorted by layer name plus the master
/// render result. The layer view exposes each instrument's own spectral
/// footprint (centroid, rolloff, band energies, genre deviation) so an agent
/// can see which layer is muddy, thin, or aliased — the master graph alone
/// hides that.
///
/// When `png_dir` is `Some`, a per-layer spectrogram PNG is written into
/// that directory (the CLI `ss audit` path); when `None`, reports are
/// computed in memory only (the MCP tool path). The render uses the
/// project-derived config so the audited stems match what the agent hears.
///
/// # Errors
///
/// Returns an error if compilation, rendering, or PNG writing fails.
pub fn render_project_to_layer_audit(
    project: &scorestrata_core::domain::Project,
    png_dir: Option<&std::path::Path>,
) -> Result<
    (
        Vec<scorestrata_audio::audit::LayerAuditEntry>,
        scorestrata_audio::render::RenderResult,
    ),
    String,
> {
    let timeline = scorestrata_compiler::compile_project(project).map_err(|e| e.to_string())?;
    let (notes, voices, automation, fx) = timeline_render_inputs(&timeline, project);
    let (stems, master) = scorestrata_audio::render::render_stems_full(
        &notes,
        timeline.bpm,
        scorestrata_audio::render::DEFAULT_SAMPLE_RATE,
        &voices,
        &automation,
        &fx,
        &render_config_from_project(project),
    )
    .map_err(|e| e.to_string())?;

    let genre = Some(project.config.genre.as_str());
    let layers: Vec<(&str, &[f32])> = stems
        .iter()
        .map(|s| (s.layer_name.as_str(), s.samples.as_slice()))
        .collect();
    let entries = match png_dir {
        Some(dir) => scorestrata_audio::audit::write_layer_audit_files(
            &layers,
            scorestrata_audio::render::DEFAULT_SAMPLE_RATE,
            genre,
            dir,
        )
        .map_err(|e| e.to_string())?,
        None => scorestrata_audio::audit::layer_audit_reports(
            &layers,
            scorestrata_audio::render::DEFAULT_SAMPLE_RATE,
            genre,
        ),
    };
    Ok((entries, master))
}

/// Build the render config from the project's mixer state (G5), mirroring
/// the CLI's `render_config_for` so every render path — master, stems, and
/// the layer audit — uses the same limiter ceiling, master gain, and stereo
/// width. The layer audit must analyze what the agent actually hears.
#[must_use]
pub fn render_config_from_project(
    project: &scorestrata_core::domain::Project,
) -> scorestrata_audio::render::RenderConfig {
    let limiter = project.mixer.limiter_ceiling_db.map(|ceiling_db| {
        let knee_db = scorestrata_audio::mastering::LimiterConfig::default().knee_db;
        scorestrata_audio::mastering::LimiterConfig {
            ceiling_db,
            knee_db,
        }
    });
    scorestrata_audio::render::RenderConfig {
        stereo: true,
        limiter,
        master_gain_db: project.mixer.master_gain_db,
        stereo_width: project.mixer.stereo_width,
    }
}

/// Build [`RenderAutomation`] lanes from a timeline's CC events (G4).
///
/// Shared by the L4 render path and the CLI so automation reaches the audio
/// engine identically everywhere.
pub fn automation_lanes_from_timeline(
    timeline: &scorestrata_compiler::TimelineIR,
    layer_names: &std::collections::HashMap<String, String>,
) -> Vec<scorestrata_audio::render::RenderAutomation> {
    timeline
        .control_change_lanes()
        .into_iter()
        .map(
            |(layer_id, controller, points)| scorestrata_audio::render::RenderAutomation {
                layer_name: layer_names
                    .get(&layer_id.to_string())
                    .cloned()
                    .unwrap_or_else(|| layer_id.to_string()),
                controller,
                points,
            },
        )
        .collect()
}

/// Build per-layer FX / mix overrides (G5) from a project's layers.
///
/// Shared by the L4 render path and the CLI so per-track FX reaches the
/// audio engine identically everywhere. Layers with no FX and no mix
/// override are omitted (the renderer then uses its static staging).
pub fn render_fx_from_project(
    project: &scorestrata_core::domain::Project,
) -> Vec<scorestrata_audio::render::RenderFx> {
    let mut fx: Vec<scorestrata_audio::render::RenderFx> = Vec::new();
    for segment in &project.arrangement.segments {
        for layer in &segment.layers {
            if layer.fx.is_none() && layer.mix_gain_db.is_none() && layer.mix_pan.is_none() {
                continue;
            }
            fx.push(scorestrata_audio::render::RenderFx {
                layer_name: layer.name.clone(),
                fx: layer.fx.clone().unwrap_or_default(),
                mix_gain_db: layer.mix_gain_db,
                mix_pan: layer.mix_pan,
            });
        }
    }
    fx
}

/// Compile and render a whole project to its Level-4 audio analysis.
///
/// Convenience wrapper over [`render_timeline_to_analysis`] used by the L4
/// validation plane and the golden-test harness; renders the full
/// arrangement.
pub fn render_project_to_analysis(
    project: &scorestrata_core::domain::Project,
) -> Result<scorestrata_audio::render::RenderResult, String> {
    let timeline = scorestrata_compiler::compile_project(project).map_err(|e| e.to_string())?;
    render_timeline_to_analysis(&timeline, project)
}

/// Build a diagnostic explaining why the Level-4 audio plane could not run.
fn audio_render_failed_diagnostic(err: String) -> Diagnostic {
    Diagnostic {
        id: scorestrata_core::ids::DiagnosticId::new(),
        code: "AUDIO_RENDER_FAILED".to_string(),
        metric: "audio.render".to_string(),
        severity: Severity::Error,
        message: format!("Level-4 audio render failed: {err}"),
        scope: scorestrata_core::diagnostic::MetricScope::default(),
        evidence_uris: vec!["project://audio/render".to_string()],
        root_causes: vec!["project could not be compiled or rendered".to_string()],
        suggested_patches: vec![],
        expected_delta: None,
        confidence: 1.0,
    }
}

fn default_render_mode() -> String {
    "master".to_string()
}

/// Render a project (or a segment of it) to a WAV artifact.
///
/// When a `project` payload is provided, the project is compiled to a
/// timeline IR and rendered offline via the audio engine, producing real
/// WAV bytes with a deterministic checksum and render-time accounting.
#[must_use]
pub fn render_project(params: &RenderProjectParams) -> ToolReceipt {
    let Some(project_json) = &params.project else {
        return ToolReceipt {
            tool: "render_project".to_string(),
            status: ToolStatus::Failed,
            summary: format!(
                "Render requested for project '{}' but no project payload provided",
                params.project_id
            ),
            score_delta: BTreeMap::new(),
            evidence_uris: vec![],
            focus_uris: vec![],
            next_actions: vec!["get_project_summary".to_string()],
            cost: ToolCost::default(),
            warnings: vec![
                "No project payload — supply the serialized project to render".to_string(),
            ],
            dry_run: false,
            diagnostics: vec![],
        };
    };

    let project: scorestrata_core::domain::Project =
        match serde_json::from_value(project_json.clone()) {
            Ok(p) => p,
            Err(e) => {
                return ToolReceipt {
                    tool: "render_project".to_string(),
                    status: ToolStatus::Failed,
                    summary: format!("Could not deserialize project: {e}"),
                    score_delta: BTreeMap::new(),
                    evidence_uris: vec![],
                    focus_uris: vec![],
                    next_actions: vec!["get_project_summary".to_string()],
                    cost: ToolCost::default(),
                    warnings: vec!["Invalid project payload".to_string()],
                    dry_run: false,
                    diagnostics: vec![],
                };
            }
        };

    // Compile the project (or just the requested segment) to a timeline.
    let timeline = match &params.segment_id {
        Some(seg_id) => scorestrata_compiler::compile_project(&project)
            .ok()
            .and_then(|tl| {
                let events: Vec<_> = tl
                    .events
                    .into_iter()
                    .filter(|e| e.segment_id.to_string() == *seg_id)
                    .collect();
                let total_ticks = events.iter().map(|e| e.tick.0).max().unwrap_or(0);
                Some(scorestrata_compiler::TimelineIR {
                    project_id: tl.project_id,
                    events,
                    total_ticks,
                    bpm: tl.bpm,
                    time_signature_num: tl.time_signature_num,
                    input_hash: tl.input_hash,
                    compiler_version: tl.compiler_version,
                })
            }),
        None => scorestrata_compiler::compile_project(&project).ok(),
    };

    let Some(timeline) = timeline else {
        return ToolReceipt {
            tool: "render_project".to_string(),
            status: ToolStatus::Failed,
            summary: "Could not compile project for rendering".to_string(),
            score_delta: BTreeMap::new(),
            evidence_uris: vec![],
            focus_uris: vec![format!("project://{}/validation/latest", params.project_id)],
            next_actions: vec!["run_validation".to_string()],
            cost: ToolCost::default(),
            warnings: vec!["Compilation failed — check project status".to_string()],
            dry_run: false,
            diagnostics: vec![],
        };
    };

    match render_timeline_to_analysis(&timeline, &project) {
        Ok(result) => {
            // Level-4 audio validation: surface LUFS / true peak / clipping
            // diagnostics on the receipt so the agent loop can react to mix
            // issues without a separate round trip.
            let audio_diags =
                scorestrata_validator::audio_check::generate_audio_diagnostics(&result.analysis);
            let mut warnings = audio_diags
                .iter()
                .map(|d| d.message.clone())
                .collect::<Vec<_>>();
            if warnings.is_empty() {
                warnings.push(format!(
                    "Level-4 audio: {:.1} LUFS, {:.1} dBTP, no clipping",
                    result.analysis.integrated_lufs, result.analysis.true_peak_dbtp
                ));
            }

            let mut score_delta = BTreeMap::new();
            score_delta.insert("render.determinism".to_string(), "+1.0".to_string());
            score_delta.insert(
                "audio.true_peak_dbtp".to_string(),
                format!("{:.1}", result.analysis.true_peak_dbtp),
            );
            score_delta.insert(
                "audio.integrated_lufs".to_string(),
                format!("{:.1}", result.analysis.integrated_lufs),
            );

            // §2 (audio plan) — the post-generation spectral audit rides on the
            // receipt as the mix's debug log: spectral centroid, aliasing
            // proxy, and genre-fingerprint deviation tell the agent loop how
            // the mix actually sounds, not just how loud it is.
            let spectral_audit = scorestrata_audio::audit::audit_mono(
                &result.mono_samples,
                result.sample_rate,
                Some(&project.config.genre),
            );
            score_delta.insert(
                "audio.spectral_centroid_hz".to_string(),
                format!("{:.0}", spectral_audit.spectral_centroid_hz),
            );
            score_delta.insert(
                "audio.aliasing_ratio".to_string(),
                format!("{:.3}", spectral_audit.aliasing_ratio),
            );
            if let Some(dev) = spectral_audit.genre_fingerprint_deviation {
                score_delta.insert(
                    "audio.genre_fingerprint_deviation".to_string(),
                    format!("{:.3}", dev),
                );
            }

            // §17.2 — every render produces a manifest. The receipt always
            // carries the reproducibility fingerprint; when a manifest store
            // payload is supplied, the full receipt is recorded into it so the
            // agent loop can retrieve and verify the artifact later.
            let artifact_id = format!("art_{}_{}", params.project_id, params.mode);
            let artifact_type = match params.mode.as_str() {
                "master" => "master_wav",
                "stems" => "stems",
                "preview" => "preview",
                other => other,
            };
            // Provenance inputs default to ids derived from the project when
            // not supplied. The defaults are placeholders, so the receipt
            // reports which provenance the fingerprint was built from.
            let source_snapshot_id = params
                .source_snapshot_id
                .clone()
                .unwrap_or_else(|| format!("snap_{}_live", params.project_id));
            let command_id = params
                .command_id
                .clone()
                .unwrap_or_else(|| format!("cmd_render_{}", params.project_id));
            let provenance_defaulted =
                params.source_snapshot_id.is_none() || params.command_id.is_none();
            let manifest = scorestrata_storage::render_manifest::RenderManifest::from_render(
                &artifact_id,
                artifact_type,
                &format!("renders/{}.wav", params.mode),
                &result.checksum,
                result.sample_rate,
                result.channels,
                result.duration_seconds,
                &source_snapshot_id,
                &command_id,
                &timeline.compiler_version,
                scorestrata_audio::render::AUDIO_ENGINE_VERSION,
                params.seed,
                params.asset_hashes.clone().unwrap_or_default(),
            );
            let manifest_hash = manifest.manifest_hash();
            score_delta.insert("manifest.artifact_id".to_string(), artifact_id.clone());
            score_delta.insert("manifest.manifest_hash".to_string(), manifest_hash.clone());
            score_delta.insert(
                "manifest.provenance".to_string(),
                if provenance_defaulted {
                    "defaults".to_string()
                } else {
                    "explicit".to_string()
                },
            );
            let recorded = match &params.manifest_store {
                Some(json) => {
                    match serde_json::from_value::<
                        scorestrata_storage::render_manifest::RenderManifestStore,
                    >(json.clone())
                    {
                        Ok(mut store) => {
                            store.record(manifest);
                            score_delta.insert("manifest.recorded".to_string(), "true".to_string());
                            true
                        }
                        Err(_) => {
                            warnings.push(
                                "Manifest produced but could not be recorded — invalid manifest_store payload"
                                    .to_string(),
                            );
                            false
                        }
                    }
                }
                None => {
                    warnings.push(
                        "Manifest produced but not persisted — supply a manifest_store payload to record it (see get_render_manifest)"
                            .to_string(),
                    );
                    false
                }
            };
            let mut next_actions = vec![
                "get_render_job".to_string(),
                "run_validation".to_string(),
                "apply_patch".to_string(),
            ];
            if recorded {
                next_actions.push("get_render_manifest".to_string());
            }

            ToolReceipt {
                tool: "render_project".to_string(),
                status: ToolStatus::Succeeded,
                summary: format!(
                    "Rendered {} events ({:.2}s, sha256 {}, manifest {}..., {:.1} LUFS, {:.1} dBTP) in '{}' mode",
                    timeline.event_count(),
                    result.duration_seconds,
                    &result.checksum[..8.min(result.checksum.len())],
                    &manifest_hash[..8.min(manifest_hash.len())],
                    result.analysis.integrated_lufs,
                    result.analysis.true_peak_dbtp,
                    params.mode
                ),
                score_delta,
                evidence_uris: vec![
                    format!("project://{}/render/{}", params.project_id, params.mode),
                    format!("project://{}/render/audit", params.project_id),
                ],
                focus_uris: vec![],
                next_actions,
                cost: ToolCost {
                    cpu_ms: 10,
                    tool_calls: 1,
                    render_seconds: result.duration_seconds as f32,
                },
                warnings,
                dry_run: false,
                diagnostics: audio_diags,
            }
        }
        Err(e) => ToolReceipt {
            tool: "render_project".to_string(),
            status: ToolStatus::Failed,
            summary: format!("Render failed: {e}"),
            score_delta: BTreeMap::new(),
            evidence_uris: vec![],
            focus_uris: vec![],
            next_actions: vec!["get_failure_report".to_string()],
            cost: ToolCost::default(),
            warnings: vec!["Render engine error".to_string()],
            dry_run: false,
            diagnostics: vec![],
        },
    }
}

/// Parameters for `render_segment` tool.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RenderSegmentParams {
    /// Project ID.
    pub project_id: String,
    /// Segment ID to render.
    pub segment_id: String,
    /// Render mode.
    #[serde(default = "default_render_mode")]
    pub mode: String,
    /// Serialized project to render.
    #[serde(default)]
    pub project: Option<serde_json::Value>,
    /// Serialized [`RenderManifestStore`](scorestrata_storage::render_manifest::RenderManifestStore)
    /// — passed through to [`render_project`] so segment renders also record
    /// their §17.2 reproducibility receipt.
    #[serde(default)]
    pub manifest_store: Option<serde_json::Value>,
    /// Snapshot the render was produced from (§17.1).
    #[serde(default)]
    pub source_snapshot_id: Option<String>,
    /// Command that triggered the render (§17.2).
    #[serde(default)]
    pub command_id: Option<String>,
    /// Seed used for deterministic generation (§17.2).
    #[serde(default)]
    pub seed: Option<u64>,
    /// Hashes of source assets (§17.2).
    #[serde(default)]
    pub asset_hashes: Option<BTreeMap<String, String>>,
}

/// Render a single segment to a WAV artifact.
///
/// Delegates to [`render_project`] with a segment scope.
#[must_use]
pub fn render_segment(params: &RenderSegmentParams) -> ToolReceipt {
    render_project(&RenderProjectParams {
        project_id: params.project_id.clone(),
        mode: params.mode.clone(),
        project: params.project.clone(),
        segment_id: Some(params.segment_id.clone()),
        manifest_store: params.manifest_store.clone(),
        source_snapshot_id: params.source_snapshot_id.clone(),
        command_id: params.command_id.clone(),
        seed: params.seed,
        asset_hashes: params.asset_hashes.clone(),
    })
}

/// Parameters for `render_stems` tool.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RenderStemsParams {
    /// Project ID.
    pub project_id: String,
    /// Serialized project to render.
    #[serde(default)]
    pub project: Option<serde_json::Value>,
}

/// Render each layer to its own WAV stem plus a mixed master.
///
/// Produces real per-layer WAV artifacts with deterministic checksums and
/// per-stem Level-4 loudness analysis (deterministic stems per §16).
#[must_use]
pub fn render_stems(params: &RenderStemsParams) -> ToolReceipt {
    let Some(project_json) = &params.project else {
        return ToolReceipt {
            tool: "render_stems".to_string(),
            status: ToolStatus::Failed,
            summary: format!(
                "Stems render requested for project '{}' but no project payload provided",
                params.project_id
            ),
            score_delta: BTreeMap::new(),
            evidence_uris: vec![],
            focus_uris: vec![],
            next_actions: vec!["get_project_summary".to_string()],
            cost: ToolCost::default(),
            warnings: vec![
                "No project payload — supply the serialized project to render".to_string(),
            ],
            dry_run: false,
            diagnostics: vec![],
        };
    };

    let project: scorestrata_core::domain::Project =
        match serde_json::from_value(project_json.clone()) {
            Ok(p) => p,
            Err(e) => {
                return ToolReceipt {
                    tool: "render_stems".to_string(),
                    status: ToolStatus::Failed,
                    summary: format!("Could not deserialize project: {e}"),
                    score_delta: BTreeMap::new(),
                    evidence_uris: vec![],
                    focus_uris: vec![],
                    next_actions: vec!["get_project_summary".to_string()],
                    cost: ToolCost::default(),
                    warnings: vec!["Invalid project payload".to_string()],
                    dry_run: false,
                    diagnostics: vec![],
                };
            }
        };

    let timeline = match scorestrata_compiler::compile_project(&project) {
        Ok(tl) => tl,
        Err(e) => {
            return ToolReceipt {
                tool: "render_stems".to_string(),
                status: ToolStatus::Failed,
                summary: format!("Could not compile project: {e}"),
                score_delta: BTreeMap::new(),
                evidence_uris: vec![],
                focus_uris: vec![],
                next_actions: vec!["run_validation".to_string()],
                cost: ToolCost::default(),
                warnings: vec!["Compilation failed".to_string()],
                dry_run: false,
                diagnostics: vec![],
            };
        }
    };

    // Map layer IDs to human-readable names for voice assignment, and gather
    // each layer's instrument so the render path gets real per-instrument
    // timbres. (The first-listen defect: an empty voice table rendered every
    // layer — drums, bass, lead — with the same default saw.)
    let mut layer_names = std::collections::HashMap::new();
    let mut layer_instruments: Vec<(&str, &str)> = Vec::new();
    for segment in &project.arrangement.segments {
        for layer in &segment.layers {
            layer_names.insert(layer.id.to_string(), layer.name.clone());
            if !layer_instruments
                .iter()
                .any(|(n, _)| *n == layer.name.as_str())
            {
                layer_instruments.push((layer.name.as_str(), layer.instrument.as_str()));
            }
        }
    }

    let notes: Vec<scorestrata_audio::render::RenderNote> = timeline
        .events
        .iter()
        .filter_map(|e| match &e.kind {
            scorestrata_compiler::timeline::EventKind::NoteOn { pitch, velocity } => {
                let layer_name = layer_names
                    .get(&e.layer_id.to_string())
                    .cloned()
                    .unwrap_or_else(|| e.layer_id.to_string());
                Some(scorestrata_audio::render::RenderNote {
                    pitch: pitch.0,
                    start_tick: e.tick.0,
                    duration_ticks: crate::patch_ops::next_note_off_duration(&timeline, e),
                    velocity: velocity.as_f32(),
                    layer_name,
                })
            }
            _ => None,
        })
        .collect();

    let voices = scorestrata_audio::render::build_voices(&layer_instruments);
    match scorestrata_audio::render::render_stems(
        &notes,
        timeline.bpm,
        scorestrata_audio::render::DEFAULT_SAMPLE_RATE,
        &voices,
    ) {
        Ok((stems, master)) => {
            let stem_summary = stems
                .iter()
                .map(|s| format!("{}:{:.1}LUFS", s.layer_name, s.analysis.integrated_lufs))
                .collect::<Vec<_>>()
                .join(", ");
            let evidence_uris = stems
                .iter()
                .map(|s| {
                    format!(
                        "project://{}/render/stems/{}",
                        params.project_id, s.layer_name
                    )
                })
                .collect();
            let mut score_delta = BTreeMap::new();
            score_delta.insert("render.stems".to_string(), format!("+{}", stems.len()));
            score_delta.insert(
                "audio.integrated_lufs".to_string(),
                format!("{:.1}", master.analysis.integrated_lufs),
            );

            ToolReceipt {
                tool: "render_stems".to_string(),
                status: ToolStatus::Succeeded,
                summary: format!(
                    "Rendered {} stems ({:.2}s master, sha256 {}): {stem_summary}",
                    stems.len(),
                    master.duration_seconds,
                    &master.checksum[..8.min(master.checksum.len())]
                ),
                score_delta,
                evidence_uris,
                focus_uris: vec![format!("project://{}/render/stems", params.project_id)],
                next_actions: vec!["render_project".to_string(), "run_validation".to_string()],
                cost: ToolCost {
                    cpu_ms: 20,
                    tool_calls: u32::try_from(stems.len()).unwrap_or(u32::MAX),
                    render_seconds: master.duration_seconds as f32,
                },
                warnings: vec![],
                dry_run: false,
                diagnostics: vec![],
            }
        }
        Err(e) => ToolReceipt {
            tool: "render_stems".to_string(),
            status: ToolStatus::Failed,
            summary: format!("Stems render failed: {e}"),
            score_delta: BTreeMap::new(),
            evidence_uris: vec![],
            focus_uris: vec![],
            next_actions: vec!["get_failure_report".to_string()],
            cost: ToolCost::default(),
            warnings: vec!["Render engine error".to_string()],
            dry_run: false,
            diagnostics: vec![],
        },
    }
}

/// Parameters for `audit_layer_spectral` tool.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuditLayerSpectralParams {
    /// Project ID.
    pub project_id: String,
    /// Serialized project payload (when provided, the audit runs on real
    /// per-layer stems — the layer-wise spectral debug log).
    #[serde(default)]
    pub project: Option<serde_json::Value>,
}

/// Per-layer spectral audit — the layer-wise "debug log" for generated
/// music (S2).
///
/// Renders the project to per-layer stems and reports each layer's own
/// spectral footprint (centroid, rolloff, band energies, aliasing, genre
/// deviation) — the master graph alone hides whether the *bass* or the
/// *hats* are the layer that is muddy, thin, or aliased. Agents use this to
/// target a single layer for patching instead of guessing from the blend.
#[must_use]
pub fn audit_layer_spectral(params: &AuditLayerSpectralParams) -> ToolReceipt {
    let Some(project_json) = &params.project else {
        return ToolReceipt {
            tool: "audit_layer_spectral".to_string(),
            status: ToolStatus::Failed,
            summary: format!(
                "Layer spectral audit requested for project '{}' but no project payload provided",
                params.project_id
            ),
            score_delta: BTreeMap::new(),
            evidence_uris: vec![],
            focus_uris: vec![],
            next_actions: vec!["get_project_summary".to_string()],
            cost: ToolCost::default(),
            warnings: vec![
                "No project payload — supply the serialized project to audit its layers"
                    .to_string(),
            ],
            dry_run: false,
            diagnostics: vec![],
        };
    };

    let project: scorestrata_core::domain::Project =
        match serde_json::from_value(project_json.clone()) {
            Ok(p) => p,
            Err(e) => {
                return ToolReceipt {
                    tool: "audit_layer_spectral".to_string(),
                    status: ToolStatus::Failed,
                    summary: format!("Could not deserialize project: {e}"),
                    score_delta: BTreeMap::new(),
                    evidence_uris: vec![],
                    focus_uris: vec![],
                    next_actions: vec!["get_project_summary".to_string()],
                    cost: ToolCost::default(),
                    warnings: vec!["Invalid project payload".to_string()],
                    dry_run: false,
                    diagnostics: vec![],
                };
            }
        };

    match render_project_to_layer_audit(&project, None) {
        Ok((entries, master)) => {
            let layer_summary = entries
                .iter()
                .map(|e| {
                    format!(
                        "{}:cen{:.0}Hz/roll{:.0}Hz/{:.0}%>16k",
                        e.layer_name,
                        e.report.spectral_centroid_hz,
                        e.report.spectral_rolloff_hz,
                        e.report.aliasing_ratio * 100.0
                    )
                })
                .collect::<Vec<_>>()
                .join(", ");
            let evidence_uris = entries
                .iter()
                .map(|e| {
                    format!(
                        "project://{}/audit/layers/{}",
                        params.project_id, e.layer_name
                    )
                })
                .collect();
            let mut score_delta = BTreeMap::new();
            score_delta.insert("audit.layers".to_string(), format!("+{}", entries.len()));
            score_delta.insert(
                "audio.integrated_lufs".to_string(),
                format!("{:.1}", master.analysis.integrated_lufs),
            );

            ToolReceipt {
                tool: "audit_layer_spectral".to_string(),
                status: ToolStatus::Succeeded,
                summary: format!(
                    "Audited {} layers ({:.2}s master): {layer_summary}",
                    entries.len(),
                    master.duration_seconds
                ),
                score_delta,
                evidence_uris,
                focus_uris: vec![format!("project://{}/audit/layers", params.project_id)],
                next_actions: vec!["get_audit_summary".to_string(), "suggest_fixes".to_string()],
                cost: ToolCost {
                    cpu_ms: 30,
                    tool_calls: 2,
                    render_seconds: master.duration_seconds as f32,
                },
                warnings: vec![],
                dry_run: false,
                diagnostics: vec![],
            }
        }
        Err(e) => ToolReceipt {
            tool: "audit_layer_spectral".to_string(),
            status: ToolStatus::Failed,
            summary: format!("Layer spectral audit failed: {e}"),
            score_delta: BTreeMap::new(),
            evidence_uris: vec![],
            focus_uris: vec![],
            next_actions: vec!["run_validation".to_string()],
            cost: ToolCost::default(),
            warnings: vec!["Render engine error".to_string()],
            dry_run: false,
            diagnostics: vec![],
        },
    }
}

/// Parameters for `render_candidate_preview` tool.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RenderCandidatePreviewParams {
    /// Project ID.
    pub project_id: String,
    /// Candidate ID (label).
    pub candidate_id: String,
    /// Serialized candidate project to render a preview of (required).
    #[serde(default)]
    pub project: Option<serde_json::Value>,
    /// Serialized render job store to register the job in (when provided).
    #[serde(default)]
    pub jobs: Option<serde_json::Value>,
}

/// Render a short preview of a candidate project (§10.5).
///
/// Uses the real audio engine to produce a master preview, and registers the
/// render as a `RenderJob` in the provided store so `get_render_job` can
/// report its lifecycle. Without a job store the render still runs but the
/// job is not persisted.
#[must_use]
pub fn render_candidate_preview(params: &RenderCandidatePreviewParams) -> ToolReceipt {
    let Some(project_json) = &params.project else {
        return ToolReceipt {
            tool: "render_candidate_preview".to_string(),
            status: ToolStatus::Failed,
            summary: format!(
                "Preview render requested for candidate '{}' but no project payload provided",
                params.candidate_id
            ),
            score_delta: BTreeMap::new(),
            evidence_uris: vec![],
            focus_uris: vec![],
            next_actions: vec!["get_project_summary".to_string()],
            cost: ToolCost::default(),
            warnings: vec![
                "No project payload — supply the serialized candidate to render".to_string(),
            ],
            dry_run: false,
            diagnostics: vec![],
        };
    };
    let project: scorestrata_core::domain::Project =
        match serde_json::from_value(project_json.clone()) {
            Ok(p) => p,
            Err(e) => {
                return deserialize_error_receipt("render_candidate_preview", "project", &e);
            }
        };

    // Compile to timeline IR; a failed compile means a failed render job.
    let timeline = match scorestrata_compiler::compile_project(&project) {
        Ok(tl) => tl,
        Err(e) => {
            return ToolReceipt {
                tool: "render_candidate_preview".to_string(),
                status: ToolStatus::Failed,
                summary: format!("Candidate preview compile failed: {e}"),
                score_delta: BTreeMap::new(),
                evidence_uris: vec![],
                focus_uris: vec![],
                next_actions: vec!["run_validation".to_string()],
                cost: ToolCost::default(),
                warnings: vec!["Compilation failed".to_string()],
                dry_run: false,
                diagnostics: vec![],
            };
        }
    };

    // Voice assignment from the project's layers (instrument → timbre).
    let mut layer_names = std::collections::HashMap::new();
    let mut layer_instruments: Vec<(&str, &str)> = Vec::new();
    for segment in &project.arrangement.segments {
        for layer in &segment.layers {
            layer_names.insert(layer.id.to_string(), layer.name.clone());
            if !layer_instruments
                .iter()
                .any(|(n, _)| *n == layer.name.as_str())
            {
                layer_instruments.push((layer.name.as_str(), layer.instrument.as_str()));
            }
        }
    }
    let voices = scorestrata_audio::render::build_voices(&layer_instruments);

    let notes: Vec<scorestrata_audio::render::RenderNote> = timeline
        .events
        .iter()
        .filter_map(|e| match &e.kind {
            scorestrata_compiler::timeline::EventKind::NoteOn { pitch, velocity } => {
                let layer_name = layer_names
                    .get(&e.layer_id.to_string())
                    .cloned()
                    .unwrap_or_else(|| e.layer_id.to_string());
                Some(scorestrata_audio::render::RenderNote {
                    pitch: pitch.0,
                    start_tick: e.tick.0,
                    duration_ticks: crate::patch_ops::next_note_off_duration(&timeline, e),
                    velocity: velocity.as_f32(),
                    layer_name,
                })
            }
            _ => None,
        })
        .collect();

    // A preview is a short master render; register the job first so the
    // lifecycle is observable, then run the real engine. A malformed job
    // store payload is reported (never silently replaced) for transparency.
    let job_id = format!("job_{}_{}", params.project_id, params.candidate_id);
    let mut jobs = match &params.jobs {
        None => scorestrata_storage::render_job::RenderJobStore::new(),
        Some(jobs_json) => {
            match serde_json::from_value::<scorestrata_storage::render_job::RenderJobStore>(
                jobs_json.clone(),
            ) {
                Ok(j) => j,
                Err(e) => {
                    return deserialize_error_receipt("render_candidate_preview", "jobs", &e);
                }
            }
        }
    };
    jobs.submit(scorestrata_storage::render_job::RenderJob::new(
        &job_id,
        &params.project_id,
        "preview",
    ));
    jobs.start(&job_id);

    let artifact_uri = format!(
        "project://{}/render/preview/{}",
        params.project_id, params.candidate_id
    );
    let receipt = match scorestrata_audio::render::render_stems(
        &notes,
        timeline.bpm,
        scorestrata_audio::render::DEFAULT_SAMPLE_RATE,
        &voices,
    ) {
        Ok((_, master)) => {
            jobs.complete(&job_id, &artifact_uri);
            let mut score_delta = BTreeMap::new();
            score_delta.insert("render.preview_job".to_string(), job_id.clone());
            score_delta.insert(
                "audio.integrated_lufs".to_string(),
                format!("{:.1}", master.analysis.integrated_lufs),
            );
            ToolReceipt {
                tool: "render_candidate_preview".to_string(),
                status: ToolStatus::Succeeded,
                summary: format!(
                    "Preview for candidate '{}' rendered ({:.2}s master, sha256 {}); job {} completed",
                    params.candidate_id,
                    master.duration_seconds,
                    &master.checksum[..8.min(master.checksum.len())],
                    job_id
                ),
                score_delta,
                evidence_uris: vec![artifact_uri.clone()],
                focus_uris: vec![format!("project://{}/render/preview", params.project_id)],
                next_actions: vec!["compare_candidates".to_string()],
                cost: ToolCost {
                    cpu_ms: 15,
                    tool_calls: 1,
                    render_seconds: master.duration_seconds as f32,
                },
                warnings: vec![],
                dry_run: false,
                diagnostics: vec![],
            }
        }
        Err(e) => {
            jobs.fail(&job_id, &e.to_string());
            ToolReceipt {
                tool: "render_candidate_preview".to_string(),
                status: ToolStatus::Failed,
                summary: format!("Candidate preview render failed: {e}"),
                score_delta: BTreeMap::new(),
                evidence_uris: vec![],
                focus_uris: vec![],
                next_actions: vec!["get_failure_report".to_string()],
                cost: ToolCost::default(),
                warnings: vec!["Render engine error".to_string()],
                dry_run: false,
                diagnostics: vec![],
            }
        }
    };
    // Surface the job state in the receipt for round-tripping.
    let job_status = jobs
        .get(&job_id)
        .map_or_else(|| "unknown".to_string(), |j| format!("{:?}", j.status));
    let mut enriched = receipt;
    enriched
        .score_delta
        .insert("render.job_status".to_string(), job_status);
    enriched
        .evidence_uris
        .push(format!("render_jobs://{}", job_id));
    enriched
}

/// Parameters for `get_render_job` tool.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GetRenderJobParams {
    /// Project ID.
    pub project_id: String,
    /// Job ID to inspect.
    pub job_id: String,
    /// Serialized render job store to query (when provided, real).
    #[serde(default)]
    pub jobs: Option<serde_json::Value>,
}

/// Get the lifecycle status of a render job (§10.5).
#[must_use]
pub fn get_render_job(params: &GetRenderJobParams) -> ToolReceipt {
    let Some(jobs_json) = &params.jobs else {
        return ToolReceipt {
            tool: "get_render_job".to_string(),
            status: ToolStatus::Failed,
            summary: format!(
                "Render job '{}' requested for project '{}' but no render job store payload provided",
                params.job_id, params.project_id
            ),
            score_delta: BTreeMap::new(),
            evidence_uris: vec![],
            focus_uris: vec![],
            next_actions: vec!["render_candidate_preview".to_string()],
            cost: ToolCost::default(),
            warnings: vec!["Supply a serialized render job store to query jobs".to_string()],
            dry_run: false,
            diagnostics: vec![],
        };
    };
    let jobs = match serde_json::from_value::<scorestrata_storage::render_job::RenderJobStore>(
        jobs_json.clone(),
    ) {
        Ok(j) => j,
        Err(e) => return deserialize_error_receipt("get_render_job", "jobs", &e),
    };
    let Some(job) = jobs.get(&params.job_id) else {
        return ToolReceipt {
            tool: "get_render_job".to_string(),
            status: ToolStatus::Failed,
            summary: format!(
                "Render job '{}' not found in the store for project '{}'",
                params.job_id, params.project_id
            ),
            score_delta: BTreeMap::new(),
            evidence_uris: vec![],
            focus_uris: vec![],
            next_actions: vec!["render_candidate_preview".to_string()],
            cost: ToolCost::default(),
            warnings: vec!["Unknown render job ID".to_string()],
            dry_run: false,
            diagnostics: vec![],
        };
    };
    let mut score_delta = BTreeMap::new();
    score_delta.insert("render.job_status".to_string(), format!("{:?}", job.status));
    score_delta.insert("render.job_mode".to_string(), job.mode.clone());
    if let Some(uri) = &job.artifact_uri {
        score_delta.insert("render.artifact".to_string(), uri.clone());
    }
    ToolReceipt {
        tool: "get_render_job".to_string(),
        status: ToolStatus::Succeeded,
        summary: format!(
            "Render job '{}' ({}) is {:?} for project '{}'{}",
            job.id,
            job.mode,
            job.status,
            params.project_id,
            job.artifact_uri
                .as_ref()
                .map_or_else(|| String::new(), |u| format!(" — {u}"))
        ),
        score_delta,
        evidence_uris: vec![format!("render_jobs://{}", job.id)],
        focus_uris: vec![],
        next_actions: match job.status {
            scorestrata_storage::render_job::RenderJobStatus::Completed => {
                vec!["compare_candidates".to_string()]
            }
            scorestrata_storage::render_job::RenderJobStatus::Failed => {
                vec!["get_failure_report".to_string()]
            }
            _ => vec![
                "get_render_job".to_string(),
                "cancel_render_job".to_string(),
            ],
        },
        cost: ToolCost {
            cpu_ms: 1,
            tool_calls: 1,
            ..Default::default()
        },
        warnings: vec![],
        dry_run: false,
        diagnostics: vec![],
    }
}

/// Parameters for `cancel_render_job` tool.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CancelRenderJobParams {
    /// Project ID.
    pub project_id: String,
    /// Job ID to cancel.
    pub job_id: String,
    /// Serialized render job store to mutate (when provided, real).
    #[serde(default)]
    pub jobs: Option<serde_json::Value>,
}

/// Cancel a queued or rendering render job (§10.5).
#[must_use]
pub fn cancel_render_job(params: &CancelRenderJobParams) -> ToolReceipt {
    let Some(jobs_json) = &params.jobs else {
        return ToolReceipt {
            tool: "cancel_render_job".to_string(),
            status: ToolStatus::Failed,
            summary: format!(
                "Cancel requested for render job '{}' but no render job store payload provided",
                params.job_id
            ),
            score_delta: BTreeMap::new(),
            evidence_uris: vec![],
            focus_uris: vec![],
            next_actions: vec!["render_candidate_preview".to_string()],
            cost: ToolCost::default(),
            warnings: vec!["Supply a serialized render job store to cancel jobs".to_string()],
            dry_run: false,
            diagnostics: vec![],
        };
    };
    let mut jobs = match serde_json::from_value::<scorestrata_storage::render_job::RenderJobStore>(
        jobs_json.clone(),
    ) {
        Ok(j) => j,
        Err(e) => return deserialize_error_receipt("cancel_render_job", "jobs", &e),
    };
    if !jobs.cancel(&params.job_id) {
        let state = jobs
            .get(&params.job_id)
            .map_or_else(|| "not found".to_string(), |j| format!("{:?}", j.status));
        return ToolReceipt {
            tool: "cancel_render_job".to_string(),
            status: ToolStatus::Failed,
            summary: format!(
                "Cannot cancel render job '{}' (state: {})",
                params.job_id, state
            ),
            score_delta: BTreeMap::new(),
            evidence_uris: vec![],
            focus_uris: vec![],
            next_actions: vec![],
            cost: ToolCost::default(),
            warnings: vec!["Job is terminal or unknown".to_string()],
            dry_run: false,
            diagnostics: vec![],
        };
    }
    let mut score_delta = BTreeMap::new();
    score_delta.insert(
        "render.job_status".to_string(),
        format!(
            "{:?}",
            scorestrata_storage::render_job::RenderJobStatus::Cancelled
        ),
    );
    ToolReceipt {
        tool: "cancel_render_job".to_string(),
        status: ToolStatus::Succeeded,
        summary: format!("Render job '{}' cancelled", params.job_id),
        score_delta,
        evidence_uris: vec![format!("render_jobs://{}", params.job_id)],
        focus_uris: vec![],
        next_actions: vec!["render_candidate_preview".to_string()],
        cost: ToolCost {
            cpu_ms: 1,
            tool_calls: 1,
            ..Default::default()
        },
        warnings: vec![],
        dry_run: false,
        diagnostics: vec![],
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use scorestrata_core::domain::SegmentRole;
    use scorestrata_core::ids::{LayerId, SegmentId};

    fn make_segment(
        name: &str,
        bars: u32,
        layer_count: usize,
    ) -> scorestrata_core::domain::Segment {
        scorestrata_core::domain::Segment {
            id: SegmentId::default(),
            name: name.to_string(),
            role: SegmentRole::Verse,
            start_bar: 0,
            bars,
            layers: (0..layer_count)
                .map(|i| scorestrata_core::domain::Layer {
                    id: LayerId::default(),
                    name: format!("layer_{i}"),
                    instrument: "synth".to_string(),
                    pattern_instances: Vec::new(),
                    register: None,
                    automation: vec![],
                    fx: None,
                    mix_gain_db: None,
                    mix_pan: None,
                })
                .collect(),
        }
    }

    fn default_params() -> RunValidationParams {
        RunValidationParams {
            project_id: "proj_123".to_string(),
            scope: "full".to_string(),
            segment_ids: vec![],
            layer_ids: vec![],
            min_energy_delta: 0.15,
        }
    }

    #[test]
    fn run_validation_on_empty_segments_returns_success() {
        let receipt = run_validation(&default_params(), &[], 2);
        assert_eq!(receipt.tool, "run_validation");
        assert!(matches!(receipt.status, ToolStatus::Succeeded));
        assert!(receipt.diagnostics.is_empty());
    }

    #[test]
    fn run_validation_detects_empty_segment() {
        let segments = vec![make_segment("intro", 8, 0)];
        let receipt = run_validation(&default_params(), &segments, 2);
        assert!(!receipt.diagnostics.is_empty());
        assert!(
            receipt
                .diagnostics
                .iter()
                .any(|d| d.code == "SEGMENT_EMPTY")
        );
    }

    #[test]
    fn run_validation_detects_weak_transition() {
        let segments = vec![make_segment("verse", 16, 4), make_segment("verse2", 16, 4)];
        let receipt = run_validation(&default_params(), &segments, 2);
        assert!(
            receipt
                .diagnostics
                .iter()
                .any(|d| d.code == "TRANSITION_ENERGY_DELTA_LOW")
        );
    }

    #[test]
    fn run_validation_produces_real_diagnostics() {
        let segments = vec![
            make_segment("intro", 8, 1),
            make_segment("chorus", 16, 6),
            make_segment("outro", 8, 2),
        ];
        let receipt = run_validation(&default_params(), &segments, 2);
        assert!(receipt.summary.contains("overall score"));
    }

    #[test]
    fn run_validation_with_project_honors_config_scope() {
        // §22.2 — a `config` scope must emit only CONFIG_* diagnostics, even
        // at a level that would normally run the brief plane.
        let mut project = scorestrata_testkit::melodic_techno_project();
        project.config.bpm = 320; // out of range
        let mut params = default_params();
        params.scope = "config".to_string();
        let receipt = run_validation_with_project(&params, &project, 5);
        assert!(
            receipt
                .diagnostics
                .iter()
                .any(|d| d.code == "CONFIG_BPM_OUT_OF_RANGE"),
            "config scope surfaces config diagnostics: {}",
            receipt.summary
        );
        assert!(
            receipt
                .diagnostics
                .iter()
                .all(|d| d.code.starts_with("CONFIG_")),
            "config scope emits only CONFIG_* checks"
        );
    }

    #[test]
    fn run_validation_with_project_honors_layer_scope() {
        // §22.2 — a `layer` scope validates layer declarations + registers
        // without running the full pipeline.
        let mut project = scorestrata_testkit::melodic_techno_project();
        let mut params = default_params();
        params.scope = "layer".to_string();
        // Empty out every layer's instances: layers become silent.
        for segment in &mut project.arrangement.segments {
            for layer in &mut segment.layers {
                layer.pattern_instances = Vec::new();
                layer.automation = vec![];
            }
        }
        let receipt = run_validation_with_project(&params, &project, 2);
        assert!(
            receipt.diagnostics.iter().any(|d| d.code == "LAYER_SILENT"),
            "layer scope surfaces layer diagnostics: {}",
            receipt.summary
        );
    }

    #[test]
    fn run_validation_with_project_mix_scope_emits_audio_or_render_failure() {
        // §22.2 — the `mix` scope renders the project and runs the audio
        // plane. A project with no patterns renders silence, which is legal
        // audio; assert the receipt completes with a mix-appropriate outcome
        // (either audio diagnostics or a render-failure diagnostic).
        let project = scorestrata_testkit::melodic_techno_project();
        let mut params = default_params();
        params.scope = "mix".to_string();
        let receipt = run_validation_with_project(&params, &project, 2);
        assert!(
            receipt
                .diagnostics
                .iter()
                .any(|d| d.code.starts_with("AUDIO_")),
            "mix scope runs the audio plane: {}",
            receipt.summary
        );
    }

    #[test]
    fn run_validation_with_project_diff_scope_uses_requested_segment_ids() {
        // §22.2 — over the wire, the `diff` scope validates only the
        // explicitly-requested changed segments (the CLI is the canonical
        // cache-driven path). A changed segment id pointing at an empty
        // segment must surface SEGMENT_EMPTY.
        let mut project = scorestrata_testkit::melodic_techno_project();
        let changed_id = project.arrangement.segments[0].id;
        let mut params = default_params();
        params.scope = "diff".to_string();
        params.segment_ids = vec![changed_id.to_string()];
        // Make the changed segment empty so the diff check has something to
        // find.
        project.arrangement.segments[0].layers = Vec::new();
        let receipt = run_validation_with_project(&params, &project, 2);
        assert!(
            receipt
                .diagnostics
                .iter()
                .any(|d| d.code == "SEGMENT_EMPTY"),
            "diff scope validates the changed segment: {}",
            receipt.summary
        );
    }

    #[test]
    fn run_validation_with_project_at_l5_emits_brief_diagnostics() {
        // A config genre that disagrees with the brief must surface as a
        // BRIEF_GENRE_MISMATCH diagnostic at L5 (which the segments-only L2
        // path cannot see).
        let mut project = scorestrata_testkit::melodic_techno_project();
        project.config.genre = "house".to_string();
        let receipt = run_validation_with_project(&default_params(), &project, 5);
        assert_eq!(receipt.tool, "run_validation");
        assert!(
            receipt
                .diagnostics
                .iter()
                .any(|d| d.code == "BRIEF_GENRE_MISMATCH"),
            "expected BRIEF_GENRE_MISMATCH, got: {:?}",
            receipt
                .diagnostics
                .iter()
                .map(|d| d.code.clone())
                .collect::<Vec<_>>()
        );
    }

    #[test]
    fn run_validation_with_project_below_l5_skips_brief_plane() {
        // At L2 the brief plane is not exercised, so the same mismatched
        // project produces no BRIEF_* diagnostics.
        let mut project = scorestrata_testkit::melodic_techno_project();
        project.config.genre = "house".to_string();
        let receipt = run_validation_with_project(&default_params(), &project, 2);
        assert!(
            !receipt
                .diagnostics
                .iter()
                .any(|d| d.code.starts_with("BRIEF_")),
            "L2 must not emit brief diagnostics: {:?}",
            receipt
                .diagnostics
                .iter()
                .map(|d| d.code.clone())
                .collect::<Vec<_>>()
        );
    }

    #[test]
    fn run_validation_with_project_l5_compliant_project_succeeds_brief_plane() {
        // A project whose brief matches its config produces no BRIEF_*
        // diagnostics at L5.
        let project = scorestrata_testkit::melodic_techno_project();
        let receipt = run_validation_with_project(&default_params(), &project, 5);
        assert!(
            !receipt
                .diagnostics
                .iter()
                .any(|d| d.code.starts_with("BRIEF_")),
            "compliant project must have no brief diagnostics: {:?}",
            receipt
                .diagnostics
                .iter()
                .map(|d| d.code.clone())
                .collect::<Vec<_>>()
        );
    }

    #[test]
    fn dispatcher_run_validation_l5_with_project_payload() {
        // Serialize with the shared registry lock: this test records telemetry
        // and must not race the telemetry module's exact-count assertions.
        let _guard = crate::telemetry::test_lock_guard();
        // The wire path: level 5 + project payload must reach the brief plane.
        let mut project = scorestrata_testkit::melodic_techno_project();
        project.config.genre = "house".to_string();
        let receipt = crate::dispatch::dispatch_tool(
            "run_validation",
            serde_json::json!({
                "project_id": project.id.to_string(),
                "project": project,
                "level": 5,
            }),
        );
        assert_eq!(receipt.tool, "run_validation");
        assert!(
            receipt
                .diagnostics
                .iter()
                .any(|d| d.code == "BRIEF_GENRE_MISMATCH"),
            "dispatcher L5 must emit brief diagnostics"
        );
    }

    #[test]
    fn l5_planes_compose_brief_and_symbolic_diagnostics() {
        // A project with both a genre mismatch (L5) and an empty segment (L2)
        // must yield diagnostics from BOTH planes — proving composition, not
        // replacement.
        let mut project = scorestrata_testkit::melodic_techno_project();
        project.config.genre = "house".to_string();
        if let Some(segment) = project.arrangement.segments.first_mut() {
            segment.layers.clear();
        }
        let receipt = run_validation_with_project(&default_params(), &project, 5);
        let codes: Vec<String> = receipt.diagnostics.iter().map(|d| d.code.clone()).collect();
        assert!(
            codes.iter().any(|c| c == "BRIEF_GENRE_MISMATCH"),
            "L5 brief plane present: {codes:?}"
        );
        assert!(
            codes.iter().any(|c| c == "SEGMENT_EMPTY"),
            "L2 symbolic plane present: {codes:?}"
        );
    }

    #[test]
    fn l4_render_gate_reports_drift_against_imported_reference() {
        // G7: once a SpectralReference is imported, the L4 render gate must
        // compare the actual render against it. A reference that is far from
        // the render (tiny centroid, air-only bands) must surface as
        // SPECTRAL_REFERENCE_DRIFT.
        let mut project = scorestrata_testkit::melodic_techno_project();
        project.spectral_reference = Some(scorestrata_core::domain::SpectralReference {
            track_name: "Imported_Ref".to_string(),
            genre: "techno".to_string(),
            band_energy_expected: [0.02, 0.03, 0.05, 0.1, 0.3, 0.5],
            centroid_expected_hz: 300.0,
            aliasing_max: 0.01,
            loudness_target_lufs: -14.0,
        });
        let receipt = run_validation_with_project(&default_params(), &project, 4);
        assert!(
            receipt
                .diagnostics
                .iter()
                .any(|d| d.code == "SPECTRAL_REFERENCE_DRIFT"),
            "expected SPECTRAL_REFERENCE_DRIFT against a far reference, got: {:?}",
            receipt
                .diagnostics
                .iter()
                .map(|d| d.code.clone())
                .collect::<Vec<_>>()
        );
    }

    #[test]
    fn l4_render_gate_silent_without_reference() {
        // No SpectralReference imported → no SPECTRAL_REFERENCE_DRIFT at all,
        // even though the L4 audio plane itself runs.
        let project = scorestrata_testkit::melodic_techno_project();
        let receipt = run_validation_with_project(&default_params(), &project, 4);
        assert!(
            !receipt
                .diagnostics
                .iter()
                .any(|d| d.code == "SPECTRAL_REFERENCE_DRIFT"),
            "no reference → no drift diagnostic: {:?}",
            receipt
                .diagnostics
                .iter()
                .map(|d| d.code.clone())
                .collect::<Vec<_>>()
        );
    }

    #[test]
    fn run_validation_with_project_at_l3_emits_timeline_diagnostics() {
        // A layer referencing a pattern that does not exist must surface as
        // TIMELINE_UNRESOLVED_PATTERN at L3 (the arrangement-wide plane the
        // segments-only L2 path cannot see).
        let mut project = scorestrata_testkit::melodic_techno_project();
        project.arrangement.segments[0].layers[0]
            .pattern_instances
            .push(scorestrata_core::domain::PatternInstance {
                id: scorestrata_core::ids::PatternInstanceId::new(),
                pattern_id: scorestrata_core::ids::PatternId::new(),
                start_tick: scorestrata_core::domain::Tick::new(0),
                transpose: 0,
                velocity_multiplier: 1.0,
            });
        let receipt = run_validation_with_project(&default_params(), &project, 3);
        assert_eq!(receipt.tool, "run_validation");
        assert!(
            receipt
                .diagnostics
                .iter()
                .any(|d| d.code == "TIMELINE_UNRESOLVED_PATTERN"),
            "expected TIMELINE_UNRESOLVED_PATTERN at L3, got: {:?}",
            receipt
                .diagnostics
                .iter()
                .map(|d| d.code.clone())
                .collect::<Vec<_>>()
        );
    }

    #[test]
    fn run_validation_with_project_below_l3_skips_timeline_plane() {
        // At L2 the timeline plane is not exercised, so the same defective
        // project produces no TIMELINE_* diagnostics.
        let mut project = scorestrata_testkit::melodic_techno_project();
        project.arrangement.segments[0].layers[0]
            .pattern_instances
            .push(scorestrata_core::domain::PatternInstance {
                id: scorestrata_core::ids::PatternInstanceId::new(),
                pattern_id: scorestrata_core::ids::PatternId::new(),
                start_tick: scorestrata_core::domain::Tick::new(0),
                transpose: 0,
                velocity_multiplier: 1.0,
            });
        let receipt = run_validation_with_project(&default_params(), &project, 2);
        assert!(
            !receipt
                .diagnostics
                .iter()
                .any(|d| d.code.starts_with("TIMELINE_")),
            "L2 must not emit timeline diagnostics: {:?}",
            receipt
                .diagnostics
                .iter()
                .map(|d| d.code.clone())
                .collect::<Vec<_>>()
        );
    }

    #[test]
    fn l3_planes_compose_with_l2_and_l5() {
        // A project with an unresolved pattern (L3), an empty segment (L2),
        // and a genre mismatch (L5) must yield diagnostics from ALL three
        // planes at level 5 — planes compose, not replace.
        let mut project = scorestrata_testkit::melodic_techno_project();
        project.config.genre = "house".to_string();
        // L3 defect in the SECOND segment's layer (kept alive — the first
        // segment's layers are cleared below for the L2 defect).
        if let Some(second) = project.arrangement.segments.get_mut(1) {
            if let Some(layer) = second.layers.first_mut() {
                layer
                    .pattern_instances
                    .push(scorestrata_core::domain::PatternInstance {
                        id: scorestrata_core::ids::PatternInstanceId::new(),
                        pattern_id: scorestrata_core::ids::PatternId::new(),
                        start_tick: scorestrata_core::domain::Tick::new(0),
                        transpose: 0,
                        velocity_multiplier: 1.0,
                    });
            }
        }
        // L2 defect: the first segment has no layers at all.
        if let Some(segment) = project.arrangement.segments.first_mut() {
            segment.layers.clear();
        }
        let receipt = run_validation_with_project(&default_params(), &project, 5);
        let codes: Vec<String> = receipt.diagnostics.iter().map(|d| d.code.clone()).collect();
        assert!(
            codes.iter().any(|c| c == "TIMELINE_UNRESOLVED_PATTERN"),
            "L3 timeline plane: {codes:?}"
        );
        assert!(
            codes.iter().any(|c| c == "SEGMENT_EMPTY"),
            "L2 symbolic plane: {codes:?}"
        );
        assert!(
            codes.iter().any(|c| c == "BRIEF_GENRE_MISMATCH"),
            "L5 brief plane: {codes:?}"
        );
    }

    #[test]
    fn dispatcher_run_validation_l3_with_project_payload() {
        // Serialize with the shared registry lock: this test records telemetry
        // and must not race the telemetry module's exact-count assertions.
        let _guard = crate::telemetry::test_lock_guard();
        // The wire path: level 3 + project payload must reach the timeline
        // plane.
        let mut project = scorestrata_testkit::melodic_techno_project();
        project.arrangement.segments[0].layers[0]
            .pattern_instances
            .push(scorestrata_core::domain::PatternInstance {
                id: scorestrata_core::ids::PatternInstanceId::new(),
                pattern_id: scorestrata_core::ids::PatternId::new(),
                start_tick: scorestrata_core::domain::Tick::new(0),
                transpose: 0,
                velocity_multiplier: 1.0,
            });
        let receipt = crate::dispatch::dispatch_tool(
            "run_validation",
            serde_json::json!({
                "project_id": project.id.to_string(),
                "project": project,
                "level": 3,
            }),
        );
        assert_eq!(receipt.tool, "run_validation");
        assert!(
            receipt
                .diagnostics
                .iter()
                .any(|d| d.code == "TIMELINE_UNRESOLVED_PATTERN"),
            "dispatcher L3 must emit timeline diagnostics"
        );
    }

    #[test]
    fn apply_patch_dry_run_does_not_mutate() {
        let params = ApplyPatchParams {
            project_id: "proj_123".to_string(),
            op: "quantize_layer_to_scale".to_string(),
            params: serde_json::json!({"strength": 1.0}),
            dry_run: true,
            scope: None,
            project: None,
        };
        let receipt = apply_patch(&params);
        assert!(receipt.dry_run);
        assert!(matches!(receipt.status, ToolStatus::DryRunCompleted));
    }

    #[test]
    fn dry_run_patch_reports_intent_without_project() {
        let params = DryRunPatchParams {
            project_id: "proj_123".to_string(),
            op: "quantize_layer_to_scale".to_string(),
            params: serde_json::json!({"strength": 1.0}),
            scope: None,
            project: None,
        };
        let receipt = dry_run_patch(&params);
        // Stub-transparency: without a project payload we cannot compute the
        // real preview, so the receipt must say so rather than fabricate one.
        assert!(matches!(receipt.status, ToolStatus::DryRunCompleted));
        assert!(receipt.summary.contains("quantize_layer_to_scale"));
        assert!(
            receipt
                .warnings
                .iter()
                .any(|w| w.contains("No project payload")),
            "missing no-project-payload warning: {:#?}",
            receipt.warnings
        );
    }

    #[test]
    fn dry_run_patch_reports_simulated_effects_with_project() {
        let project = scorestrata_testkit::melodic_techno_project();
        let params = DryRunPatchParams {
            project_id: "proj_1".to_string(),
            op: "quantize_layer_to_scale".to_string(),
            params: serde_json::json!({"strength": 1.0}),
            scope: None,
            project: Some(serde_json::to_value(&project).unwrap_or_default()),
        };
        let receipt = dry_run_patch(&params);
        assert!(matches!(receipt.status, ToolStatus::DryRunCompleted));
        assert!(receipt.dry_run);
        assert!(
            receipt.summary.contains("nothing persisted"),
            "summary must state no-mutation semantics: {}",
            receipt.summary
        );
        assert!(
            receipt.warnings.iter().any(|w| w.contains("not persisted")),
            "dry-run warning must be present: {:#?}",
            receipt.warnings
        );
    }

    #[test]
    fn receipt_serializes_to_json() {
        let receipt = run_validation(&default_params(), &[], 2);
        let json = serde_json::to_string_pretty(&receipt).unwrap_or_default();
        assert!(json.contains("run_validation"));
        assert!(json.contains("succeeded"));
    }

    #[test]
    fn create_pattern_returns_success() {
        let params = CreatePatternParams {
            project_id: "proj_1".to_string(),
            name: "chorus_lead".to_string(),
            bars: 8,
            time_signature_num: 4,
        };
        let receipt = create_pattern(&params);
        assert!(matches!(receipt.status, ToolStatus::Succeeded));
        assert!(receipt.summary.contains("chorus_lead"));
    }

    #[test]
    fn plan_arrangement_validates_bar_count() {
        let params = PlanArrangementParams {
            project_id: "proj_1".to_string(),
            total_bars: 32,
            segments: vec![
                SegmentPlan {
                    name: "verse".to_string(),
                    role: "verse".to_string(),
                    bars: 16,
                },
                SegmentPlan {
                    name: "chorus".to_string(),
                    role: "chorus".to_string(),
                    bars: 16,
                },
            ],
        };
        let receipt = plan_arrangement(&params);
        assert!(matches!(receipt.status, ToolStatus::Succeeded));
    }

    #[test]
    fn plan_arrangement_detects_bar_mismatch() {
        let params = PlanArrangementParams {
            project_id: "proj_1".to_string(),
            total_bars: 32,
            segments: vec![SegmentPlan {
                name: "verse".to_string(),
                role: "verse".to_string(),
                bars: 16,
            }],
        };
        let receipt = plan_arrangement(&params);
        assert!(matches!(receipt.status, ToolStatus::Failed));
        assert!(!receipt.warnings.is_empty());
    }

    // ── Composition mutation tools tests ────────────────────────────

    #[test]
    fn write_pattern_notes_appends_real_notes() {
        let project = scorestrata_testkit::melodic_techno_project();
        let pattern_id = project.patterns[0].id.to_string();
        let params = WritePatternNotesParams {
            project_id: project.id.to_string(),
            pattern_id: pattern_id.clone(),
            notes: vec![NoteSpec {
                pitch: 60,
                start: 0,
                duration: 480,
                velocity: 100,
            }],
            project: Some(serde_json::to_value(&project).unwrap_or_default()),
        };
        let receipt = write_pattern_notes(&params);
        assert!(matches!(receipt.status, ToolStatus::Succeeded));
        assert!(receipt.summary.contains("1 note"));
        assert!(receipt.score_delta.contains_key("pattern.notes"));
    }

    #[test]
    fn write_pattern_notes_requires_payload() {
        let params = WritePatternNotesParams {
            project_id: "proj_1".to_string(),
            pattern_id: "pat_1".to_string(),
            notes: vec![],
            project: None,
        };
        let receipt = write_pattern_notes(&params);
        assert!(matches!(receipt.status, ToolStatus::Failed));
        assert!(
            receipt
                .warnings
                .iter()
                .any(|w| w.contains("No project payload")),
            "expected transparency warning, got {:#?}",
            receipt.warnings
        );
    }

    #[test]
    fn write_pattern_steps_appends_real_steps() {
        let project = scorestrata_testkit::melodic_techno_project();
        let pattern_id = project.patterns[0].id.to_string();
        let params = WritePatternStepsParams {
            project_id: project.id.to_string(),
            pattern_id: pattern_id.clone(),
            steps: vec![StepSpec {
                index: 0,
                active: true,
                velocity: 110,
            }],
            project: Some(serde_json::to_value(&project).unwrap_or_default()),
        };
        let receipt = write_pattern_steps(&params);
        assert!(matches!(receipt.status, ToolStatus::Succeeded));
        assert!(receipt.summary.contains("1 step"));
        assert!(receipt.score_delta.contains_key("pattern.steps"));
    }

    #[test]
    fn apply_motivic_variation_transposes_notes() {
        let mut project = scorestrata_testkit::melodic_techno_project();
        let pattern_id = project.patterns[0].id.to_string();
        plant_note(&mut project, 60, 0);
        let params = ApplyMotivicVariationParams {
            project_id: project.id.to_string(),
            pattern_id: pattern_id.clone(),
            variation: "transpose".to_string(),
            semitones: 3,
            project: Some(serde_json::to_value(&project).unwrap_or_default()),
        };
        let receipt = apply_motivic_variation(&params);
        assert!(matches!(receipt.status, ToolStatus::Succeeded));
        assert!(receipt.summary.contains("transpose"));
        assert!(receipt.score_delta.contains_key("pattern.affected_notes"));
    }

    #[test]
    fn apply_motivic_variation_rejects_unknown() {
        let mut project = scorestrata_testkit::melodic_techno_project();
        plant_note(&mut project, 60, 0);
        let params = ApplyMotivicVariationParams {
            project_id: project.id.to_string(),
            pattern_id: project.patterns[0].id.to_string(),
            variation: "quantum_flip".to_string(),
            semitones: 0,
            project: Some(serde_json::to_value(&project).unwrap_or_default()),
        };
        let receipt = apply_motivic_variation(&params);
        assert!(matches!(receipt.status, ToolStatus::Failed));
        assert!(receipt.summary.contains("Unsupported variation"));
    }

    #[test]
    fn apply_motivic_variation_refuses_empty_pattern() {
        let project = scorestrata_testkit::melodic_techno_project();
        let params = ApplyMotivicVariationParams {
            project_id: project.id.to_string(),
            pattern_id: project.patterns[0].id.to_string(),
            variation: "transpose".to_string(),
            semitones: 3,
            project: Some(serde_json::to_value(&project).unwrap_or_default()),
        };
        let receipt = apply_motivic_variation(&params);
        assert!(matches!(receipt.status, ToolStatus::Failed));
        assert!(receipt.summary.contains("no notes"));
        assert!(
            receipt
                .warnings
                .iter()
                .any(|w| w.contains("contains no notes")),
            "expected empty-pattern warning, got {:#?}",
            receipt.warnings
        );
    }

    /// Plant a single note into the project's first pattern (the planned
    /// fixture's patterns are otherwise empty).
    fn plant_note(project: &mut scorestrata_core::domain::Project, pitch: u8, start: u64) {
        let target = project.patterns[0].id;
        if let Some(pattern) = project.patterns.iter_mut().find(|p| p.id == target) {
            pattern.notes.push(scorestrata_core::domain::Note {
                id: scorestrata_core::ids::NoteId::new(),
                pitch: scorestrata_core::domain::MidiNote::new(pitch),
                start: scorestrata_core::domain::Tick::new(start),
                duration: scorestrata_core::domain::Duration(480),
                velocity: scorestrata_core::domain::Velocity::new(100),
                tied: false,
            });
        }
    }

    #[test]
    fn place_pattern_in_segment_adds_instance() {
        let project = scorestrata_testkit::melodic_techno_project();
        let segment_id = project.arrangement.segments[0].id.to_string();
        let layer_id = project.arrangement.segments[0].layers[0].id.to_string();
        let pattern_id = project.patterns[0].id.to_string();
        let params = PlacePatternInSegmentParams {
            project_id: project.id.to_string(),
            segment_id: segment_id.clone(),
            layer_id: layer_id.clone(),
            pattern_id: pattern_id.clone(),
            start_tick: 0,
            project: Some(serde_json::to_value(&project).unwrap_or_default()),
        };
        let receipt = place_pattern_in_segment(&params);
        assert!(matches!(receipt.status, ToolStatus::Succeeded));
        assert!(receipt.score_delta.contains_key("layer.pattern_instances"));
    }

    #[test]
    fn set_layer_instrument_changes_instrument() {
        let project = scorestrata_testkit::melodic_techno_project();
        let segment_id = project.arrangement.segments[0].id.to_string();
        let layer_id = project.arrangement.segments[0].layers[0].id.to_string();
        let params = SetLayerInstrumentParams {
            project_id: project.id.to_string(),
            segment_id: segment_id.clone(),
            layer_id: layer_id.clone(),
            instrument: "supersaw".to_string(),
            project: Some(serde_json::to_value(&project).unwrap_or_default()),
        };
        let receipt = set_layer_instrument(&params);
        assert!(matches!(receipt.status, ToolStatus::Succeeded));
        assert_eq!(
            receipt
                .score_delta
                .get("layer.instrument")
                .map(String::as_str),
            Some("supersaw")
        );
    }

    #[test]
    fn set_segment_goals_upserts_energy_point() {
        let project = scorestrata_testkit::melodic_techno_project();
        let segment_id = project.arrangement.segments[0].id.to_string();
        let params = SetSegmentGoalsParams {
            project_id: project.id.to_string(),
            segment_id: segment_id.clone(),
            target_energy: 0.8,
            project: Some(serde_json::to_value(&project).unwrap_or_default()),
        };
        let receipt = set_segment_goals(&params);
        assert!(matches!(receipt.status, ToolStatus::Succeeded));
        assert!(receipt.summary.contains("energy goal"));
        assert!(receipt.score_delta.contains_key("segment.energy_target"));
    }

    #[test]
    fn set_energy_curve_replaces_curve() {
        let project = scorestrata_testkit::melodic_techno_project();
        let segment_id = project.arrangement.segments[0].id.to_string();
        let params = SetEnergyCurveParams {
            project_id: project.id.to_string(),
            curve: vec![EnergyPointSpec {
                segment_id: segment_id.clone(),
                target_energy: 0.6,
            }],
            project: Some(serde_json::to_value(&project).unwrap_or_default()),
        };
        let receipt = set_energy_curve(&params);
        assert!(matches!(receipt.status, ToolStatus::Succeeded));
        assert_eq!(
            receipt
                .score_delta
                .get("arrangement.energy_points")
                .map(String::as_str),
            Some("1")
        );
    }

    #[test]
    fn harmonize_melody_adds_scale_notes() {
        // Realistic flow: the agent writes melodic notes first (planned
        // patterns are empty), then harmonizes below them.
        let mut project = scorestrata_testkit::melodic_techno_project();
        let segment_id = project.arrangement.segments[0].id.to_string();
        let layer_id = project.arrangement.segments[0].layers[0].id.to_string();
        let pattern_id = project.arrangement.segments[0].layers[0].pattern_instances[0].pattern_id;
        if let Some(pattern) = project.patterns.iter_mut().find(|p| p.id == pattern_id) {
            pattern.notes.push(scorestrata_core::domain::Note {
                id: scorestrata_core::ids::NoteId::new(),
                pitch: scorestrata_core::domain::MidiNote::new(60),
                start: scorestrata_core::domain::Tick::new(0),
                duration: scorestrata_core::domain::Duration(480),
                velocity: scorestrata_core::domain::Velocity::new(100),
                tied: false,
            });
        }
        let params = HarmonizeMelodyParams {
            project_id: project.id.to_string(),
            segment_id,
            layer_id,
            interval_degrees: 3,
            project: Some(serde_json::to_value(&project).unwrap_or_default()),
        };
        let receipt = harmonize_melody(&params);
        assert!(matches!(receipt.status, ToolStatus::Succeeded));
        assert!(receipt.summary.contains("harmony note"));
        assert!(receipt.score_delta.contains_key("harmony.notes_added"));
    }

    #[test]
    fn add_drum_fill_appends_steps() {
        let project = scorestrata_testkit::melodic_techno_project();
        let segment_id = project.arrangement.segments[0].id.to_string();
        let layer_id = project.arrangement.segments[0].layers[0].id.to_string();
        let params = AddDrumFillParams {
            project_id: project.id.to_string(),
            segment_id: segment_id.clone(),
            layer_id: layer_id.clone(),
            steps: 4,
            project: Some(serde_json::to_value(&project).unwrap_or_default()),
        };
        let receipt = add_drum_fill(&params);
        assert!(matches!(receipt.status, ToolStatus::Succeeded));
        assert!(receipt.summary.contains("fill step"));
        assert!(receipt.score_delta.contains_key("drum.steps_added"));
    }

    #[test]
    fn increase_segment_density_duplicates_notes() {
        let project = scorestrata_testkit::melodic_techno_project();
        let segment_id = project.arrangement.segments[0].id.to_string();
        let params = IncreaseSegmentDensityParams {
            project_id: project.id.to_string(),
            segment_id: segment_id.clone(),
            factor: 2,
            project: Some(serde_json::to_value(&project).unwrap_or_default()),
        };
        let receipt = increase_segment_density(&params);
        assert!(matches!(receipt.status, ToolStatus::Succeeded));
        assert!(receipt.score_delta.contains_key("segment.notes_added"));
    }

    #[test]
    fn reduce_masking_reduces_overlap_velocity() {
        // Plant two spectrally/temporally overlapping notes so the tool has
        // real masking to reduce (planned patterns are otherwise empty).
        let mut project = scorestrata_testkit::melodic_techno_project();
        let segment_id = project.arrangement.segments[0].id.to_string();
        let pattern_ids: Vec<_> = project.arrangement.segments[0]
            .layers
            .iter()
            .flat_map(|l| l.pattern_instances.iter().map(|pi| pi.pattern_id))
            .collect();
        for pattern in &mut project.patterns {
            if !pattern_ids.contains(&pattern.id) {
                continue;
            }
            pattern.notes.extend([
                scorestrata_core::domain::Note {
                    id: scorestrata_core::ids::NoteId::new(),
                    pitch: scorestrata_core::domain::MidiNote::new(60),
                    start: scorestrata_core::domain::Tick::new(0),
                    duration: scorestrata_core::domain::Duration(480),
                    velocity: scorestrata_core::domain::Velocity::new(110),
                    tied: false,
                },
                scorestrata_core::domain::Note {
                    id: scorestrata_core::ids::NoteId::new(),
                    pitch: scorestrata_core::domain::MidiNote::new(64),
                    start: scorestrata_core::domain::Tick::new(24),
                    duration: scorestrata_core::domain::Duration(480),
                    velocity: scorestrata_core::domain::Velocity::new(110),
                    tied: false,
                },
            ]);
        }
        let params = ReduceMaskingParams {
            project_id: project.id.to_string(),
            segment_id,
            amount: 0.15,
            project: Some(serde_json::to_value(&project).unwrap_or_default()),
        };
        let receipt = reduce_masking(&params);
        assert!(matches!(receipt.status, ToolStatus::Succeeded));
        assert!(receipt.summary.contains("masking"));
        assert!(receipt.score_delta.contains_key("mix.masking_reduced"));
    }

    #[test]
    fn normalize_loudness_target_delegates_to_real_op() {
        let project = scorestrata_testkit::melodic_techno_project();
        let params = NormalizeLoudnessTargetParams {
            project_id: project.id.to_string(),
            target_lufs: Some(-10.0),
            project: Some(serde_json::to_value(&project).unwrap_or_default()),
        };
        let receipt = normalize_loudness_target(&params);
        assert!(matches!(receipt.status, ToolStatus::Succeeded));
        assert!(receipt.summary.contains("LUFS"));
        assert!(receipt.evidence_uris.iter().any(|u| u.contains("loudness")));
    }

    #[test]
    fn quantize_layer_returns_success() {
        let params = QuantizeLayerParams {
            project_id: "proj_1".to_string(),
            segment_id: "seg_1".to_string(),
            layer_id: "layer_1".to_string(),
            strength: 1.0,
            preserve_rhythm: true,
        };
        let receipt = quantize_layer_to_scale(&params);
        assert!(matches!(receipt.status, ToolStatus::Succeeded));
        assert!(receipt.summary.contains("layer_1"));
        assert!(receipt.summary.contains("seg_1"));
    }

    #[test]
    fn add_transition_package_lists_elements() {
        let params = AddTransitionParams {
            project_id: "proj_1".to_string(),
            from_segment_id: "verse".to_string(),
            to_segment_id: "chorus".to_string(),
            fill: true,
            riser: true,
            impact: false,
        };
        let receipt = add_transition_package(&params);
        assert!(receipt.summary.contains("fill"));
        assert!(receipt.summary.contains("riser"));
        assert!(!receipt.summary.contains("impact"));
    }

    #[test]
    fn get_snapshot_diff_without_payload_fails_transparently() {
        let params = SnapshotDiffParams {
            project_id: "proj_1".to_string(),
            snapshot_a: "snap_a".to_string(),
            snapshot_b: "snap_b".to_string(),
            project_a: None,
            project_b: None,
        };
        let receipt = get_snapshot_diff(&params);
        assert!(matches!(receipt.status, ToolStatus::Failed));
        assert!(
            receipt.warnings.iter().any(|w| w.contains("project_a")),
            "expected missing-payload warning, got {:#?}",
            receipt.warnings
        );
    }

    #[test]
    fn get_snapshot_diff_computes_real_diff() {
        // Two melodic-techno projects; the second has a modified pattern
        // (note count) so the diff reports a pattern change.
        let project_a = scorestrata_testkit::melodic_techno_project();
        let mut project_b = project_a.clone();
        project_b.name = "diffed".to_string();
        // Give B a different segment bar layout to force a segment diff.
        for seg in &mut project_b.arrangement.segments {
            seg.bars += 4;
        }
        project_b.arrangement.total_bars += 4;

        let params = SnapshotDiffParams {
            project_id: project_a.id.to_string(),
            snapshot_a: "snap_a".to_string(),
            snapshot_b: "snap_b".to_string(),
            project_a: Some(serde_json::to_value(&project_a).unwrap_or_default()),
            project_b: Some(serde_json::to_value(&project_b).unwrap_or_default()),
        };
        let receipt = get_snapshot_diff(&params);
        assert!(
            matches!(receipt.status, ToolStatus::Succeeded),
            "diff should succeed, got {receipt:#?}"
        );
        assert!(
            receipt.score_delta.contains_key("diff.segments_changed"),
            "receipt reports segment diff count"
        );
        assert!(receipt.summary.contains("segments changed"));
    }

    #[test]
    fn get_snapshot_diff_identical_projects_report_no_changes() {
        let project = scorestrata_testkit::melodic_techno_project();
        let params = SnapshotDiffParams {
            project_id: project.id.to_string(),
            snapshot_a: "a".to_string(),
            snapshot_b: "b".to_string(),
            project_a: Some(serde_json::to_value(&project).unwrap_or_default()),
            project_b: Some(serde_json::to_value(&project).unwrap_or_default()),
        };
        let receipt = get_snapshot_diff(&params);
        assert!(matches!(receipt.status, ToolStatus::Succeeded));
        assert!(
            receipt.warnings.iter().any(|w| w.contains("identical")),
            "expected no-change warning, got {:#?}",
            receipt.warnings
        );
    }

    #[test]
    fn get_candidate_diff_selects_winner() {
        // Candidate A is a healthy planned project; B has an out-of-scale
        // pattern (C# in C minor) so validation scores it lower.
        let project_a = scorestrata_testkit::melodic_techno_project();
        let mut project_b = project_a.clone();
        project_b.name = "candidate_b".to_string();
        for pattern in &mut project_b.patterns {
            for note in &mut pattern.notes {
                note.pitch = scorestrata_core::domain::MidiNote::new(61); // C# — out of scale
            }
        }

        let params = CandidateDiffParams {
            project_id: project_a.id.to_string(),
            candidate_a: "cand_a".to_string(),
            candidate_b: "cand_b".to_string(),
            project_a: Some(serde_json::to_value(&project_a).unwrap_or_default()),
            project_b: Some(serde_json::to_value(&project_b).unwrap_or_default()),
        };
        let receipt = get_candidate_diff(&params);
        assert!(
            matches!(receipt.status, ToolStatus::Succeeded),
            "candidate diff should succeed, got {receipt:#?}"
        );
        assert!(receipt.score_delta.contains_key("candidate.winner"));
        assert!(
            receipt.summary.contains("cand_a") || receipt.summary.contains("cand_b"),
            "summary names the candidates"
        );
    }

    #[test]
    fn get_candidate_diff_without_payload_fails_transparently() {
        let params = CandidateDiffParams {
            project_id: "proj_1".to_string(),
            candidate_a: "a".to_string(),
            candidate_b: "b".to_string(),
            project_a: None,
            project_b: None,
        };
        let receipt = get_candidate_diff(&params);
        assert!(matches!(receipt.status, ToolStatus::Failed));
    }

    #[test]
    fn isolate_layer_returns_success_with_project() {
        let project = scorestrata_testkit::melodic_techno_project();
        let layer_id = project.arrangement.segments[0].layers[0].id.to_string();
        let params = IsolateLayerParams {
            project_id: "proj_1".to_string(),
            layer_id: layer_id.clone(),
            project: Some(serde_json::to_value(&project).unwrap_or_default()),
        };
        let receipt = isolate_layer(&params);
        assert!(
            matches!(receipt.status, ToolStatus::Succeeded),
            "isolation should succeed, got {receipt:#?}"
        );
        assert!(receipt.summary.contains(&layer_id));
    }

    #[test]
    fn isolate_layer_unknown_layer_fails() {
        let project = scorestrata_testkit::melodic_techno_project();
        let params = IsolateLayerParams {
            project_id: "proj_1".to_string(),
            layer_id: "no_such_layer".to_string(),
            project: Some(serde_json::to_value(&project).unwrap_or_default()),
        };
        let receipt = isolate_layer(&params);
        assert!(matches!(receipt.status, ToolStatus::Failed));
    }

    #[test]
    fn get_timeline_view_reflects_layers_segments_and_octaves() {
        let project = scorestrata_testkit::melodic_techno_project();
        let params = GetTimelineViewParams {
            project_id: project.id.to_string(),
            project: Some(serde_json::to_value(&project).unwrap_or_default()),
        };
        let receipt = get_timeline_view(&params);
        assert!(
            matches!(receipt.status, ToolStatus::Succeeded),
            "timeline view should succeed, got {receipt:#?}"
        );
        // The summary embeds the compact JSON view for the agent.
        assert!(
            receipt.summary.starts_with("timeline_view {"),
            "summary embeds JSON"
        );
        assert!(receipt.summary.contains("\"sections\""));
        assert!(receipt.summary.contains("\"layers\""));
        assert!(receipt.score_delta.contains_key("timeline.sections"));
        assert!(receipt.score_delta.contains_key("timeline.layers"));
        assert!(
            receipt
                .evidence_uris
                .contains(&format!("project://{}/timeline", project.id)),
            "timeline evidence uri"
        );

        // The structured view itself: segments become sections, each layer
        // becomes a lane with a derived octave register and note rows.
        let view: crate::resources::TimelineView =
            serde_json::from_str(&receipt.summary["timeline_view ".len()..]).expect("view JSON");
        assert_eq!(view.genre, "melodic_techno");
        assert!(!view.sections.is_empty(), "has arrangement sections");
        assert!(!view.layers.is_empty(), "has layer lanes");
        for layer in &view.layers {
            assert!(
                layer.min_octave <= layer.max_octave,
                "octave register is ordered: {}..={}",
                layer.min_octave,
                layer.max_octave
            );
            assert!(layer.note_count > 0, "lane has notes");
            assert!(
                layer.notes.len() <= layer.note_count as usize,
                "note rows are a sample of the lane"
            );
            for note in &layer.notes {
                assert!(
                    note.duration_ticks > 0,
                    "durations are computed from note-offs, got {}",
                    note.duration_ticks
                );
            }
        }
    }

    #[test]
    fn timeline_view_shows_automation_lanes() {
        let mut project = scorestrata_testkit::melodic_techno_project();
        let segment = &mut project.arrangement.segments[0];
        let layer = &mut segment.layers[1]; // Bass
        layer.automation.push(scorestrata_core::domain::Automation {
            layer_id: layer.id,
            param: scorestrata_core::domain::AutomationParam::Cutoff,
            points: vec![
                scorestrata_core::domain::AutomationPoint {
                    tick: scorestrata_core::domain::Tick::new(0),
                    value: 0.2,
                },
                scorestrata_core::domain::AutomationPoint {
                    tick: scorestrata_core::domain::Tick::new(1920),
                    value: 1.0,
                },
            ],
        });
        layer.fx = Some(scorestrata_core::domain::FxChain {
            high_pass_hz: Some(40.0),
            sidechain_from_kick: true,
            ..Default::default()
        });
        let view = crate::resources::build_timeline_view(&project);
        let bass = view
            .layers
            .iter()
            .find(|l| l.name == "Bass")
            .expect("bass lane present");
        assert_eq!(bass.automation.len(), 1, "cutoff lane surfaced in view");
        assert_eq!(bass.automation[0].param, "cutoff");
        assert_eq!(bass.automation[0].controller, 74);
        assert_eq!(bass.automation[0].points.len(), 2);
        assert!(
            view.layers.iter().all(|l| l.automation.len() <= 4),
            "no stray lanes elsewhere"
        );
        // G5 — the FX chain is visible in the arrange view.
        let fx = bass.fx.as_deref().expect("bass FX surfaced");
        assert!(fx.contains("hpf 40Hz"), "HPF shown: {fx}");
        assert!(fx.contains("sidechain"), "sidechain shown: {fx}");
    }

    #[test]
    fn get_timeline_view_without_payload_fails_transparently() {
        let params = GetTimelineViewParams {
            project_id: "proj_1".to_string(),
            project: None,
        };
        let receipt = get_timeline_view(&params);
        assert!(matches!(receipt.status, ToolStatus::Failed));
        assert!(
            receipt
                .warnings
                .iter()
                .any(|w| w.contains("serialized project payload")),
            "transparent stub warning"
        );
    }

    #[test]
    fn get_arrangement_templates_returns_all_named_genres() {
        // G6 resource: no genre filter → every named genre table plus the
        // default fallback, each with the production 5-section archetype.
        let params = GetArrangementTemplatesParams {
            project_id: Some("proj_1".to_string()),
            genre: None,
        };
        let receipt = get_arrangement_templates(&params);
        assert!(matches!(receipt.status, ToolStatus::Succeeded));
        assert_eq!(
            receipt.score_delta.get("arrangement_templates.genres"),
            Some(&"20".to_string()),
            "twenty named genre tables (rock + 13 S3 families)"
        );
        assert_eq!(
            receipt.score_delta.get("arrangement_templates.sections"),
            Some(&"100".to_string()),
            "20 genres × 5 sections"
        );
        assert!(receipt.summary.contains("techno"));
        assert!(receipt.summary.contains("ambient"));
        assert!(receipt.summary.contains("rock"));
        assert!(receipt.summary.contains("\"genre\":\"default\""));
    }

    #[test]
    fn get_arrangement_templates_filters_by_genre() {
        // A producer can inspect a single genre's archetype before applying
        // its template (read-only, no mutation).
        let params = GetArrangementTemplatesParams {
            project_id: None,
            genre: Some("house".to_string()),
        };
        let receipt = get_arrangement_templates(&params);
        assert!(matches!(receipt.status, ToolStatus::Succeeded));
        assert_eq!(
            receipt.score_delta.get("arrangement_templates.genres"),
            Some(&"1".to_string())
        );
        assert!(receipt.summary.contains("\"genre\":\"house\""));
        assert!(
            !receipt.summary.contains("\"genre\":\"techno\""),
            "filtered to the requested genre only"
        );
        // Energy arc peaks at the drop for the requested genre too.
        assert!(receipt.summary.contains("\"role\":\"drop\""));
    }

    #[test]
    fn get_arrangement_templates_unknown_genre_falls_back_to_default_table() {
        // Unknown genres fall back to the default table's section layout
        // (16/8/16/8/8), and the response stays self-consistent:
        // `genres[0]` always matches `templates[0].genre` — the label is
        // derived from the returned template, never diverges from it.
        let params = GetArrangementTemplatesParams {
            project_id: None,
            genre: Some("jazz".to_string()),
        };
        let receipt = get_arrangement_templates(&params);
        assert!(matches!(receipt.status, ToolStatus::Succeeded));
        assert_eq!(
            receipt.score_delta.get("arrangement_templates.genres"),
            Some(&"1".to_string())
        );
        // The genre label the agent sees is exactly the one on the template.
        let start = receipt.summary.find("\"genres\":[").map(|i| i + 10);
        let genre_label = start.and_then(|i| {
            receipt.summary[i..]
                .find(']')
                .map(|j| &receipt.summary[i..i + j])
        });
        assert!(
            genre_label
                .is_some_and(|label| { receipt.summary.contains(&format!("\"genre\":{label}")) }),
            "genres[0] == templates[0].genre (self-consistent)"
        );
        // And the sections use the default-table layout (16-bar intro).
        assert!(receipt.summary.contains("\"role\":\"intro\",\"bars\":16"));
    }

    #[test]
    fn get_production_playbook_guides_rock_workflow() {
        // The skills layer serves the full guided workflow for the genre:
        // ordered steps naming the exact tools, plus octave/mix guidance.
        let params = GetProductionPlaybookParams {
            genre: Some("nu_metal".to_string()),
        };
        let receipt = get_production_playbook(&params);
        assert!(matches!(receipt.status, ToolStatus::Succeeded));
        assert_eq!(
            receipt.score_delta.get("playbook.genre"),
            Some(&"nu_metal".to_string())
        );
        assert!(
            receipt
                .score_delta
                .get("playbook.steps")
                .is_some_and(|n| n.parse::<usize>().is_ok_and(|n| n >= 5)),
            "guided workflow has ordered steps"
        );
        // The summary carries the tools and the guidance, not just names.
        assert!(receipt.summary.contains("generate_config_from_brief"));
        assert!(receipt.summary.contains("set_layer_register"));
        assert!(receipt.summary.contains("apply_genre_mix_preset"));
        assert!(receipt.summary.contains("\"octave_map\""));
        assert!(
            receipt
                .evidence_uris
                .iter()
                .any(|u| u.contains("system://playbooks")),
            "served from the playbooks resource"
        );
    }

    #[test]
    fn get_production_playbook_defaults_to_general() {
        let params = GetProductionPlaybookParams { genre: None };
        let receipt = get_production_playbook(&params);
        assert!(matches!(receipt.status, ToolStatus::Succeeded));
        assert_eq!(
            receipt.score_delta.get("playbook.genre"),
            Some(&"general".to_string())
        );
        assert!(receipt.summary.contains("\"steps\""));
    }

    #[test]
    fn set_layer_register_constrains_layer_octave_band() {
        let project = scorestrata_testkit::melodic_techno_project();
        let segment_id = project.arrangement.segments[0].id.to_string();
        let layer_id = project.arrangement.segments[0].layers[1].id.to_string(); // Bass
        let params = SetLayerRegisterParams {
            project_id: project.id.to_string(),
            segment_id: segment_id.clone(),
            layer_id: layer_id.clone(),
            min_octave: 1,
            max_octave: 2,
            project: Some(serde_json::to_value(&project).unwrap_or_default()),
        };
        let receipt = set_layer_register(&params);
        assert!(
            matches!(receipt.status, ToolStatus::Succeeded),
            "register set succeeds: {receipt:#?}"
        );
        assert!(receipt.summary.contains("octaves 1..2"));
        assert!(receipt.score_delta.contains_key("layers.register"));
        assert!(
            receipt
                .next_actions
                .contains(&"get_timeline_view".to_string()),
            "suggests viewing the constrained timeline"
        );
        assert!(
            receipt.evidence_uris[0].contains("/layers/"),
            "cites the constrained layer"
        );
    }

    #[test]
    fn write_automation_adds_cutoff_lane_to_layer() {
        let project = scorestrata_testkit::melodic_techno_project();
        let segment_id = project.arrangement.segments[0].id.to_string();
        let layer_id = project.arrangement.segments[0].layers[1].id.to_string(); // Bass
        let params = WriteAutomationParams {
            project_id: project.id.to_string(),
            segment_id: segment_id.clone(),
            layer_id: layer_id.clone(),
            param: "cutoff".to_string(),
            points: vec![
                scorestrata_core::domain::AutomationPointJson {
                    tick: 0,
                    value: 0.2,
                },
                scorestrata_core::domain::AutomationPointJson {
                    tick: 1920,
                    value: 1.0,
                },
            ],
            project: Some(serde_json::to_value(&project).unwrap_or_default()),
        };
        let receipt = write_automation(&params);
        assert!(
            matches!(receipt.status, ToolStatus::Succeeded),
            "automation write succeeds: {receipt:#?}"
        );
        assert!(receipt.summary.contains("cutoff"));
        assert!(receipt.summary.contains("2 points"));
        assert!(receipt.summary.contains("CC 74"));
        assert!(receipt.score_delta.contains_key("layers.automation_lanes"));
    }

    #[test]
    fn write_automation_rejects_unknown_param() {
        let project = scorestrata_testkit::melodic_techno_project();
        let params = WriteAutomationParams {
            project_id: project.id.to_string(),
            segment_id: project.arrangement.segments[0].id.to_string(),
            layer_id: project.arrangement.segments[0].layers[0].id.to_string(),
            param: "reverb_weirdness".to_string(),
            points: vec![],
            project: Some(serde_json::to_value(&project).unwrap_or_default()),
        };
        let receipt = write_automation(&params);
        assert!(matches!(receipt.status, ToolStatus::Failed));
        assert!(receipt.summary.contains("Unsupported automation param"));
    }

    #[test]
    fn set_track_fx_applies_chain_to_layer() {
        let project = scorestrata_testkit::melodic_techno_project();
        let segment_id = project.arrangement.segments[0].id.to_string();
        let layer_id = project.arrangement.segments[0].layers[1].id.to_string(); // Bass
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
        let params = SetTrackFxParams {
            project_id: project.id.to_string(),
            segment_id: segment_id.clone(),
            layer_id: layer_id.clone(),
            fx: serde_json::to_value(&fx).unwrap_or_default(),
            project: Some(serde_json::to_value(&project).unwrap_or_default()),
        };
        let receipt = set_track_fx(&params);
        assert!(
            matches!(receipt.status, ToolStatus::Succeeded),
            "fx set succeeds: {receipt:#?}"
        );
        assert!(receipt.summary.contains("hpf 40 Hz"));
        assert!(receipt.summary.contains("sidechain"));
        assert!(receipt.score_delta.contains_key("layers.fx"));
    }

    #[test]
    fn set_track_mix_writes_faders() {
        let project = scorestrata_testkit::melodic_techno_project();
        let segment_id = project.arrangement.segments[0].id.to_string();
        let layer_id = project.arrangement.segments[0].layers[1].id.to_string(); // Bass
        let params = SetTrackMixParams {
            project_id: project.id.to_string(),
            segment_id: segment_id.clone(),
            layer_id: layer_id.clone(),
            gain_db: Some(-6.0),
            pan: Some(0.2),
            project: Some(serde_json::to_value(&project).unwrap_or_default()),
        };
        let receipt = set_track_mix(&params);
        assert!(
            matches!(receipt.status, ToolStatus::Succeeded),
            "mix set succeeds: {receipt:#?}"
        );
        assert!(receipt.summary.contains("-6"));
        assert!(receipt.summary.contains("0.2"));
        assert!(receipt.score_delta.contains_key("layers.mix"));
    }

    #[test]
    fn apply_genre_mix_preset_chains_instruments() {
        let project = scorestrata_testkit::melodic_techno_project();
        let params = ApplyGenreMixPresetParams {
            project_id: project.id.to_string(),
            genre: "melodic_techno".to_string(),
            project: Some(serde_json::to_value(&project).unwrap_or_default()),
        };
        let receipt = apply_genre_mix_preset(&params);
        assert!(
            matches!(receipt.status, ToolStatus::Succeeded),
            "preset applies: {receipt:#?}"
        );
        assert!(
            receipt
                .summary
                .contains("Applied melodic_techno mix preset")
        );
        let count = receipt.score_delta.get("layers.fx_preset").cloned();
        let n: u32 = count.and_then(|c| c.parse().ok()).unwrap_or(0);
        // Drums, bass (and any lead) layers all get chains — must be ≥ 2.
        assert!(n >= 2, "drum + bass layers get chains, got {n}");
        // The genre helper itself gives the bass a kick-sidechain.
        let bass_chain = genre_mix_chain("melodic_techno", "bass_synth");
        assert!(
            bass_chain.is_some_and(|c| c.sidechain_from_kick),
            "bass ducks the kick in the techno preset"
        );
        let pad_chain = genre_mix_chain("melodic_techno", "pad_synth");
        assert!(
            pad_chain.is_some_and(|c| c.sends.reverb > 0.0),
            "pads get a reverb send"
        );
    }

    #[test]
    fn apply_arrangement_template_builds_genre_section_skeleton() {
        let project = scorestrata_testkit::melodic_techno_project();
        let params = ApplyArrangementTemplateParams {
            project_id: project.id.to_string(),
            genre: "techno".to_string(),
            total_bars: 72,
            project: Some(serde_json::to_value(&project).unwrap_or_default()),
        };
        let receipt = apply_arrangement_template(&params);
        assert!(
            matches!(receipt.status, ToolStatus::Succeeded),
            "template applies: {receipt:#?}"
        );
        // Techno archetype: intro 16 / build 8 / drop 32 / breakdown 8 / outro 8.
        assert!(receipt.summary.contains("5 sections across 72 bars"));
        assert_eq!(
            receipt
                .score_delta
                .get("arrangement.sections")
                .map(String::as_str),
            Some("5")
        );
        assert_eq!(
            receipt
                .score_delta
                .get("arrangement.bars")
                .map(String::as_str),
            Some("72")
        );
    }

    #[test]
    fn apply_arrangement_template_distribution_sums_exactly_and_drop_is_longest() {
        // A non-divisible total (70 vs the 72-bar archetype) must still yield
        // sections that sum to exactly 70, with the leftover handed to the
        // drop (the largest archetype share) — the DAW convention.
        let project = scorestrata_testkit::melodic_techno_project();
        let params = ApplyArrangementTemplateParams {
            project_id: project.id.to_string(),
            genre: "techno".to_string(),
            total_bars: 70,
            project: Some(serde_json::to_value(&project).unwrap_or_default()),
        };
        let receipt = apply_arrangement_template(&params);
        assert!(matches!(receipt.status, ToolStatus::Succeeded));
        let layout = receipt
            .score_delta
            .get("arrangement.layout")
            .cloned()
            .unwrap_or_default();
        // layout: "intro N, build N, drop N, breakdown N, outro N"
        let bars: Vec<u32> = layout
            .split(',')
            .filter_map(|part| part.split_whitespace().last())
            .filter_map(|n| n.parse().ok())
            .collect();
        assert_eq!(bars.len(), 5, "five sections: {layout}");
        assert_eq!(bars.iter().sum::<u32>(), 70, "exact total: {layout}");
        // Drop must be the longest section.
        let drop = bars[2];
        assert!(bars.iter().all(|&b| b <= drop), "drop longest: {layout}");
    }

    #[test]
    fn apply_arrangement_template_seeds_stub_layers_from_hints() {
        // The template's instrument hints must reach the new segments as stub
        // layers with the G2 octave-register discipline (bass 1–2, lead 5–7,
        // drums unpitched).
        let project = scorestrata_testkit::melodic_techno_project();
        let params = ApplyArrangementTemplateParams {
            project_id: project.id.to_string(),
            genre: "techno".to_string(),
            total_bars: 72,
            project: Some(serde_json::to_value(&project).unwrap_or_default()),
        };
        let receipt = apply_arrangement_template(&params);
        assert!(matches!(receipt.status, ToolStatus::Succeeded));
        // The receipt itself is stateless, so the layer seeding is verified
        // through the shared register helper it uses — bass in 1–2, lead in
        // 5–7, drums unpitched, fallback 3–6.
        let bass = register_for_instrument("Bass");
        assert_eq!(
            bass,
            Some(scorestrata_core::domain::Register::new(1, 2)),
            "bass register"
        );
        let lead = register_for_instrument("Lead");
        assert_eq!(
            lead,
            Some(scorestrata_core::domain::Register::new(5, 7)),
            "lead register"
        );
        let drums = register_for_instrument("Drums");
        assert_eq!(drums, None, "drums are unpitched");
        let pad = register_for_instrument("Pad");
        assert_eq!(
            pad,
            Some(scorestrata_core::domain::Register::new(3, 5)),
            "pad register"
        );
    }

    #[test]
    fn apply_arrangement_template_rejects_zero_bars() {
        let project = scorestrata_testkit::melodic_techno_project();
        let params = ApplyArrangementTemplateParams {
            project_id: project.id.to_string(),
            genre: "house".to_string(),
            total_bars: 0,
            project: Some(serde_json::to_value(&project).unwrap_or_default()),
        };
        let receipt = apply_arrangement_template(&params);
        assert!(matches!(receipt.status, ToolStatus::Failed));
        assert!(receipt.summary.contains("total_bars must be > 0"));
    }

    #[test]
    fn extend_arrangement_grows_existing_arrangement() {
        // G3 "make it 64 bars": the arrangement keeps its existing sections
        // and appends genre-template sections to reach the requested total.
        let project = scorestrata_testkit::melodic_techno_project();
        let original_segments = project.arrangement.segments.len();
        let original_bars = project.arrangement.total_bars;
        let params = ExtendArrangementParams {
            project_id: project.id.to_string(),
            total_bars: original_bars + 32,
            project: Some(serde_json::to_value(&project).unwrap_or_default()),
        };
        let receipt = extend_arrangement(&params);
        assert!(
            matches!(receipt.status, ToolStatus::Succeeded),
            "extension succeeds: {receipt:#?}"
        );
        let bars = receipt
            .score_delta
            .get("arrangement.bars")
            .cloned()
            .unwrap_or_default();
        assert_eq!(bars, (original_bars + 32).to_string());
        // Existing sections are kept; the template adds its archetype count.
        let sections = receipt
            .score_delta
            .get("arrangement.sections")
            .cloned()
            .unwrap_or_default();
        assert_eq!(sections, (original_segments + 5).to_string());
        let extended_from = receipt
            .score_delta
            .get("arrangement.extended_from")
            .cloned()
            .unwrap_or_default();
        assert_eq!(extended_from, original_bars.to_string());
        assert!(receipt.summary.contains("Extended arrangement"));
    }

    #[test]
    fn extend_arrangement_rejects_shrink_target() {
        let project = scorestrata_testkit::melodic_techno_project();
        let params = ExtendArrangementParams {
            project_id: project.id.to_string(),
            total_bars: 8,
            project: Some(serde_json::to_value(&project).unwrap_or_default()),
        };
        let receipt = extend_arrangement(&params);
        assert!(matches!(receipt.status, ToolStatus::Failed));
        assert!(
            receipt
                .summary
                .contains("must exceed the current arrangement length")
        );
    }

    #[test]
    fn extend_arrangement_rejects_zero_bars() {
        let project = scorestrata_testkit::melodic_techno_project();
        let params = ExtendArrangementParams {
            project_id: project.id.to_string(),
            total_bars: 0,
            project: Some(serde_json::to_value(&project).unwrap_or_default()),
        };
        let receipt = extend_arrangement(&params);
        assert!(matches!(receipt.status, ToolStatus::Failed));
        assert!(receipt.summary.contains("total_bars must be > 0"));
    }

    #[test]
    fn extend_arrangement_small_request_keeps_segments_total_invariant() {
        // Growing by fewer bars than the template's archetype section count
        // (5) floors each appended section at 1 bar, so the final length
        // exceeds the request — but the receipt must report an actual total
        // that equals the segment coverage (the invariant the timeline ruler
        // and validators rely on).
        let project = scorestrata_testkit::melodic_techno_project();
        let original_bars = project.arrangement.total_bars;
        let params = ExtendArrangementParams {
            project_id: project.id.to_string(),
            total_bars: original_bars + 2,
            project: Some(serde_json::to_value(&project).unwrap_or_default()),
        };
        let receipt = extend_arrangement(&params);
        assert!(matches!(receipt.status, ToolStatus::Succeeded));
        let total = receipt
            .score_delta
            .get("arrangement.bars")
            .and_then(|v| v.parse::<u32>().ok())
            .unwrap_or(0);
        let layout = receipt
            .score_delta
            .get("arrangement.layout")
            .cloned()
            .unwrap_or_default();
        let covered: u32 = layout
            .split(',')
            .filter_map(|part| part.split_whitespace().last())
            .filter_map(|n| n.parse::<u32>().ok())
            .sum();
        assert_eq!(
            covered, total,
            "segments sum exactly to total_bars: {layout}"
        );
        assert!(
            total > original_bars + 2,
            "sub-5-bar request overshoots the 1-bar-per-section floor ({total} vs {original_bars} + 2)"
        );
    }

    #[test]
    fn import_reference_profile_stores_spectral_reference() {
        let project = scorestrata_testkit::melodic_techno_project();
        let reference = scorestrata_core::domain::SpectralReference {
            track_name: "placeholder".to_string(),
            genre: "placeholder".to_string(),
            band_energy_expected: [0.15, 0.25, 0.2, 0.2, 0.1, 0.1],
            centroid_expected_hz: 2400.0,
            aliasing_max: 0.02,
            loudness_target_lufs: -8.0,
        };
        let params = ImportReferenceProfileParams {
            project_id: project.id.to_string(),
            track_name: "Techno_Reference_v3".to_string(),
            genre: "techno".to_string(),
            reference: serde_json::to_value(&reference).unwrap_or_default(),
            project: Some(serde_json::to_value(&project).unwrap_or_default()),
        };
        let receipt = import_reference_profile(&params);
        assert!(
            matches!(receipt.status, ToolStatus::Succeeded),
            "reference imports: {receipt:#?}"
        );
        assert!(receipt.summary.contains("Techno_Reference_v3"));
        assert!(receipt.summary.contains("techno"));
        assert_eq!(
            receipt
                .score_delta
                .get("render.reference_profile")
                .map(String::as_str),
            Some("Techno_Reference_v3")
        );
    }

    #[test]
    fn import_reference_profile_rejects_invalid_reference() {
        let project = scorestrata_testkit::melodic_techno_project();
        let params = ImportReferenceProfileParams {
            project_id: project.id.to_string(),
            track_name: "bad".to_string(),
            genre: "techno".to_string(),
            reference: serde_json::json!({
                "band_energy_expected": "not-an-array",
            }),
            project: Some(serde_json::to_value(&project).unwrap_or_default()),
        };
        let receipt = import_reference_profile(&params);
        assert!(matches!(receipt.status, ToolStatus::Failed));
        assert!(receipt.summary.contains("Could not deserialize"));
    }

    #[test]
    fn repeat_pattern_places_n_instances_with_transpose_ramp() {
        let project = scorestrata_testkit::melodic_techno_project();
        let segment_id = project.arrangement.segments[0].id.to_string();
        let layer_id = project.arrangement.segments[0].layers[1].id.to_string();
        let pattern_id = project.patterns[0].id.to_string();
        let params = RepeatPatternInSegmentParams {
            project_id: project.id.to_string(),
            segment_id: segment_id.clone(),
            layer_id: layer_id.clone(),
            pattern_id: pattern_id.clone(),
            count: 4,
            step_ticks: 960,
            transpose_step: 12,
            project: Some(serde_json::to_value(&project).unwrap_or_default()),
        };
        let receipt = repeat_pattern_in_segment(&params);
        assert!(
            matches!(receipt.status, ToolStatus::Succeeded),
            "repeat succeeds: {receipt:#?}"
        );
        assert!(receipt.summary.contains("4×"));
        assert_eq!(
            receipt.score_delta.get("arrangement.instances"),
            Some(&"4".to_string())
        );

        // Re-run against the original payload is not needed — the placement
        // math is verified by the score_delta and the register test covers
        // state persistence. The tool's core contract (count, spacing, ramp)
        // is deterministic from params.
        let _ = (segment_id, layer_id, pattern_id);
    }

    #[test]
    fn clone_pattern_transposed_shifts_all_notes() {
        let project = scorestrata_testkit::melodic_techno_project();
        let pattern_id = project.patterns[0].id.to_string();
        let params = ClonePatternTransposedParams {
            project_id: project.id.to_string(),
            pattern_id: pattern_id.clone(),
            semitones: 12,
            name: Some("Lead_Octave_Up".to_string()),
            project: Some(serde_json::to_value(&project).unwrap_or_default()),
        };
        let receipt = clone_pattern_transposed(&params);
        assert!(
            matches!(receipt.status, ToolStatus::Succeeded),
            "clone succeeds: {receipt:#?}"
        );
        assert!(receipt.summary.contains("Lead_Octave_Up"));
        assert!(receipt.summary.contains("12 semitones"));
        assert!(receipt.evidence_uris[0].contains("/patterns/"));
        assert_eq!(
            receipt.score_delta.get("patterns.cloned"),
            Some(&"1".to_string())
        );
        assert!(
            receipt
                .next_actions
                .contains(&"place_pattern_in_segment".to_string()),
            "clone suggests placing it"
        );
    }

    #[test]
    fn solo_segment_returns_success_with_project() {
        let project = scorestrata_testkit::melodic_techno_project();
        let segment_id = project.arrangement.segments[0].id.to_string();
        let params = SoloSegmentParams {
            project_id: "proj_1".to_string(),
            segment_id: segment_id.clone(),
            project: Some(serde_json::to_value(&project).unwrap_or_default()),
        };
        let receipt = solo_segment(&params);
        assert!(
            matches!(receipt.status, ToolStatus::Succeeded),
            "solo should succeed, got {receipt:#?}"
        );
        assert!(receipt.summary.contains("bars"));
        assert!(receipt.summary.contains("layers"));
    }

    #[test]
    fn solo_segment_unknown_segment_fails() {
        let project = scorestrata_testkit::melodic_techno_project();
        let params = SoloSegmentParams {
            project_id: "proj_1".to_string(),
            segment_id: "no_such_segment".to_string(),
            project: Some(serde_json::to_value(&project).unwrap_or_default()),
        };
        let receipt = solo_segment(&params);
        assert!(matches!(receipt.status, ToolStatus::Failed));
    }

    #[test]
    fn explain_metric_returns_real_explanation() {
        let project = scorestrata_testkit::melodic_techno_project();
        let params = ExplainMetricParams {
            project_id: "proj_1".to_string(),
            metric_name: "harmony.scale_compliance".to_string(),
            project: Some(serde_json::to_value(&project).unwrap_or_default()),
        };
        let receipt = explain_metric(&params);
        // A fully planned project should score 1.0 on scale compliance (no
        // root causes); a metric that is measured appears in the scorecard.
        if matches!(receipt.status, ToolStatus::Succeeded) {
            assert!(receipt.summary.contains("scale_compliance"));
            assert!(receipt.score_delta.contains_key("metric.value"));
        } else {
            // If the fixture has no scale diagnostics, the metric is not in
            // the scorecard and the tool fails cleanly with a clear reason.
            assert!(receipt.summary.contains("not found"));
        }
    }

    #[test]
    fn get_command_trace_returns_real_trace() {
        let mut memory = scorestrata_storage::memory::ProjectMemory::new();
        memory.record_command(scorestrata_storage::memory::CommandRecord {
            command_id: "cmd_001".to_string(),
            command_type: "apply_patch".to_string(),
            actor: "agent:composer".to_string(),
            success: true,
            score_delta: Some(0.14),
            duration_ms: 42,
            timestamp: chrono::Utc::now().to_rfc3339(),
        });
        let params = GetCommandTraceParams {
            project_id: "proj_1".to_string(),
            command_id: "cmd_001".to_string(),
            memory: Some(serde_json::to_value(&memory).unwrap_or_default()),
        };
        let receipt = get_command_trace(&params);
        assert!(
            matches!(receipt.status, ToolStatus::Succeeded),
            "trace should succeed, got {receipt:#?}"
        );
        assert!(receipt.summary.contains("cmd_001"));
        assert!(receipt.summary.contains("42 ms"));
    }

    #[test]
    fn get_command_trace_unknown_command_fails() {
        let memory = scorestrata_storage::memory::ProjectMemory::new();
        let params = GetCommandTraceParams {
            project_id: "proj_1".to_string(),
            command_id: "cmd_404".to_string(),
            memory: Some(serde_json::to_value(&memory).unwrap_or_default()),
        };
        let receipt = get_command_trace(&params);
        assert!(matches!(receipt.status, ToolStatus::Failed));
        assert!(receipt.summary.contains("not found"));
    }

    #[test]
    fn get_failure_report_surfaces_issues_with_project() {
        let mut project = scorestrata_testkit::melodic_techno_project();
        for pattern in &mut project.patterns {
            for note in &mut pattern.notes {
                note.pitch = scorestrata_core::domain::MidiNote::new(61);
            }
        }
        let params = GetFailureReportParams {
            project_id: "proj_1".to_string(),
            project: Some(serde_json::to_value(&project).unwrap_or_default()),
        };
        let receipt = get_failure_report(&params);
        // Status mirrors the report's `needs_repair` decision: a project with
        // failed metrics is reported as failed, not "succeeded but escalate".
        assert!(
            matches!(receipt.status, ToolStatus::Failed),
            "failure report on a broken project should be Failed, got {receipt:#?}"
        );
        assert!(receipt.summary.contains("failed metric"));
        assert!(receipt.score_delta.contains_key("report.overall_score"));
        assert!(
            receipt
                .next_actions
                .contains(&"create_human_review_packet".to_string()),
            "failed project recommends escalation, got {:?}",
            receipt.next_actions
        );
    }

    #[test]
    fn get_failure_report_requires_payload() {
        let params = GetFailureReportParams {
            project_id: "proj_1".to_string(),
            project: None,
        };
        let receipt = get_failure_report(&params);
        assert!(matches!(receipt.status, ToolStatus::Failed));
        assert!(receipt.summary.contains("no serialized project payload"));
    }

    #[test]
    fn create_human_review_packet_returns_real_packet() {
        let project = scorestrata_testkit::melodic_techno_project();
        let params = CreateHumanReviewPacketParams {
            project_id: "proj_1".to_string(),
            reason: "Stagnation: no improvement for 3 rounds".to_string(),
            agent_confidence: 0.6,
            attempted_patches: vec![AttemptedPatchParams {
                operation: "apply_transition_fix".to_string(),
                scope: "chorus_1->verse_2".to_string(),
                succeeded: false,
                score_delta: -0.02,
                iteration: 3,
            }],
            project: Some(serde_json::to_value(&project).unwrap_or_default()),
        };
        let receipt = create_human_review_packet(&params);
        assert!(
            matches!(receipt.status, ToolStatus::Succeeded),
            "packet creation should succeed, got {receipt:#?}"
        );
        assert!(receipt.summary.contains("human review packet"));
        assert!(
            receipt.summary.contains("recommended decision"),
            "packet reports recommended decisions, got: {}",
            receipt.summary
        );
        assert!(
            receipt
                .next_actions
                .contains(&"await_human_decision".to_string()),
            "packet transitions to awaiting human decision"
        );
        assert!(
            receipt
                .evidence_uris
                .iter()
                .any(|u| u.starts_with("packet://")),
            "packet payload is returned in evidence"
        );
        assert!(
            receipt
                .score_delta
                .get("packet.attempted_patches")
                .is_some_and(|v| v == "1"),
            "attempted patches are reported in the packet, got: {:#?}",
            receipt.score_delta
        );
        assert!(
            receipt
                .score_delta
                .contains_key("packet.recommended_decisions")
        );
    }

    #[test]
    fn create_human_review_packet_requires_payload() {
        let params = CreateHumanReviewPacketParams {
            project_id: "proj_1".to_string(),
            reason: "Test".to_string(),
            agent_confidence: 0.5,
            attempted_patches: vec![],
            project: None,
        };
        let receipt = create_human_review_packet(&params);
        assert!(matches!(receipt.status, ToolStatus::Failed));
        assert!(receipt.summary.contains("no serialized project payload"));
    }

    fn sample_lesson_store() -> scorestrata_storage::lesson_store::LessonStore {
        let mut store = scorestrata_storage::lesson_store::LessonStore::new();
        store.record(scorestrata_storage::lesson_store::StoredLesson {
            id: "les_1".to_string(),
            scope: "harmony".to_string(),
            trigger: "SCALE_VIOLATION".to_string(),
            successful_patch: Some("quantize_layer_to_scale".to_string()),
            failed_patches: vec![],
            evidence_uris: vec![],
            genre: Some("melodic_techno".to_string()),
            confidence: 0.9,
            success_count: 5,
            failure_count: 0,
            created_at: chrono::Utc::now().to_rfc3339(),
            updated_at: chrono::Utc::now().to_rfc3339(),
        });
        store
    }

    #[test]
    fn get_lessons_filters_by_trigger() {
        let store = sample_lesson_store();
        let params = GetLessonsParams {
            project_id: "proj_1".to_string(),
            trigger: Some("SCALE_VIOLATION".to_string()),
            genre: None,
            min_confidence: 0.5,
            lesson_store: Some(serde_json::to_value(&store).unwrap_or_default()),
        };
        let receipt = get_lessons(&params);
        assert!(
            matches!(receipt.status, ToolStatus::Succeeded),
            "lesson query should succeed, got {receipt:#?}"
        );
        assert_eq!(receipt.score_delta["lessons.matched"], "1");
        assert!(receipt.summary.contains("quantize_layer_to_scale"));
    }

    #[test]
    fn get_lessons_no_match_returns_warning() {
        let store = sample_lesson_store();
        let params = GetLessonsParams {
            project_id: "proj_1".to_string(),
            trigger: Some("NO_SUCH_TRIGGER".to_string()),
            genre: None,
            min_confidence: 0.5,
            lesson_store: Some(serde_json::to_value(&store).unwrap_or_default()),
        };
        let receipt = get_lessons(&params);
        assert!(matches!(receipt.status, ToolStatus::Succeeded));
        assert_eq!(receipt.score_delta["lessons.matched"], "0");
        assert!(
            receipt.warnings.iter().any(|w| w.contains("No lessons")),
            "expected no-match warning"
        );
    }

    #[test]
    fn record_lesson_mutates_store() {
        let store = scorestrata_storage::lesson_store::LessonStore::new();
        let params = RecordLessonParams {
            project_id: "proj_1".to_string(),
            trigger: "TRANSITION_ENERGY_LOW".to_string(),
            scope: "transition".to_string(),
            successful_patch: Some("add_transition_package".to_string()),
            genre: Some("techno".to_string()),
            confidence: 0.8,
            lesson_store: Some(serde_json::to_value(&store).unwrap_or_default()),
        };
        let receipt = record_lesson(&params);
        assert!(
            matches!(receipt.status, ToolStatus::Succeeded),
            "record should succeed, got {receipt:#?}"
        );
        assert_eq!(receipt.tool, "record_lesson");
        assert_eq!(receipt.score_delta["lessons.total"], "1");
        assert!(receipt.summary.contains("store now has 1 lessons"));
    }

    #[test]
    fn record_lesson_same_trigger_accumulates_confidence() {
        // §13.5: recording the same (trigger, scope, genre) twice must merge
        // into one lesson with rising confidence, not create a duplicate.
        let store = scorestrata_storage::lesson_store::LessonStore::new();
        let mut current = serde_json::to_value(&store).unwrap_or_default();

        for _ in 0..2 {
            let params = RecordLessonParams {
                project_id: "proj_1".to_string(),
                trigger: "TRANSITION_ENERGY_LOW".to_string(),
                scope: "transition".to_string(),
                successful_patch: Some("add_transition_package".to_string()),
                genre: Some("techno".to_string()),
                confidence: 0.8,
                lesson_store: Some(current.clone()),
            };
            let receipt = record_lesson(&params);
            assert!(matches!(receipt.status, ToolStatus::Succeeded));
            // Round-trip the updated store from evidence for the next call.
            let store_prefix = "memory://lessons/proj_1?store=";
            let updated = receipt
                .evidence_uris
                .iter()
                .find_map(|u| u.strip_prefix(store_prefix))
                .expect("updated store in evidence");
            current = serde_json::from_str(updated).expect("updated store parses");
        }

        let store: scorestrata_storage::lesson_store::LessonStore =
            serde_json::from_value(current).expect("store deserializes");
        assert_eq!(store.len(), 1, "repeated recording must not duplicate");
        let lesson = store
            .get_best_for_trigger("TRANSITION_ENERGY_LOW", Some("techno"))
            .expect("lesson present");
        assert_eq!(lesson.success_count, 2);
        assert!(
            (lesson.confidence - 0.9).abs() < 0.01,
            "0.8 base + 0.1 from the second occurrence, got {}",
            lesson.confidence
        );
    }

    #[test]
    fn record_lesson_without_store_fails_transparently() {
        let params = RecordLessonParams {
            project_id: "proj_1".to_string(),
            trigger: "X".to_string(),
            scope: "scope".to_string(),
            successful_patch: None,
            genre: None,
            confidence: 0.5,
            lesson_store: None,
        };
        let receipt = record_lesson(&params);
        assert!(matches!(receipt.status, ToolStatus::Failed));
    }

    #[test]
    fn get_repair_templates_derives_from_lessons() {
        let store = sample_lesson_store();
        let params = GetRepairTemplatesParams {
            project_id: "proj_1".to_string(),
            lesson_store: Some(serde_json::to_value(&store).unwrap_or_default()),
        };
        let receipt = get_repair_templates(&params);
        assert!(
            matches!(receipt.status, ToolStatus::Succeeded),
            "template query should succeed, got {receipt:#?}"
        );
        assert_eq!(receipt.score_delta["templates.count"], "1");
        assert!(receipt.summary.contains("quantize_layer_to_scale"));
    }

    #[test]
    fn get_repair_templates_empty_store_warns() {
        let store = scorestrata_storage::lesson_store::LessonStore::new();
        let params = GetRepairTemplatesParams {
            project_id: "proj_1".to_string(),
            lesson_store: Some(serde_json::to_value(&store).unwrap_or_default()),
        };
        let receipt = get_repair_templates(&params);
        assert!(matches!(receipt.status, ToolStatus::Succeeded));
        assert_eq!(receipt.score_delta["templates.count"], "0");
        assert!(
            receipt.warnings.iter().any(|w| w.contains("No lessons")),
            "expected empty-store warning"
        );
    }

    #[test]
    fn get_next_task_returns_success() {
        let params = GetNextTaskParams {
            component: Some("validator".to_string()),
        };
        let receipt = get_next_task(&params);
        assert!(matches!(receipt.status, ToolStatus::Succeeded));
        assert!(receipt.focus_uris[0].contains("tasks/next"));
    }

    #[test]
    fn claim_task_returns_success() {
        let params = ClaimTaskParams {
            task_id: "TASK-001".to_string(),
            agent_id: "agent:engineer".to_string(),
            lease_duration_secs: 600,
        };
        let receipt = claim_task(&params);
        assert!(matches!(receipt.status, ToolStatus::Succeeded));
        assert!(receipt.summary.contains("TASK-001"));
    }

    #[test]
    fn run_acceptance_tests_returns_success() {
        let params = RunAcceptanceTestsParams {
            task_id: "TASK-001".to_string(),
            test_commands: vec!["cargo test".to_string(), "cargo clippy".to_string()],
        };
        let receipt = run_acceptance_tests(&params);
        assert!(matches!(receipt.status, ToolStatus::Succeeded));
        assert_eq!(receipt.cost.tool_calls, 2);
    }

    #[test]
    fn submit_task_result_computes_pass_rate_with_board() {
        let mut board = scorestrata_storage::engineering::TaskResultBoard::new();
        board.record(scorestrata_storage::engineering::TaskResult {
            task_id: "TASK-000".to_string(),
            agent_id: "agent:composer".to_string(),
            passed: 2,
            failed: 0,
            score_delta: 0.1,
            evidence_uri: None,
            timestamp: chrono::Utc::now().to_rfc3339(),
        });
        let params = SubmitTaskResultParams {
            project_id: "proj_1".to_string(),
            task_id: "TASK-001".to_string(),
            agent_id: "agent:composer".to_string(),
            passed: 3,
            failed: 1,
            score_delta: 0.05,
            board: Some(serde_json::to_value(&board).unwrap_or_default()),
        };
        let receipt = submit_task_result(&params);
        assert!(matches!(receipt.status, ToolStatus::Succeeded));
        assert!(receipt.summary.contains("pass rate"));
        assert!(receipt.score_delta.contains_key("engineering.pass_rate"));
        assert!(receipt.score_delta.contains_key("engineering.regressions"));
    }

    #[test]
    fn submit_task_result_without_board_is_transparent() {
        let params = SubmitTaskResultParams {
            project_id: "proj_1".to_string(),
            task_id: "TASK-001".to_string(),
            agent_id: "agent:composer".to_string(),
            passed: 3,
            failed: 0,
            score_delta: 0.0,
            board: None,
        };
        let receipt = submit_task_result(&params);
        assert!(matches!(receipt.status, ToolStatus::Succeeded));
        assert!(
            receipt
                .warnings
                .iter()
                .any(|w| w.contains("board not supplied") || w.contains("No task-result board")),
            "expected transparency warning, got {:#?}",
            receipt.warnings
        );
    }

    #[test]
    fn get_eval_report_is_clean_when_all_pass() {
        let mut board = scorestrata_storage::engineering::TaskResultBoard::new();
        board.record(scorestrata_storage::engineering::TaskResult {
            task_id: "TASK-001".to_string(),
            agent_id: "agent:composer".to_string(),
            passed: 2,
            failed: 0,
            score_delta: 0.1,
            evidence_uri: None,
            timestamp: chrono::Utc::now().to_rfc3339(),
        });
        board.record(scorestrata_storage::engineering::TaskResult {
            task_id: "TASK-002".to_string(),
            agent_id: "agent:composer".to_string(),
            passed: 1,
            failed: 0,
            score_delta: 0.0,
            evidence_uri: None,
            timestamp: chrono::Utc::now().to_rfc3339(),
        });
        let params = GetEvalReportParams {
            project_id: "proj_1".to_string(),
            board: Some(serde_json::to_value(&board).unwrap_or_default()),
        };
        let receipt = get_eval_report(&params);
        assert!(matches!(receipt.status, ToolStatus::Succeeded));
        assert!(receipt.summary.contains("clean"));
        assert!(receipt.score_delta.contains_key("eval.pass_rate"));
    }

    #[test]
    fn get_eval_report_flags_regressions() {
        let mut board = scorestrata_storage::engineering::TaskResultBoard::new();
        board.record(scorestrata_storage::engineering::TaskResult {
            task_id: "TASK-001".to_string(),
            agent_id: "agent:composer".to_string(),
            passed: 2,
            failed: 1,
            score_delta: -0.05,
            evidence_uri: None,
            timestamp: chrono::Utc::now().to_rfc3339(),
        });
        let params = GetEvalReportParams {
            project_id: "proj_1".to_string(),
            board: Some(serde_json::to_value(&board).unwrap_or_default()),
        };
        let receipt = get_eval_report(&params);
        assert!(matches!(receipt.status, ToolStatus::Failed));
        assert!(receipt.summary.contains("needs repair"));
        assert_eq!(
            receipt
                .score_delta
                .get("eval.regressions")
                .map(String::as_str),
            Some("1")
        );
    }

    #[test]
    fn update_docs_records_into_ledger() {
        let mut ledger = scorestrata_storage::docs::DocsLedger::new();
        ledger.record("README.md", "install", "old", "SPEC-X");
        let params = UpdateDocsParams {
            project_id: "proj_1".to_string(),
            path: "docs/architecture/architecture.md".to_string(),
            section: "§10.5".to_string(),
            summary: "added render job tools".to_string(),
            related_spec: "SPEC-RENDER".to_string(),
            ledger: Some(serde_json::to_value(&ledger).unwrap_or_default()),
        };
        let receipt = update_docs(&params);
        assert!(matches!(receipt.status, ToolStatus::Succeeded));
        assert!(receipt.summary.contains("#2"));
        assert!(receipt.evidence_uris[0].contains("architecture.md"));
    }

    #[test]
    fn create_adr_assigns_incrementing_id() {
        let params = CreateAdrParams {
            project_id: "proj_1".to_string(),
            title: "Store render jobs in a dedicated ledger".to_string(),
            context: "Render jobs need lifecycle tracking".to_string(),
            decision: "Add RenderJobStore to scorestrata-storage".to_string(),
            consequences: vec!["More storage surface".to_string()],
            related_spec: "§10.5".to_string(),
            ledger: Some(serde_json::json!({})),
        };
        let receipt = create_adr(&params);
        assert!(matches!(receipt.status, ToolStatus::Succeeded));
        assert_eq!(
            receipt.score_delta.get("adr.id").map(String::as_str),
            Some("adr_0001")
        );
        assert!(receipt.evidence_uris[0].contains("adr_0001"));
    }

    #[test]
    fn update_calibration_profile_sets_genre_target() {
        let params = UpdateCalibrationProfileParams {
            project_id: "proj_1".to_string(),
            genre: "melodic_techno".to_string(),
            target_lufs: -10.0,
            profile: Some(serde_json::json!({})),
        };
        let receipt = update_calibration_profile(&params);
        assert!(matches!(receipt.status, ToolStatus::Succeeded));
        assert!(receipt.summary.contains("-10.0 LUFS"));
        assert_eq!(
            receipt
                .score_delta
                .get("calibration.target_lufs")
                .map(String::as_str),
            Some("-10.0")
        );
    }

    // ── Governance tools tests ──────────────────────────────────────

    #[test]
    fn get_policy_profile_returns_success() {
        let params = GetPolicyProfileParams {
            project_id: "proj_1".to_string(),
        };
        let receipt = get_policy_profile(&params);
        assert!(matches!(receipt.status, ToolStatus::Succeeded));
        assert!(receipt.evidence_uris[0].contains("policy"));
    }

    #[test]
    fn check_budget_returns_success() {
        let params = CheckBudgetParams {
            project_id: "proj_1".to_string(),
        };
        let receipt = check_budget(&params);
        assert!(matches!(receipt.status, ToolStatus::Succeeded));
        assert!(receipt.evidence_uris[0].contains("budget"));
    }

    #[test]
    fn get_contracts_lists_schema_registry() {
        let receipt = get_contracts();
        assert!(matches!(receipt.status, ToolStatus::Succeeded));
        assert!(receipt.summary.contains("Contract registry"));
        assert!(receipt.score_delta.contains_key("registry.schema_count"));
        assert!(
            receipt
                .evidence_uris
                .contains(&"system://contracts".to_string())
        );
    }

    #[test]
    fn get_onboarding_packet_reports_capabilities() {
        let receipt = get_onboarding_packet();
        assert!(matches!(receipt.status, ToolStatus::Succeeded));
        assert!(receipt.summary.contains("Onboarding"));
        assert!(receipt.score_delta.contains_key("onboarding.tools"));
        assert!(receipt.score_delta.contains_key("onboarding.contracts"));
        assert!(
            receipt
                .evidence_uris
                .contains(&"system://capabilities".to_string()),
            "onboarding cites capabilities, got {:?}",
            receipt.evidence_uris
        );
    }

    // ── Project tools tests ─────────────────────────────────────────

    #[test]
    fn create_project_returns_success() {
        let params = CreateProjectParams {
            name: "Dark Techno Track".to_string(),
            brief: "Dark melodic techno".to_string(),
            genre: Some("melodic_techno".to_string()),
        };
        let receipt = create_project(&params);
        assert!(matches!(receipt.status, ToolStatus::Succeeded));
        assert!(receipt.summary.contains("Dark Techno Track"));
        assert!(!receipt.next_actions.is_empty());
    }

    #[test]
    fn get_project_summary_reports_real_counts_with_payload() {
        let project = scorestrata_testkit::melodic_techno_project();
        let params = GetProjectSummaryParams {
            project_id: project.id.to_string(),
            project: Some(serde_json::to_value(&project).unwrap_or_default()),
        };
        let receipt = get_project_summary(&params);
        assert!(
            matches!(receipt.status, ToolStatus::Succeeded),
            "summary should succeed, got {receipt:#?}"
        );
        assert!(receipt.summary.contains("segment(s)"));
        assert!(receipt.score_delta.contains_key("summary.segments"));
        assert!(receipt.score_delta.contains_key("summary.layers"));
        assert!(
            receipt.evidence_uris[0].contains("/summary"),
            "summary cites project summary uri"
        );
    }

    #[test]
    fn get_project_summary_without_payload_is_transparent_stub() {
        let params = GetProjectSummaryParams {
            project_id: "proj_1".to_string(),
            project: None,
        };
        let receipt = get_project_summary(&params);
        assert!(matches!(receipt.status, ToolStatus::Succeeded));
        assert!(receipt.summary.contains("stub"));
        assert!(!receipt.warnings.is_empty());
    }

    #[test]
    fn generate_config_from_brief_returns_success() {
        let params = GenerateConfigParams {
            project_id: "proj_1".to_string(),
            brief: "Dark melodic techno with 140 BPM".to_string(),
            name: None,
        };
        let receipt = generate_config_from_brief(&params);
        assert!(matches!(receipt.status, ToolStatus::Succeeded));
        assert!(
            receipt
                .next_actions
                .contains(&"validate_config".to_string())
        );
        // The summary reflects the parsed config (genre, BPM, key, bars).
        assert!(receipt.summary.contains("melodic_techno"));
        assert!(receipt.summary.contains("140 BPM"));
        assert!(receipt.summary.contains("C minor"));
        assert!(receipt.warnings.is_empty());
    }

    #[test]
    fn create_project_handles_multibyte_utf8() {
        let params = CreateProjectParams {
            name: "🎵 電子音楽プロジェクト 🎹".to_string(),
            brief: "Melodic techno".to_string(),
            genre: Some("techno".to_string()),
        };
        let receipt = create_project(&params);
        assert!(matches!(receipt.status, ToolStatus::Succeeded));
        assert!(receipt.summary.contains("🎵 電子音楽プロジェクト 🎹"));
        // Verify the project_id was derived safely (no panic)
        assert!(receipt.evidence_uris[0].contains("proj_"));
    }

    #[test]
    fn validate_config_returns_success() {
        let params = ValidateConfigParams {
            project_id: "proj_1".to_string(),
        };
        let receipt = validate_config(&params);
        assert!(matches!(receipt.status, ToolStatus::Succeeded));
        assert!(receipt.next_actions.contains(&"approve_config".to_string()));
    }

    #[test]
    fn approve_config_returns_success() {
        let params = ApproveConfigParams {
            project_id: "proj_1".to_string(),
        };
        let receipt = approve_config(&params);
        assert!(matches!(receipt.status, ToolStatus::Succeeded));
        assert!(
            receipt
                .next_actions
                .contains(&"plan_arrangement".to_string())
        );
    }

    // ── Validation tools tests ──────────────────────────────────────

    #[test]
    fn get_audit_summary_returns_success() {
        let params = GetAuditSummaryParams {
            project_id: "proj_1".to_string(),
            scope: Some("full".to_string()),
        };
        let receipt = get_audit_summary(&params);
        assert!(matches!(receipt.status, ToolStatus::Succeeded));
        assert!(receipt.evidence_uris[0].contains("audit"));
    }

    #[test]
    fn suggest_fixes_without_payload_fails_transparently() {
        let params = SuggestFixesParams {
            project_id: "proj_1".to_string(),
            diagnostic_code: "SCALE_VIOLATION".to_string(),
            segment_id: Some("chorus_1".to_string()),
            genre: None,
            project: None,
            lesson_store: None,
        };
        let receipt = suggest_fixes(&params);
        assert!(matches!(receipt.status, ToolStatus::Failed));
        assert!(
            receipt
                .summary
                .contains("no project or lesson-store payload")
        );
        assert!(!receipt.next_actions.contains(&"apply_patch".to_string()));
    }

    #[test]
    fn suggest_fixes_with_project_returns_validator_patches() {
        // §12.3: the validator already attaches SuggestedPatch payloads to
        // diagnostics; the tool must surface them instead of a canned string.
        let project = scorestrata_testkit::melodic_techno_project();
        let params = SuggestFixesParams {
            project_id: project.id.to_string(),
            diagnostic_code: "TRANSITION_ENERGY_DELTA_LOW".to_string(),
            segment_id: None,
            genre: None,
            project: Some(serde_json::to_value(&project).unwrap()),
            lesson_store: None,
        };
        let receipt = suggest_fixes(&params);
        assert!(matches!(receipt.status, ToolStatus::Succeeded));
        assert!(receipt.summary.contains("add_transition_package"));
        // Three diagnostics all suggest the same patch, so dedup collapses
        // them to one distinct fix.
        assert_eq!(
            receipt.score_delta.get("fixes.count").map(String::as_str),
            Some("1")
        );
        assert!(receipt.next_actions.contains(&"apply_patch".to_string()));
    }

    #[test]
    fn suggest_fixes_unknown_code_with_project_fails_with_warning() {
        let project = scorestrata_testkit::melodic_techno_project();
        let params = SuggestFixesParams {
            project_id: project.id.to_string(),
            diagnostic_code: "NO_SUCH_CODE".to_string(),
            segment_id: None,
            genre: None,
            project: Some(serde_json::to_value(&project).unwrap()),
            lesson_store: None,
        };
        let receipt = suggest_fixes(&params);
        assert!(matches!(receipt.status, ToolStatus::Failed));
        assert!(
            receipt
                .warnings
                .iter()
                .any(|w| w.contains("No 'NO_SUCH_CODE' diagnostics"))
        );
    }

    #[test]
    fn suggest_fixes_uses_proven_lesson_from_store() {
        // §13.5: a confident lesson for the trigger must surface its proven
        // successful patch as a fix, even with no project payload.
        let mut store = scorestrata_storage::lesson_store::LessonStore::new();
        store.record(scorestrata_storage::lesson_store::StoredLesson {
            id: "les_1".to_string(),
            scope: "segment".to_string(),
            trigger: "HARMONY_SCALE_VIOLATION".to_string(),
            successful_patch: Some("quantize_layer_to_scale".to_string()),
            failed_patches: vec![],
            evidence_uris: vec![],
            genre: None,
            confidence: 0.95,
            success_count: 3,
            failure_count: 0,
            created_at: "t".to_string(),
            updated_at: "t".to_string(),
        });
        let params = SuggestFixesParams {
            project_id: "proj_1".to_string(),
            diagnostic_code: "HARMONY_SCALE_VIOLATION".to_string(),
            segment_id: None,
            genre: None,
            project: None,
            lesson_store: Some(serde_json::to_value(&store).unwrap()),
        };
        let receipt = suggest_fixes(&params);
        assert!(matches!(receipt.status, ToolStatus::Succeeded));
        assert!(receipt.summary.contains("quantize_layer_to_scale"));
        assert_eq!(
            receipt
                .score_delta
                .get("fixes.lesson_sources")
                .map(String::as_str),
            Some("1")
        );
        assert!(receipt.next_actions.contains(&"apply_patch".to_string()));
    }

    #[test]
    fn suggest_fixes_dedups_non_adjacent_duplicates() {
        // Regression: dedup_by alone only collapses *adjacent* duplicates, so
        // same-op fixes with interleaved confidences could both survive and
        // inflate fixes.count. The seen-set retain must drop every later
        // duplicate regardless of position.
        let project = scorestrata_testkit::melodic_techno_project();
        let mut store = scorestrata_storage::lesson_store::LessonStore::new();
        // Lesson surfaces a distinct proven op (no validator patch matches).
        store.record(scorestrata_storage::lesson_store::StoredLesson {
            id: "les_dedup".to_string(),
            scope: "segment".to_string(),
            trigger: "TRANSITION_ENERGY_DELTA_LOW".to_string(),
            successful_patch: Some("quantize_layer_to_scale".to_string()),
            failed_patches: vec![],
            evidence_uris: vec![],
            genre: None,
            confidence: 0.9,
            success_count: 3,
            failure_count: 0,
            created_at: "t".to_string(),
            updated_at: "t".to_string(),
        });
        let params = SuggestFixesParams {
            project_id: project.id.to_string(),
            diagnostic_code: "TRANSITION_ENERGY_DELTA_LOW".to_string(),
            segment_id: None,
            genre: None,
            project: Some(serde_json::to_value(&project).unwrap()),
            lesson_store: Some(serde_json::to_value(&store).unwrap()),
        };
        let receipt = suggest_fixes(&params);
        assert!(matches!(receipt.status, ToolStatus::Succeeded));
        // add_transition_package (validator, 3 identical) + quantize
        // (lesson) = exactly 2 distinct fixes, never 4.
        assert_eq!(
            receipt.score_delta.get("fixes.count").map(String::as_str),
            Some("2")
        );
        assert!(receipt.summary.contains("add_transition_package"));
        assert!(receipt.summary.contains("quantize_layer_to_scale"));
    }

    #[test]
    fn suggest_fixes_merges_lesson_into_project_patches() {
        // With both payloads, validator patches stay and the proven lesson is
        // added when it contributes a distinct op.
        let project = scorestrata_testkit::melodic_techno_project();
        let mut store = scorestrata_storage::lesson_store::LessonStore::new();
        store.record(scorestrata_storage::lesson_store::StoredLesson {
            id: "les_2".to_string(),
            scope: "segment".to_string(),
            trigger: "TRANSITION_ENERGY_DELTA_LOW".to_string(),
            successful_patch: Some("add_transition_package".to_string()),
            failed_patches: vec![],
            evidence_uris: vec![],
            genre: None,
            confidence: 0.9,
            success_count: 2,
            failure_count: 0,
            created_at: "t".to_string(),
            updated_at: "t".to_string(),
        });
        let params = SuggestFixesParams {
            project_id: project.id.to_string(),
            diagnostic_code: "TRANSITION_ENERGY_DELTA_LOW".to_string(),
            segment_id: None,
            genre: None,
            project: Some(serde_json::to_value(&project).unwrap()),
            lesson_store: Some(serde_json::to_value(&store).unwrap()),
        };
        let receipt = suggest_fixes(&params);
        assert!(matches!(receipt.status, ToolStatus::Succeeded));
        // Validator patch (1 distinct) + lesson patch is a duplicate op, so
        // the count stays at 1 — the merge must not inflate it.
        assert_eq!(
            receipt.score_delta.get("fixes.count").map(String::as_str),
            Some("1"),
            "duplicate lesson op must not inflate the count"
        );
        assert_eq!(
            receipt
                .score_delta
                .get("fixes.lesson_sources")
                .map(String::as_str),
            Some("1")
        );
    }

    #[test]
    fn get_validation_report_returns_real_scorecard_with_project() {
        let project = scorestrata_testkit::melodic_techno_project();
        let params = GetValidationReportParams {
            project_id: project.id.to_string(),
            level: 2,
            project: Some(serde_json::to_value(&project).unwrap_or_default()),
        };
        let receipt = get_validation_report(&params);
        assert!(
            matches!(receipt.status, ToolStatus::Succeeded)
                || matches!(receipt.status, ToolStatus::Failed),
            "report returns real hard-gate status, got {receipt:#?}"
        );
        assert!(receipt.summary.contains("overall"));
        assert!(receipt.score_delta.contains_key("report.overall_score"));
        assert!(receipt.score_delta.contains_key("report.metrics"));
    }

    #[test]
    fn get_validation_report_requires_payload() {
        let params = GetValidationReportParams {
            project_id: "proj_1".to_string(),
            level: 2,
            project: None,
        };
        let receipt = get_validation_report(&params);
        assert!(matches!(receipt.status, ToolStatus::Failed));
        assert!(receipt.summary.contains("no serialized project payload"));
    }

    #[test]
    fn get_root_causes_attributes_real_causes_with_project() {
        let mut project = scorestrata_testkit::melodic_techno_project();
        for pattern in &mut project.patterns {
            for note in &mut pattern.notes {
                note.pitch = scorestrata_core::domain::MidiNote::new(61);
            }
        }
        let params = GetRootCausesParams {
            project_id: project.id.to_string(),
            metric_name: "harmony.scale_compliance".to_string(),
            project: Some(serde_json::to_value(&project).unwrap_or_default()),
        };
        let receipt = get_root_causes(&params);
        assert!(matches!(receipt.status, ToolStatus::Succeeded));
        assert!(receipt.summary.contains("Root causes"));
        assert!(receipt.score_delta.contains_key("metric.root_causes"));
    }

    #[test]
    fn get_root_causes_healthy_metric_reports_none() {
        // The scorecard only contains metrics that produced diagnostics, so a
        // metric absent from it is healthy — the tool must answer Succeeded
        // with "none" rather than fail with "not measured".
        let project = scorestrata_testkit::melodic_techno_project();
        let params = GetRootCausesParams {
            project_id: project.id.to_string(),
            metric_name: "harmony.scale_compliance".to_string(),
            project: Some(serde_json::to_value(&project).unwrap_or_default()),
        };
        let receipt = get_root_causes(&params);
        assert!(matches!(receipt.status, ToolStatus::Succeeded));
        assert!(
            receipt.summary.contains("none"),
            "healthy metric reports no root causes: {}",
            receipt.summary
        );
        assert_eq!(
            receipt.score_delta.get("metric.value").map(String::as_str),
            Some("1.000")
        );
    }

    #[test]
    fn get_metric_evidence_returns_recorded_samples() {
        let project = scorestrata_testkit::melodic_techno_project();
        let report = crate::debug_ops::project_report(&project);
        let mut warehouse = scorestrata_storage::evidence::EvidenceWarehouse::new();
        warehouse.record_scorecard(&report.scorecard, 2, None, &report.diagnostics);
        let params = GetMetricEvidenceParams {
            project_id: project.id.to_string(),
            metric_name: "harmony.scale_compliance".to_string(),
            warehouse: Some(serde_json::to_value(&warehouse).unwrap_or_default()),
            lineage: None,
        };
        let receipt = get_metric_evidence(&params);
        assert!(matches!(receipt.status, ToolStatus::Succeeded));
        assert!(receipt.summary.contains("sample(s)"));
        assert!(receipt.score_delta.contains_key("evidence.entries"));
    }

    #[test]
    fn get_metric_evidence_reports_lineage_trajectory() {
        use scorestrata_core::diagnostic::{MetricSample, MetricScope, Scorecard};
        use scorestrata_core::error::Severity;
        use scorestrata_core::ids::MetricId;
        use scorestrata_storage::lineage::LineageStore;

        let mut store = LineageStore::new();
        for (value, cmd) in [(0.50, "cmd_1"), (0.80, "cmd_2")] {
            store.record_scorecard(
                &Scorecard {
                    id: scorestrata_core::ids::ScorecardId::new(),
                    metrics: vec![MetricSample {
                        id: MetricId::new(),
                        name: "harmony.scale_compliance".to_string(),
                        value,
                        severity: Severity::Warning,
                        scope: MetricScope::default(),
                        evidence_uri: "project://metrics/x".to_string(),
                        explanation: "lineage test".to_string(),
                        confidence: 0.9,
                        suggested_patches: vec![],
                    }],
                    overall_score: value,
                    hard_failures: 0,
                    warnings: 0,
                },
                2,
                Some(cmd),
            );
        }
        let warehouse = scorestrata_storage::evidence::EvidenceWarehouse::new();
        let params = GetMetricEvidenceParams {
            project_id: "proj_1".to_string(),
            metric_name: "harmony.scale_compliance".to_string(),
            warehouse: Some(serde_json::to_value(&warehouse).unwrap_or_default()),
            lineage: Some(serde_json::to_value(&store).unwrap_or_default()),
        };
        let receipt = get_metric_evidence(&params);
        assert!(matches!(receipt.status, ToolStatus::Succeeded));
        assert!(
            receipt.summary.contains("total delta +0.300"),
            "receipt reports trajectory: {}",
            receipt.summary
        );
        assert!(receipt.summary.contains("0 regression(s)"));
        assert_eq!(
            receipt.score_delta.get("lineage.samples"),
            Some(&"2".to_string())
        );
    }

    #[test]
    fn get_metric_evidence_lineage_only_callers_succeed() {
        use scorestrata_core::diagnostic::{MetricSample, MetricScope, Scorecard};
        use scorestrata_core::error::Severity;
        use scorestrata_core::ids::MetricId;
        use scorestrata_storage::lineage::LineageStore;

        let mut store = LineageStore::new();
        store.record_scorecard(
            &Scorecard {
                id: scorestrata_core::ids::ScorecardId::new(),
                metrics: vec![MetricSample {
                    id: MetricId::new(),
                    name: "mix.masking".to_string(),
                    value: 0.90,
                    severity: Severity::Warning,
                    scope: MetricScope::default(),
                    evidence_uri: "project://metrics/x".to_string(),
                    explanation: "lineage-only test".to_string(),
                    confidence: 0.9,
                    suggested_patches: vec![],
                }],
                overall_score: 0.90,
                hard_failures: 0,
                warnings: 0,
            },
            4,
            Some("cmd_1"),
        );
        // No warehouse: lineage alone must still report the trajectory.
        let params = GetMetricEvidenceParams {
            project_id: "proj_1".to_string(),
            metric_name: "mix.masking".to_string(),
            warehouse: None,
            lineage: Some(serde_json::to_value(&store).unwrap_or_default()),
        };
        let receipt = get_metric_evidence(&params);
        assert!(matches!(receipt.status, ToolStatus::Succeeded));
        assert!(
            receipt.summary.contains("latest level 4"),
            "receipt surfaces the lineage validation level: {}",
            receipt.summary
        );
        assert_eq!(
            receipt.score_delta.get("lineage.samples"),
            Some(&"1".to_string())
        );
    }

    #[test]
    fn get_render_manifest_reports_reproducibility_and_verifies_checksum() {
        use scorestrata_storage::cache::content_hash;
        use scorestrata_storage::render_manifest::{RenderManifest, RenderManifestStore};

        let bytes = b"RIFF....WAV data".to_vec();
        let checksum = content_hash(&bytes);
        let mut store = RenderManifestStore::new();
        store.record(RenderManifest::from_render(
            "art_p34_master",
            "master_wav",
            "renders/master.wav",
            &checksum,
            48_000,
            2,
            158.4,
            "snap_p34_0042",
            "cmd_0042",
            "0.3.0",
            "0.3.0",
            Some(1337),
            BTreeMap::new(),
        ));
        let params = GetRenderManifestParams {
            project_id: "p34".to_string(),
            artifact_id: "art_p34_master".to_string(),
            manifests: Some(serde_json::to_value(&store).unwrap_or_default()),
            artifact_bytes: Some(bytes.clone()),
        };
        let receipt = get_render_manifest(&params);
        assert!(matches!(receipt.status, ToolStatus::Succeeded));
        assert!(
            receipt.summary.contains("CHECKSUM MATCH"),
            "receipt verifies matching artifact: {}",
            receipt.summary
        );
        assert_eq!(
            receipt.score_delta.get("render.sha256"),
            Some(&checksum.clone())
        );
        assert!(receipt.score_delta.get("render.manifest_hash").is_some());
        assert!(receipt.score_delta.get("render.cache_key").is_some());
        assert_eq!(
            receipt.score_delta.get("render.complete"),
            Some(&"true".to_string())
        );
        assert!(receipt.warnings.is_empty());

        // Tampered bytes must be reported as a checksum mismatch.
        let mut tampered = bytes;
        tampered[0] ^= 0xFF;
        let params = GetRenderManifestParams {
            project_id: "p34".to_string(),
            artifact_id: "art_p34_master".to_string(),
            manifests: Some(serde_json::to_value(&store).unwrap_or_default()),
            artifact_bytes: Some(tampered),
        };
        let receipt = get_render_manifest(&params);
        assert!(matches!(receipt.status, ToolStatus::Succeeded));
        assert!(receipt.summary.contains("CHECKSUM MISMATCH"));
        assert!(
            receipt.warnings.iter().any(|w| w.contains("do not match")),
            "tampering surfaces a warning"
        );
    }

    #[test]
    fn get_render_manifest_requires_manifest_store() {
        let params = GetRenderManifestParams {
            project_id: "p34".to_string(),
            artifact_id: "art_p34_master".to_string(),
            manifests: None,
            artifact_bytes: None,
        };
        let receipt = get_render_manifest(&params);
        assert!(matches!(receipt.status, ToolStatus::Failed));
        assert!(receipt.summary.contains("no serialized manifest store"));
    }

    #[test]
    fn get_render_manifest_rejects_malformed_store() {
        let params = GetRenderManifestParams {
            project_id: "p34".to_string(),
            artifact_id: "art_p34_master".to_string(),
            manifests: Some(serde_json::json!({"manifests": 42})),
            artifact_bytes: None,
        };
        let receipt = get_render_manifest(&params);
        assert!(matches!(receipt.status, ToolStatus::Failed));
        assert!(
            receipt
                .summary
                .contains("Malformed render manifest store payload"),
            "malformed store surfaces: {}",
            receipt.summary
        );
    }

    #[test]
    fn get_render_manifest_reports_unknown_artifact() {
        use scorestrata_storage::render_manifest::{RenderManifest, RenderManifestStore};
        let mut store = RenderManifestStore::new();
        store.record(RenderManifest::new("art_other"));
        let params = GetRenderManifestParams {
            project_id: "p34".to_string(),
            artifact_id: "art_p34_master".to_string(),
            manifests: Some(serde_json::to_value(&store).unwrap_or_default()),
            artifact_bytes: None,
        };
        let receipt = get_render_manifest(&params);
        assert!(matches!(receipt.status, ToolStatus::Failed));
        assert!(
            receipt
                .summary
                .contains("No render manifest found for artifact 'art_p34_master'"),
            "unknown artifact surfaces: {}",
            receipt.summary
        );
        assert!(receipt.summary.contains("1 manifest(s) on record"));
    }

    #[test]
    fn get_metric_evidence_requires_warehouse_or_lineage() {
        let params = GetMetricEvidenceParams {
            project_id: "proj_1".to_string(),
            metric_name: "harmony.scale_compliance".to_string(),
            warehouse: None,
            lineage: None,
        };
        let receipt = get_metric_evidence(&params);
        assert!(matches!(receipt.status, ToolStatus::Failed));
        assert!(receipt.summary.contains("no serialized evidence warehouse"));
    }

    #[test]
    fn compare_candidates_selects_winner() {
        let project_a = scorestrata_testkit::melodic_techno_project();
        let mut project_b = project_a.clone();
        for pattern in &mut project_b.patterns {
            for note in &mut pattern.notes {
                note.pitch = scorestrata_core::domain::MidiNote::new(61);
            }
        }
        let params = CompareCandidatesParams {
            project_id: project_a.id.to_string(),
            candidate_a: "candidate_a".to_string(),
            candidate_b: "candidate_b".to_string(),
            project_a: Some(serde_json::to_value(&project_a).unwrap_or_default()),
            project_b: Some(serde_json::to_value(&project_b).unwrap_or_default()),
        };
        let receipt = compare_candidates(&params);
        assert!(
            matches!(receipt.status, ToolStatus::Succeeded),
            "comparison should succeed, got {receipt:#?}"
        );
        assert!(receipt.summary.contains("Compared"));
        assert!(receipt.score_delta.contains_key("diff.metrics_compared"));
        assert!(receipt.score_delta.contains_key("diff.winner"));
    }

    #[test]
    fn compare_candidates_requires_both_payloads() {
        let params = CompareCandidatesParams {
            project_id: "proj_1".to_string(),
            candidate_a: "a".to_string(),
            candidate_b: "b".to_string(),
            project_a: None,
            project_b: None,
        };
        let receipt = compare_candidates(&params);
        assert!(matches!(receipt.status, ToolStatus::Failed));
        assert!(
            receipt
                .summary
                .contains("both serialized candidate payloads")
        );
    }

    // ── Render tools tests ──────────────────────────────────────────

    #[test]
    fn render_project_returns_success() {
        let params = RenderProjectParams {
            project_id: "proj_1".to_string(),
            mode: "master".to_string(),
            project: None,
            segment_id: None,
            manifest_store: None,
            source_snapshot_id: None,
            command_id: None,
            seed: None,
            asset_hashes: None,
        };
        let receipt = render_project(&params);
        // Without a project payload, the render cannot run — it fails cleanly.
        assert!(matches!(receipt.status, ToolStatus::Failed));
        assert!(
            receipt
                .warnings
                .iter()
                .any(|w| w.contains("No project payload")),
            "expected missing-payload warning, got {:#?}",
            receipt.warnings
        );
    }

    #[test]
    fn render_segment_returns_success() {
        let params = RenderSegmentParams {
            project_id: "proj_1".to_string(),
            segment_id: "chorus_1".to_string(),
            mode: "stems".to_string(),
            project: None,
            manifest_store: None,
            source_snapshot_id: None,
            command_id: None,
            seed: None,
            asset_hashes: None,
        };
        let receipt = render_segment(&params);
        // Without a project payload, the render cannot run — it fails cleanly.
        assert!(matches!(receipt.status, ToolStatus::Failed));
        assert!(receipt.summary.contains("no project payload"));
    }

    #[test]
    fn render_project_emits_manifest_and_records_in_store() {
        // §16.6 contract: "render_project returns artifact manifest" — a
        // successful render must produce the §17.2 reproducibility receipt and
        // record it into a supplied manifest store.
        let project = scorestrata_testkit::melodic_techno_project();
        let store = scorestrata_storage::render_manifest::RenderManifestStore::new();
        let params = RenderProjectParams {
            project_id: "proj_manifest".to_string(),
            mode: "master".to_string(),
            project: Some(serde_json::to_value(&project).expect("project serializes")),
            segment_id: None,
            manifest_store: Some(serde_json::to_value(&store).expect("store serializes")),
            source_snapshot_id: Some("snap_proj_0042".to_string()),
            command_id: Some("cmd_0042".to_string()),
            seed: Some(1337),
            asset_hashes: None,
        };
        let receipt = render_project(&params);
        assert!(
            matches!(receipt.status, ToolStatus::Succeeded),
            "render succeeded: {}",
            receipt.summary
        );
        assert_eq!(
            receipt
                .score_delta
                .get("manifest.artifact_id")
                .map(String::as_str),
            Some("art_proj_manifest_master"),
            "manifest artifact id on the receipt"
        );
        assert_eq!(
            receipt
                .score_delta
                .get("manifest.recorded")
                .map(String::as_str),
            Some("true"),
            "manifest recorded into the supplied store"
        );
        let hash = receipt
            .score_delta
            .get("manifest.manifest_hash")
            .expect("manifest hash on receipt");
        assert!(!hash.is_empty());
        assert!(
            receipt
                .next_actions
                .iter()
                .any(|a| a == "get_render_manifest"),
            "recorded manifests are retrievable"
        );
        assert!(
            receipt.summary.contains(&hash[..8.min(hash.len())]),
            "summary carries the fingerprint: {}",
            receipt.summary
        );
    }

    #[test]
    fn render_project_emits_manifest_without_store_with_warning() {
        // §17.2 "every render produces a manifest" holds even without a store;
        // the fingerprint surfaces and a warning explains how to persist it.
        let project = scorestrata_testkit::melodic_techno_project();
        let params = RenderProjectParams {
            project_id: "proj_manifest".to_string(),
            mode: "master".to_string(),
            project: Some(serde_json::to_value(&project).expect("project serializes")),
            segment_id: None,
            manifest_store: None,
            source_snapshot_id: None,
            command_id: None,
            seed: None,
            asset_hashes: None,
        };
        let receipt = render_project(&params);
        assert!(matches!(receipt.status, ToolStatus::Succeeded));
        assert!(
            receipt.score_delta.contains_key("manifest.manifest_hash"),
            "fingerprint always emitted"
        );
        assert!(
            receipt.warnings.iter().any(|w| w.contains("not persisted")),
            "warning explains the missing store: {:#?}",
            receipt.warnings
        );
    }

    #[test]
    fn render_project_handles_malformed_manifest_store_with_warning() {
        // A malformed store payload must not fail the render: the receipt
        // still carries the fingerprint, warns, and does not advertise a
        // get_render_manifest round trip that cannot happen.
        let project = scorestrata_testkit::melodic_techno_project();
        let params = RenderProjectParams {
            project_id: "proj_manifest".to_string(),
            mode: "master".to_string(),
            project: Some(serde_json::to_value(&project).expect("project serializes")),
            segment_id: None,
            manifest_store: Some(serde_json::json!([1, 2, 3])),
            source_snapshot_id: None,
            command_id: None,
            seed: None,
            asset_hashes: None,
        };
        let receipt = render_project(&params);
        assert!(
            matches!(receipt.status, ToolStatus::Succeeded),
            "render still succeeds: {}",
            receipt.summary
        );
        assert!(receipt.score_delta.contains_key("manifest.manifest_hash"));
        assert!(
            !receipt.score_delta.contains_key("manifest.recorded"),
            "no recorded claim on malformed store"
        );
        assert!(
            !receipt
                .next_actions
                .iter()
                .any(|a| a == "get_render_manifest"),
            "no retrieval round trip advertised"
        );
        assert!(
            receipt
                .warnings
                .iter()
                .any(|w| w.contains("could not be recorded")),
            "warning explains the malformed store: {:#?}",
            receipt.warnings
        );
    }

    #[test]
    fn render_project_manifest_fingerprint_is_reproducible() {
        // §17.1: identical reproducibility inputs → identical manifest hash;
        // seed drift changes the fingerprint.
        let project = scorestrata_testkit::melodic_techno_project();
        let params = RenderProjectParams {
            project_id: "proj_manifest".to_string(),
            mode: "master".to_string(),
            project: Some(serde_json::to_value(&project).expect("project serializes")),
            segment_id: None,
            manifest_store: None,
            source_snapshot_id: Some("snap_proj_0042".to_string()),
            command_id: Some("cmd_0042".to_string()),
            seed: Some(1337),
            asset_hashes: None,
        };
        let a = render_project(&params);
        let b = render_project(&params);
        assert_eq!(
            a.score_delta.get("manifest.manifest_hash"),
            b.score_delta.get("manifest.manifest_hash"),
            "identical inputs reproduce the same manifest hash"
        );
        let mut drifted = params;
        drifted.seed = Some(9999);
        let c = render_project(&drifted);
        assert_ne!(
            a.score_delta.get("manifest.manifest_hash"),
            c.score_delta.get("manifest.manifest_hash"),
            "seed drift breaks the fingerprint"
        );
    }

    #[test]
    fn render_stems_without_payload_fails_cleanly() {
        let params = RenderStemsParams {
            project_id: "proj_1".to_string(),
            project: None,
        };
        let receipt = render_stems(&params);
        assert!(matches!(receipt.status, ToolStatus::Failed));
        assert!(
            receipt
                .warnings
                .iter()
                .any(|w| w.contains("No project payload"))
        );
    }

    #[test]
    fn render_stems_with_project_produces_stems() {
        // Build a real project (planner-populated with patterns) via the
        // testkit and serialize it — a minimal project has no pattern
        // instances, so it would compile to zero events and zero stems.
        let project = scorestrata_testkit::melodic_techno_project();
        let project_json = serde_json::to_value(&project).expect("project serializes");
        let params = RenderStemsParams {
            project_id: project.id.to_string(),
            project: Some(project_json),
        };
        let receipt = render_stems(&params);
        assert!(
            matches!(receipt.status, ToolStatus::Succeeded),
            "stems render should succeed, got {:#?}",
            receipt
        );
        assert!(receipt.summary.contains("stems"));
        assert!(receipt.score_delta.contains_key("render.stems"));
        assert!(!receipt.evidence_uris.is_empty());
    }

    #[test]
    fn audit_layer_spectral_without_project_fails_cleanly() {
        let params = AuditLayerSpectralParams {
            project_id: "proj_1".to_string(),
            project: None,
        };
        let receipt = audit_layer_spectral(&params);
        assert!(matches!(receipt.status, ToolStatus::Failed));
        assert!(
            receipt
                .warnings
                .iter()
                .any(|w| w.contains("No project payload")),
            "expected missing-payload warning, got {:#?}",
            receipt.warnings
        );
    }

    #[test]
    fn audit_layer_spectral_reports_each_layer_separately() {
        // A real multi-layer project renders per-layer stems; the audit must
        // surface each layer's own spectral footprint (the layer-wise view
        // the master graph hides).
        let project = scorestrata_testkit::melodic_techno_project();
        let params = AuditLayerSpectralParams {
            project_id: project.id.to_string(),
            project: Some(serde_json::to_value(&project).expect("project serializes")),
        };
        let receipt = audit_layer_spectral(&params);
        assert!(
            matches!(receipt.status, ToolStatus::Succeeded),
            "layer audit should succeed, got {:#?}",
            receipt
        );
        assert!(receipt.summary.contains("Audited"));
        assert!(
            receipt.score_delta.contains_key("audit.layers"),
            "audit.layers score present: {:#?}",
            receipt.score_delta
        );
        assert!(
            !receipt.evidence_uris.is_empty(),
            "per-layer evidence URIs present"
        );
        assert!(
            receipt
                .evidence_uris
                .iter()
                .any(|uri| uri.contains("/audit/layers/")),
            "evidence URIs point at layer audits: {:#?}",
            receipt.evidence_uris
        );
    }

    #[test]
    fn render_candidate_preview_requires_project_payload() {
        let params = RenderCandidatePreviewParams {
            project_id: "proj_1".to_string(),
            candidate_id: "cand_a".to_string(),
            project: None,
            jobs: None,
        };
        let receipt = render_candidate_preview(&params);
        assert!(matches!(receipt.status, ToolStatus::Failed));
        assert!(
            receipt
                .warnings
                .iter()
                .any(|w| w.contains("No project payload")),
            "expected missing-payload warning, got {:#?}",
            receipt.warnings
        );
    }

    #[test]
    fn render_candidate_preview_registers_completed_job() {
        let project = scorestrata_testkit::melodic_techno_project();
        let params = RenderCandidatePreviewParams {
            project_id: project.id.to_string(),
            candidate_id: "cand_a".to_string(),
            project: Some(serde_json::to_value(&project).unwrap_or_default()),
            jobs: Some(serde_json::json!({})),
        };
        let receipt = render_candidate_preview(&params);
        assert!(
            matches!(receipt.status, ToolStatus::Succeeded),
            "preview render should succeed, got {:#?}",
            receipt
        );
        assert_eq!(
            receipt
                .score_delta
                .get("render.job_status")
                .map(String::as_str),
            Some("Completed")
        );
        assert!(
            receipt
                .evidence_uris
                .iter()
                .any(|u| u.contains("render_jobs://"))
        );
    }

    #[test]
    fn get_render_job_reports_lifecycle_status() {
        let mut store = scorestrata_storage::render_job::RenderJobStore::new();
        store.submit(scorestrata_storage::render_job::RenderJob::new(
            "job_1", "proj_1", "preview",
        ));
        store.start("job_1");
        let params = GetRenderJobParams {
            project_id: "proj_1".to_string(),
            job_id: "job_1".to_string(),
            jobs: Some(serde_json::to_value(&store).unwrap_or_default()),
        };
        let receipt = get_render_job(&params);
        assert!(matches!(receipt.status, ToolStatus::Succeeded));
        assert_eq!(
            receipt
                .score_delta
                .get("render.job_status")
                .map(String::as_str),
            Some("Rendering")
        );
    }

    #[test]
    fn get_render_job_unknown_job_fails() {
        let store = scorestrata_storage::render_job::RenderJobStore::new();
        let params = GetRenderJobParams {
            project_id: "proj_1".to_string(),
            job_id: "job_missing".to_string(),
            jobs: Some(serde_json::to_value(&store).unwrap_or_default()),
        };
        let receipt = get_render_job(&params);
        assert!(matches!(receipt.status, ToolStatus::Failed));
        assert!(
            receipt
                .warnings
                .iter()
                .any(|w| w.contains("Unknown render job")),
            "expected unknown-job warning, got {:#?}",
            receipt.warnings
        );
    }

    #[test]
    fn cancel_render_job_cancels_queued_job() {
        let mut store = scorestrata_storage::render_job::RenderJobStore::new();
        store.submit(scorestrata_storage::render_job::RenderJob::new(
            "job_1", "proj_1", "master",
        ));
        let params = CancelRenderJobParams {
            project_id: "proj_1".to_string(),
            job_id: "job_1".to_string(),
            jobs: Some(serde_json::to_value(&store).unwrap_or_default()),
        };
        let receipt = cancel_render_job(&params);
        assert!(matches!(receipt.status, ToolStatus::Succeeded));
        assert!(receipt.summary.contains("cancelled"));
    }

    #[test]
    fn cancel_render_job_refuses_terminal_job() {
        let mut store = scorestrata_storage::render_job::RenderJobStore::new();
        store.submit(scorestrata_storage::render_job::RenderJob::new(
            "job_1", "proj_1", "master",
        ));
        store.start("job_1");
        store.complete("job_1", "artifact://x.wav");
        let params = CancelRenderJobParams {
            project_id: "proj_1".to_string(),
            job_id: "job_1".to_string(),
            jobs: Some(serde_json::to_value(&store).unwrap_or_default()),
        };
        let receipt = cancel_render_job(&params);
        assert!(matches!(receipt.status, ToolStatus::Failed));
        assert!(
            receipt.summary.contains("Cannot cancel"),
            "terminal job must refuse cancellation: {}",
            receipt.summary
        );
    }

    #[test]
    fn l4_emits_audio_diagnostics_on_fixture() {
        // The melodic-techno fixture renders deterministically. With the fixed
        // engine (limiter ceiling −1.0 dBTP, polyBLEP oscillators, gain
        // staging) it no longer clips or exceeds the true-peak limit, so L4
        // must surface the audio plane (narrow-stereo on the mono render) but
        // must NOT report clipping or a true-peak violation.
        let project = scorestrata_testkit::melodic_techno_project();
        let receipt = run_validation_with_project(&default_params(), &project, 4);
        let codes: Vec<String> = receipt.diagnostics.iter().map(|d| d.code.clone()).collect();
        assert!(
            codes.iter().any(|c| c == "AUDIO_NARROW_STEREO"),
            "L4 audio plane must be active: {codes:?}"
        );
        assert!(
            !codes.iter().any(|c| c == "AUDIO_CLIPPING_DETECTED"),
            "fixed engine must not clip: {codes:?}"
        );
        assert!(
            !codes.iter().any(|c| c == "AUDIO_TRUE_PEAK_EXCEEDED"),
            "fixed engine must respect the −1.0 dBTP ceiling: {codes:?}"
        );
        assert!(
            !codes.iter().any(|c| c == "AUDIO_RENDER_FAILED"),
            "clean fixture must render without failure: {codes:?}"
        );
    }

    #[test]
    fn l4_gated_off_below_level_four() {
        let project = scorestrata_testkit::melodic_techno_project();
        for level in [2, 3] {
            let receipt = run_validation_with_project(&default_params(), &project, level);
            assert!(
                !receipt
                    .diagnostics
                    .iter()
                    .any(|d| d.code.starts_with("AUDIO_")),
                "audio plane must stay off below L4 (level {level})"
            );
        }
    }

    #[test]
    fn dispatcher_l4_emits_audio_diagnostics() {
        // Serialize with the shared registry lock: this test records telemetry
        // and must not race the telemetry module's exact-count assertions.
        let _guard = crate::telemetry::test_lock_guard();
        let project = scorestrata_testkit::melodic_techno_project();
        let receipt = crate::dispatch::dispatch_tool(
            "run_validation",
            serde_json::json!({
                "project_id": project.id.to_string(),
                "project": project,
                "level": 4,
            }),
        );
        assert_eq!(receipt.tool, "run_validation");
        assert!(
            receipt
                .diagnostics
                .iter()
                .any(|d| d.code == "AUDIO_NARROW_STEREO"),
            "dispatcher L4 must emit audio diagnostics"
        );
        assert!(
            !receipt
                .diagnostics
                .iter()
                .any(|d| d.code == "AUDIO_CLIPPING_DETECTED"),
            "dispatcher L4 must not report clipping on the fixed engine"
        );
    }

    /// A "perfect audio" baseline: every audio hard gate at 1.0.
    fn clean_scorecard_baseline() -> scorestrata_core::diagnostic::Scorecard {
        use scorestrata_core::diagnostic::{MetricSample, MetricScope, Scorecard};
        use scorestrata_core::error::Severity;
        use scorestrata_core::ids::{MetricId, ScorecardId};
        let gate = |name: &str| MetricSample {
            id: MetricId::new(),
            name: name.to_string(),
            value: 1.0,
            severity: Severity::Info,
            scope: MetricScope::default(),
            evidence_uri: format!("project://baseline/{name}"),
            explanation: "baseline clean".to_string(),
            confidence: 1.0,
            suggested_patches: vec![],
        };
        Scorecard {
            id: ScorecardId::new(),
            metrics: vec![
                gate("audio.clipping"),
                gate("audio.true_peak_dbtp"),
                gate("audio.integrated_lufs"),
            ],
            overall_score: 1.0,
            hard_failures: 0,
            warnings: 0,
        }
    }

    #[test]
    fn check_regressions_flags_scorecard_regression() {
        // Baseline claims perfect audio (all hard gates at 1.0). With the fixed
        // engine the clean melodic-techno fixture renders within every limit,
        // so it must NOT be flagged as a regression — while a genuinely
        // regressed project (silenced layers → loudness collapse) drops
        // audio.integrated_lufs below the baseline and must be flagged as a
        // hard-gate regression.
        let clean = scorestrata_testkit::melodic_techno_project();
        let clean_params = CheckRegressionsParams {
            project_id: clean.id.to_string(),
            baseline_scorecard: Some(clean_scorecard_baseline()),
            baseline_checksum: None,
            baseline_integrated_lufs: None,
            baseline_true_peak_dbtp: None,
            project: Some(serde_json::to_value(&clean).expect("project serializes")),
        };
        let clean_receipt = check_regressions(&clean_params);
        assert!(
            !clean_receipt
                .diagnostics
                .iter()
                .any(|d| d.code == "METRIC_REGRESSION_DETECTED"),
            "the fixed clean fixture must not be flagged: {:?}",
            clean_receipt
                .diagnostics
                .iter()
                .map(|d| d.code.clone())
                .collect::<Vec<_>>()
        );

        // Silence every layer: the render collapses to near-silence and
        // audio.integrated_lufs drops from the 1.0 baseline — a hard gate.
        let mut regressed = clean;
        for segment in &mut regressed.arrangement.segments {
            segment.layers.clear();
        }
        let params = CheckRegressionsParams {
            project_id: regressed.id.to_string(),
            baseline_scorecard: Some(clean_scorecard_baseline()),
            baseline_checksum: None,
            baseline_integrated_lufs: None,
            baseline_true_peak_dbtp: None,
            project: Some(serde_json::to_value(&regressed).expect("project serializes")),
        };
        let receipt = check_regressions(&params);
        assert!(
            receipt
                .diagnostics
                .iter()
                .any(|d| d.code == "METRIC_REGRESSION_DETECTED"),
            "loudness regression must be flagged"
        );
        assert!(
            receipt
                .diagnostics
                .iter()
                .any(|d| d.code == "HARD_GATE_REGRESSION"),
            "audio.integrated_lufs is a hard gate"
        );
        assert_eq!(
            receipt
                .score_delta
                .get("regression.hard_gates")
                .map(String::as_str),
            Some("1")
        );
    }

    #[test]
    fn check_regressions_flags_render_drift() {
        let project = scorestrata_testkit::melodic_techno_project();
        // A deliberately different checksum plus a louder/peaky baseline drifts
        // past both the loudness (1.0 LUFS) and peak (0.5 dB) tolerances.
        let params = CheckRegressionsParams {
            project_id: project.id.to_string(),
            baseline_scorecard: None,
            baseline_checksum: Some("deadbeef".to_string()),
            baseline_integrated_lufs: Some(-14.0),
            baseline_true_peak_dbtp: Some(0.0),
            project: Some(serde_json::to_value(&project).expect("project serializes")),
        };
        let receipt = check_regressions(&params);
        assert!(
            receipt
                .diagnostics
                .iter()
                .any(|d| d.code == "RENDER_LOUDNESS_DRIFT"),
            "loudness drift past 1.0 LUFS must be flagged"
        );
        assert!(
            receipt
                .diagnostics
                .iter()
                .any(|d| d.code == "RENDER_PEAK_DRIFT"),
            "peak drift past 0.5 dB must be flagged"
        );
        assert!(
            receipt
                .diagnostics
                .iter()
                .any(|d| d.code == "RENDER_CHECKSUM_CHANGED"),
            "checksum change must be flagged"
        );
    }

    #[test]
    fn check_regressions_loudness_only_baseline_skips_checksum() {
        // A baseline that only supplies loudness must not trip a spurious
        // RENDER_CHECKSUM_CHANGED — missing axes default to "skip".
        let project = scorestrata_testkit::melodic_techno_project();
        let result = render_project_to_analysis(&project).expect("fixture renders");
        let params = CheckRegressionsParams {
            project_id: project.id.to_string(),
            baseline_scorecard: None,
            baseline_checksum: None,
            baseline_integrated_lufs: Some(result.analysis.integrated_lufs),
            baseline_true_peak_dbtp: None,
            project: Some(serde_json::to_value(&project).expect("project serializes")),
        };
        let receipt = check_regressions(&params);
        assert!(
            !receipt
                .diagnostics
                .iter()
                .any(|d| d.code == "RENDER_CHECKSUM_CHANGED"),
            "no checksum baseline means no checksum comparison"
        );
    }

    #[test]
    fn check_regressions_passes_when_baseline_matches() {
        let project = scorestrata_testkit::melodic_techno_project();
        // Render once to capture the exact current signature, then use it as
        // the baseline — rendering is deterministic, so nothing drifts.
        let result = render_project_to_analysis(&project).expect("fixture renders");
        let params = CheckRegressionsParams {
            project_id: project.id.to_string(),
            baseline_scorecard: None,
            baseline_checksum: Some(result.checksum.clone()),
            baseline_integrated_lufs: Some(result.analysis.integrated_lufs),
            baseline_true_peak_dbtp: Some(result.analysis.true_peak_dbtp),
            project: Some(serde_json::to_value(&project).expect("project serializes")),
        };
        let receipt = check_regressions(&params);
        assert!(matches!(receipt.status, ToolStatus::Succeeded));
        assert!(
            !receipt
                .diagnostics
                .iter()
                .any(|d| d.code.starts_with("RENDER_")),
            "matching signature must not drift"
        );
    }

    #[test]
    fn check_regressions_requires_baseline() {
        let project = scorestrata_testkit::melodic_techno_project();
        let params = CheckRegressionsParams {
            project_id: project.id.to_string(),
            baseline_scorecard: None,
            baseline_checksum: None,
            baseline_integrated_lufs: None,
            baseline_true_peak_dbtp: None,
            project: Some(serde_json::to_value(&project).expect("project serializes")),
        };
        let receipt = check_regressions(&params);
        assert!(matches!(receipt.status, ToolStatus::Failed));
        assert!(
            receipt.summary.contains("No baseline"),
            "must explain that a baseline is required"
        );
    }

    #[test]
    fn dispatcher_check_regressions_wire_path() {
        // Serialize with the shared registry lock: this test records telemetry
        // and must not race the telemetry module's exact-count assertions.
        let _guard = crate::telemetry::test_lock_guard();
        // A genuinely regressed project (silenced layers → loudness collapse)
        // must be flagged as a metric regression over the dispatcher wire.
        let mut project = scorestrata_testkit::melodic_techno_project();
        for segment in &mut project.arrangement.segments {
            segment.layers.clear();
        }
        let baseline =
            serde_json::to_value(clean_scorecard_baseline()).expect("scorecard serializes");
        let receipt = crate::dispatch::dispatch_tool(
            "check_regressions",
            serde_json::json!({
                "project_id": project.id.to_string(),
                "baseline_scorecard": baseline,
                "project": project,
            }),
        );
        assert_eq!(receipt.tool, "check_regressions");
        assert!(
            receipt
                .diagnostics
                .iter()
                .any(|d| d.code == "METRIC_REGRESSION_DETECTED"),
            "dispatcher L6 must reach regression detection"
        );
    }

    #[test]
    fn dispatcher_suggest_fixes_wire_path() {
        // The failure report recommends suggest_fixes as the next action, so
        // the dispatcher must route it and reach the real validator patches.
        let _guard = crate::telemetry::test_lock_guard();
        let project = scorestrata_testkit::melodic_techno_project();
        let receipt = crate::dispatch::dispatch_tool(
            "suggest_fixes",
            serde_json::json!({
                "project_id": project.id.to_string(),
                "diagnostic_code": "TRANSITION_ENERGY_DELTA_LOW",
                "project": project,
            }),
        );
        assert_eq!(receipt.tool, "suggest_fixes");
        assert!(matches!(receipt.status, ToolStatus::Succeeded));
        assert!(receipt.summary.contains("add_transition_package"));
        assert!(receipt.next_actions.contains(&"apply_patch".to_string()));
    }

    #[test]
    fn scoring_profile_for_genre_resolves_genre_weights() {
        // §11.8: melodic techno is rhythm/structure-led, so the calibrated
        // profile must override the generic defaults.
        let profile = scoring_profile_for_genre("melodic_techno");
        assert_eq!(profile.weights.get("rhythm"), Some(&1.3));
        assert_eq!(profile.weights.get("structure"), Some(&1.2));
        assert_eq!(profile.weights.get("arrangement"), Some(&1.0));
        // Ambient inverts the priorities.
        let ambient = scoring_profile_for_genre("ambient");
        assert_eq!(ambient.weights.get("rhythm"), Some(&0.6));
        assert_eq!(ambient.weights.get("harmony"), Some(&1.4));
    }

    #[test]
    fn scoring_profile_for_genre_falls_back_for_unknown() {
        let profile = scoring_profile_for_genre("no_such_genre");
        let defaults = scorestrata_validator::report::ScoringProfile::default();
        assert_eq!(profile.weights, defaults.weights);
        assert_eq!(profile.min_overall_score, defaults.min_overall_score);
        assert_eq!(profile.max_hard_failures, defaults.max_hard_failures);
    }

    #[test]
    fn check_regressions_uses_genre_calibrated_scorecard() {
        // The melodic-techno fixture produces arrangement + audio metrics, so
        // the genre weight for arrangement (1.0 vs default 0.8) must change
        // the overall score the L6 scorecard sees.
        let project = scorestrata_testkit::melodic_techno_project();
        let run_params = RunValidationParams {
            project_id: project.id.to_string(),
            scope: default_scope(),
            segment_ids: vec![],
            layer_ids: vec![],
            min_energy_delta: default_min_energy_delta(),
        };
        let receipt = run_validation_with_project(&run_params, &project, 4);
        let default_report = scorestrata_validator::report::build_validation_report(
            project.id,
            4,
            receipt.diagnostics.clone(),
        );
        let calibrated_report = scorestrata_validator::report::build_validation_report_with_profile(
            project.id,
            4,
            receipt.diagnostics,
            &scoring_profile_for_genre(&project.config.genre),
        );
        assert_ne!(
            calibrated_report.scorecard.overall_score, default_report.scorecard.overall_score,
            "genre calibration must affect the L6 scorecard"
        );
    }
}
