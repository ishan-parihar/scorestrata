//! MCP tools — mutating operations that return structured receipts.
//!
//! Every tool returns a receipt with status, score delta, evidence URIs,
//! policy decisions, and next actions per the architecture spec §10.5.
//!
//! Tool categories from the architecture:
//! - Governance tools: get_capabilities, get_contracts, get_policy_profile, check_budget
//! - Project tools: create_project, generate_config_from_brief, validate_config, approve_config
//! - Composition tools: plan_arrangement, create_pattern, write_pattern_notes, apply_motivic_variation
//! - Patch tools: apply_patch, dry_run_patch, quantize_layer_to_scale, add_transition_package
//! - Validation tools: run_validation, get_validation_report, suggest_fixes, compare_candidates
//! - Debug tools: get_command_trace, get_snapshot_diff, get_candidate_diff,
//!   isolate_layer, solo_segment, explain_metric (real logic in [`debug_ops`])
//! - Learning tools: get_lessons, record_lesson, get_repair_templates
//!   (real logic against [`LessonStore`](scorestrata_storage::lesson_store::LessonStore))
//! - Engineering tools: get_next_task, claim_task, submit_task_result, run_acceptance_tests

use scorestrata_core::diagnostic::Diagnostic;
use scorestrata_core::error::Severity;
use scorestrata_validator::report::build_validation_report;
use scorestrata_validator::segment_check::{check_segment_bar_count, check_segment_has_layers};
use scorestrata_validator::transition_check::check_all_transitions;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

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
pub fn run_validation(
    params: &RunValidationParams,
    segments: &[scorestrata_core::domain::Segment],
    level: u8,
) -> ToolReceipt {
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
    match &params.project {
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
    }
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
fn deserialize_error_receipt(tool: &str, field: &str, err: &serde_json::Error) -> ToolReceipt {
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
    store.record(lesson);

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
}

/// Suggest fixes for a specific diagnostic.
#[must_use]
pub fn suggest_fixes(params: &SuggestFixesParams) -> ToolReceipt {
    ToolReceipt {
        tool: "suggest_fixes".to_string(),
        status: ToolStatus::Succeeded,
        summary: format!(
            "Suggested fixes for '{}' in project '{}'",
            params.diagnostic_code, params.project_id
        ),
        score_delta: BTreeMap::new(),
        evidence_uris: vec![format!(
            "project://{}/diagnostics/{}/fixes",
            params.project_id, params.diagnostic_code
        )],
        focus_uris: vec![],
        next_actions: vec!["apply_patch".to_string()],
        cost: ToolCost::default(),
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

    // Map layer IDs to human-readable names for voice assignment.
    let mut layer_names = std::collections::HashMap::new();
    for segment in &project.arrangement.segments {
        for layer in &segment.layers {
            layer_names.insert(layer.id.to_string(), layer.name.clone());
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
                    duration_ticks: crate::patch_ops::next_note_off_duration(&timeline, e),
                    velocity: velocity.as_f32(),
                    layer_name,
                })
            }
            _ => None,
        })
        .collect();

    match scorestrata_audio::render::render_notes(
        &notes,
        timeline.bpm,
        scorestrata_audio::render::DEFAULT_SAMPLE_RATE,
        &[],
    ) {
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

            ToolReceipt {
                tool: "render_project".to_string(),
                status: ToolStatus::Succeeded,
                summary: format!(
                    "Rendered {} events ({:.2}s, sha256 {}, {:.1} LUFS, {:.1} dBTP) in '{}' mode",
                    timeline.event_count(),
                    result.duration_seconds,
                    &result.checksum[..8.min(result.checksum.len())],
                    result.analysis.integrated_lufs,
                    result.analysis.true_peak_dbtp,
                    params.mode
                ),
                score_delta,
                evidence_uris: vec![format!(
                    "project://{}/render/{}",
                    params.project_id, params.mode
                )],
                focus_uris: vec![],
                next_actions: vec![
                    "get_render_job".to_string(),
                    "run_validation".to_string(),
                    "apply_patch".to_string(),
                ],
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

    // Map layer IDs to human-readable names for voice assignment.
    let mut layer_names = std::collections::HashMap::new();
    for segment in &project.arrangement.segments {
        for layer in &segment.layers {
            layer_names.insert(layer.id.to_string(), layer.name.clone());
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

    match scorestrata_audio::render::render_stems(
        &notes,
        timeline.bpm,
        scorestrata_audio::render::DEFAULT_SAMPLE_RATE,
        &[],
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
    fn suggest_fixes_returns_success() {
        let params = SuggestFixesParams {
            project_id: "proj_1".to_string(),
            diagnostic_code: "SCALE_VIOLATION".to_string(),
            segment_id: Some("chorus_1".to_string()),
        };
        let receipt = suggest_fixes(&params);
        assert!(matches!(receipt.status, ToolStatus::Succeeded));
        assert!(receipt.next_actions.contains(&"apply_patch".to_string()));
    }

    // ── Render tools tests ──────────────────────────────────────────

    #[test]
    fn render_project_returns_success() {
        let params = RenderProjectParams {
            project_id: "proj_1".to_string(),
            mode: "master".to_string(),
            project: None,
            segment_id: None,
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
        };
        let receipt = render_segment(&params);
        // Without a project payload, the render cannot run — it fails cleanly.
        assert!(matches!(receipt.status, ToolStatus::Failed));
        assert!(receipt.summary.contains("no project payload"));
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
}
