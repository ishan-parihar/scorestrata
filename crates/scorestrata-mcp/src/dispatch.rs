//! MCP tool dispatcher — the server's `call_tool` core (§10.5).
//!
//! Routes a `tool_name` plus JSON params to the concrete tool implementation
//! and returns a `ToolReceipt`. Every tool registered in
//! [`crate::capabilities::get_capabilities()`] is routed here, so agents can
//! invoke any tool over the wire by name without knowing its Rust signature.
//!
//! Three shapes of tool exist:
//! - **Standard** (56 tools): `fn(&XParams) -> ToolReceipt` — deserialized from
//!   the JSON params and forwarded.
//! - **No-param** (3 tools): `get_contracts`, `get_onboarding_packet`,
//!   `get_capabilities` — the latter returns a `CapabilityManifest` which is
//!   wrapped into a receipt.
//! - **Multi-arg** (1 tool): `run_validation(params, segments, level)` — the
//!   segments and level are read from the params payload.

use std::collections::BTreeMap;

use crate::tools::{
    AddDrumFillParams, AddTransitionParams, ApplyMotivicVariationParams, ApplyPatchParams,
    ApproveConfigParams, CancelRenderJobParams, CandidateDiffParams, CheckBudgetParams,
    ClaimTaskParams, CompareCandidatesParams, CreateAdrParams, CreateHumanReviewPacketParams,
    CreatePatternParams, CreateProjectParams, DryRunPatchParams, ExplainMetricParams,
    GenerateConfigParams, GetAuditSummaryParams, GetCommandTraceParams, GetEvalReportParams,
    GetFailureReportParams, GetLessonsParams, GetMetricEvidenceParams, GetNextTaskParams,
    GetPolicyProfileParams, GetProjectSummaryParams, GetRenderJobParams, GetRepairTemplatesParams,
    GetRootCausesParams, GetValidationReportParams, HarmonizeMelodyParams,
    IncreaseSegmentDensityParams, IsolateLayerParams, NormalizeLoudnessTargetParams,
    PlacePatternInSegmentParams, PlanArrangementParams, QuantizeLayerParams, RecordLessonParams,
    ReduceMaskingParams, RenderCandidatePreviewParams, RenderProjectParams, RenderSegmentParams,
    RenderStemsParams, RunAcceptanceTestsParams, RunValidationParams, SetEnergyCurveParams,
    SetLayerInstrumentParams, SetSegmentGoalsParams, SnapshotDiffParams, SoloSegmentParams,
    SubmitTaskResultParams, SuggestFixesParams, ToolCost, ToolReceipt, ToolStatus,
    UpdateCalibrationProfileParams, UpdateDocsParams, ValidateConfigParams,
    WritePatternNotesParams, WritePatternStepsParams,
};

/// Deserialize `params` into `$ty` and forward to `$func`, mapping any
/// deserialization failure to a transparent `Failed` receipt.
macro_rules! route {
    ($tool_name:expr, $params:expr, $ty:ty, $func:path) => {{
        match serde_json::from_value::<$ty>($params) {
            Ok(p) => $func(&p),
            Err(e) => crate::tools::deserialize_error_receipt($tool_name, "params", &e),
        }
    }};
}

/// Dispatch a tool call by name with JSON params.
///
/// Returns a `ToolReceipt` whose `tool` field always echoes the requested
/// name. Unknown tools produce a `Failed` receipt pointing at
/// `get_capabilities` for the registered catalog.
///
/// Every invocation is recorded in the telemetry registry (§15.5), so the
/// snapshot reflects what the system has actually run.
#[must_use]
pub fn dispatch_tool(tool_name: &str, params: serde_json::Value) -> ToolReceipt {
    crate::telemetry::record_tool_call(tool_name);
    match tool_name {
        // ── Special-shape tools ─────────────────────────────────────
        "get_capabilities" => capabilities_receipt(),
        "get_contracts" => crate::tools::get_contracts(),
        "get_onboarding_packet" => crate::tools::get_onboarding_packet(),
        "get_telemetry" => crate::tools::get_telemetry(),
        "run_validation" => run_validation_from_params(params),
        // ── Governance tools ────────────────────────────────────────
        "get_policy_profile" => route!(
            tool_name,
            params,
            GetPolicyProfileParams,
            crate::tools::get_policy_profile
        ),
        "check_budget" => {
            route!(
                tool_name,
                params,
                CheckBudgetParams,
                crate::tools::check_budget
            )
        }
        // ── Project tools ───────────────────────────────────────────
        "create_project" => {
            route!(
                tool_name,
                params,
                CreateProjectParams,
                crate::tools::create_project
            )
        }
        "generate_config_from_brief" => route!(
            tool_name,
            params,
            GenerateConfigParams,
            crate::tools::generate_config_from_brief
        ),
        "validate_config" => {
            route!(
                tool_name,
                params,
                ValidateConfigParams,
                crate::tools::validate_config
            )
        }
        "approve_config" => {
            route!(
                tool_name,
                params,
                ApproveConfigParams,
                crate::tools::approve_config
            )
        }
        "get_project_summary" => route!(
            tool_name,
            params,
            GetProjectSummaryParams,
            crate::tools::get_project_summary
        ),
        // ── Composition tools ───────────────────────────────────────
        "plan_arrangement" => {
            route!(
                tool_name,
                params,
                PlanArrangementParams,
                crate::tools::plan_arrangement
            )
        }
        "create_pattern" => {
            route!(
                tool_name,
                params,
                CreatePatternParams,
                crate::tools::create_pattern
            )
        }
        "write_pattern_notes" => route!(
            tool_name,
            params,
            WritePatternNotesParams,
            crate::tools::write_pattern_notes
        ),
        "write_pattern_steps" => route!(
            tool_name,
            params,
            WritePatternStepsParams,
            crate::tools::write_pattern_steps
        ),
        "apply_motivic_variation" => route!(
            tool_name,
            params,
            ApplyMotivicVariationParams,
            crate::tools::apply_motivic_variation
        ),
        "place_pattern_in_segment" => route!(
            tool_name,
            params,
            PlacePatternInSegmentParams,
            crate::tools::place_pattern_in_segment
        ),
        "set_layer_instrument" => route!(
            tool_name,
            params,
            SetLayerInstrumentParams,
            crate::tools::set_layer_instrument
        ),
        "set_segment_goals" => route!(
            tool_name,
            params,
            SetSegmentGoalsParams,
            crate::tools::set_segment_goals
        ),
        "set_energy_curve" => route!(
            tool_name,
            params,
            SetEnergyCurveParams,
            crate::tools::set_energy_curve
        ),
        // ── Patch tools ─────────────────────────────────────────────
        "apply_patch" => {
            route!(
                tool_name,
                params,
                ApplyPatchParams,
                crate::tools::apply_patch
            )
        }
        "dry_run_patch" => {
            route!(
                tool_name,
                params,
                DryRunPatchParams,
                crate::tools::dry_run_patch
            )
        }
        "quantize_layer_to_scale" => route!(
            tool_name,
            params,
            QuantizeLayerParams,
            crate::tools::quantize_layer_to_scale
        ),
        "harmonize_melody" => route!(
            tool_name,
            params,
            HarmonizeMelodyParams,
            crate::tools::harmonize_melody
        ),
        "add_drum_fill" => {
            route!(
                tool_name,
                params,
                AddDrumFillParams,
                crate::tools::add_drum_fill
            )
        }
        "add_transition_package" => route!(
            tool_name,
            params,
            AddTransitionParams,
            crate::tools::add_transition_package
        ),
        "increase_segment_density" => route!(
            tool_name,
            params,
            IncreaseSegmentDensityParams,
            crate::tools::increase_segment_density
        ),
        "reduce_masking" => {
            route!(
                tool_name,
                params,
                ReduceMaskingParams,
                crate::tools::reduce_masking
            )
        }
        "normalize_loudness_target" => route!(
            tool_name,
            params,
            NormalizeLoudnessTargetParams,
            crate::tools::normalize_loudness_target
        ),
        // ── Validation tools ────────────────────────────────────────
        "get_validation_report" => route!(
            tool_name,
            params,
            GetValidationReportParams,
            crate::tools::get_validation_report
        ),
        "get_audit_summary" => route!(
            tool_name,
            params,
            GetAuditSummaryParams,
            crate::tools::get_audit_summary
        ),
        "explain_metric" => route!(
            tool_name,
            params,
            ExplainMetricParams,
            crate::tools::explain_metric
        ),
        "get_root_causes" => route!(
            tool_name,
            params,
            GetRootCausesParams,
            crate::tools::get_root_causes
        ),
        "get_metric_evidence" => route!(
            tool_name,
            params,
            GetMetricEvidenceParams,
            crate::tools::get_metric_evidence
        ),
        "suggest_fixes" => route!(
            tool_name,
            params,
            SuggestFixesParams,
            crate::tools::suggest_fixes
        ),
        "compare_candidates" => route!(
            tool_name,
            params,
            CompareCandidatesParams,
            crate::tools::compare_candidates
        ),
        // ── Render tools ────────────────────────────────────────────
        "render_project" => {
            route!(
                tool_name,
                params,
                RenderProjectParams,
                crate::tools::render_project
            )
        }
        "render_segment" => {
            route!(
                tool_name,
                params,
                RenderSegmentParams,
                crate::tools::render_segment
            )
        }
        "render_stems" => {
            route!(
                tool_name,
                params,
                RenderStemsParams,
                crate::tools::render_stems
            )
        }
        "render_candidate_preview" => route!(
            tool_name,
            params,
            RenderCandidatePreviewParams,
            crate::tools::render_candidate_preview
        ),
        "get_render_job" => {
            route!(
                tool_name,
                params,
                GetRenderJobParams,
                crate::tools::get_render_job
            )
        }
        "cancel_render_job" => route!(
            tool_name,
            params,
            CancelRenderJobParams,
            crate::tools::cancel_render_job
        ),
        // ── Debug tools ─────────────────────────────────────────────
        "get_command_trace" => route!(
            tool_name,
            params,
            GetCommandTraceParams,
            crate::tools::get_command_trace
        ),
        "get_snapshot_diff" => route!(
            tool_name,
            params,
            SnapshotDiffParams,
            crate::tools::get_snapshot_diff
        ),
        "get_candidate_diff" => route!(
            tool_name,
            params,
            CandidateDiffParams,
            crate::tools::get_candidate_diff
        ),
        "isolate_layer" => {
            route!(
                tool_name,
                params,
                IsolateLayerParams,
                crate::tools::isolate_layer
            )
        }
        "solo_segment" => {
            route!(
                tool_name,
                params,
                SoloSegmentParams,
                crate::tools::solo_segment
            )
        }
        "get_failure_report" => route!(
            tool_name,
            params,
            GetFailureReportParams,
            crate::tools::get_failure_report
        ),
        "create_human_review_packet" => route!(
            tool_name,
            params,
            CreateHumanReviewPacketParams,
            crate::tools::create_human_review_packet
        ),
        // ── Learning tools ──────────────────────────────────────────
        "get_lessons" => {
            route!(
                tool_name,
                params,
                GetLessonsParams,
                crate::tools::get_lessons
            )
        }
        "record_lesson" => {
            route!(
                tool_name,
                params,
                RecordLessonParams,
                crate::tools::record_lesson
            )
        }
        "get_repair_templates" => route!(
            tool_name,
            params,
            GetRepairTemplatesParams,
            crate::tools::get_repair_templates
        ),
        "update_calibration_profile" => route!(
            tool_name,
            params,
            UpdateCalibrationProfileParams,
            crate::tools::update_calibration_profile
        ),
        // ── Engineering tools ───────────────────────────────────────
        "get_next_task" => {
            route!(
                tool_name,
                params,
                GetNextTaskParams,
                crate::tools::get_next_task
            )
        }
        "claim_task" => {
            route!(tool_name, params, ClaimTaskParams, crate::tools::claim_task)
        }
        "submit_task_result" => route!(
            tool_name,
            params,
            SubmitTaskResultParams,
            crate::tools::submit_task_result
        ),
        "run_acceptance_tests" => route!(
            tool_name,
            params,
            RunAcceptanceTestsParams,
            crate::tools::run_acceptance_tests
        ),
        "get_eval_report" => {
            route!(
                tool_name,
                params,
                GetEvalReportParams,
                crate::tools::get_eval_report
            )
        }
        "update_docs" => {
            route!(
                tool_name,
                params,
                UpdateDocsParams,
                crate::tools::update_docs
            )
        }
        "create_adr" => {
            route!(tool_name, params, CreateAdrParams, crate::tools::create_adr)
        }
        // ── Unknown ─────────────────────────────────────────────────
        _ => unknown_tool_receipt(tool_name),
    }
}

/// Wrap the capability manifest into a `Succeeded` receipt for `get_capabilities`.
fn capabilities_receipt() -> ToolReceipt {
    let caps = crate::capabilities::get_capabilities();
    let summary = serde_json::to_string(&caps)
        .map(|json| format!("Capabilities manifest: {json}"))
        .unwrap_or_else(|_| "Capabilities manifest (serialization failed)".to_string());
    ToolReceipt {
        tool: "get_capabilities".to_string(),
        status: ToolStatus::Succeeded,
        summary,
        score_delta: BTreeMap::from([
            (
                "capabilities.features".to_string(),
                caps.features.len().to_string(),
            ),
            (
                "capabilities.tools".to_string(),
                caps.tools.len().to_string(),
            ),
            (
                "capabilities.resources".to_string(),
                caps.resources.len().to_string(),
            ),
        ]),
        evidence_uris: vec!["system://capabilities".to_string()],
        focus_uris: vec![],
        next_actions: vec![
            "get_contracts".to_string(),
            "get_onboarding_packet".to_string(),
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

/// `run_validation` needs `(params, segments, level)`.
///
/// Wire contract (beyond the typed [`RunValidationParams`] fields):
/// - `project`: optional serialized [`Project`]. At L2 its
///   `arrangement.segments` are the segments under validation; at `level >= 5`
///   the whole project (brief, config, arrangement) also feeds the brief
///   compliance plane, producing `BRIEF_*` diagnostics (absent → no segments
///   and no brief plane).
/// - `level`: optional validation level, defaults to `2` (L2 hard gates).
fn run_validation_from_params(params: serde_json::Value) -> ToolReceipt {
    let rp = match serde_json::from_value::<RunValidationParams>(params.clone()) {
        Ok(p) => p,
        Err(e) => {
            return crate::tools::deserialize_error_receipt("run_validation", "params", &e);
        }
    };
    let level = params
        .get("level")
        .and_then(serde_json::Value::as_u64)
        .unwrap_or(2) as u8;
    // With a project payload, run the full plane (L2 symbolic + L5 brief
    // compliance when level >= 5). Without one, fall back to segments-only.
    match params
        .get("project")
        .and_then(|p| serde_json::from_value::<scorestrata_core::domain::Project>(p.clone()).ok())
    {
        Some(project) => crate::tools::run_validation_with_project(&rp, &project, level),
        None => crate::tools::run_validation(&rp, &[], level),
    }
}

/// Receipt for an unregistered tool name.
fn unknown_tool_receipt(tool_name: &str) -> ToolReceipt {
    ToolReceipt {
        tool: tool_name.to_string(),
        status: ToolStatus::Failed,
        summary: format!(
            "Unknown tool '{tool_name}'. See get_capabilities for the registered catalog."
        ),
        score_delta: BTreeMap::new(),
        evidence_uris: vec![],
        focus_uris: vec!["system://capabilities".to_string()],
        next_actions: vec!["get_capabilities".to_string()],
        cost: ToolCost::default(),
        warnings: vec![format!(
            "'{tool_name}' is not in the registered tool catalog"
        )],
        dry_run: false,
        diagnostics: vec![],
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use scorestrata_core::ids::ProjectId;

    #[test]
    fn every_registered_tool_dispatches_without_unknown_marker() {
        // Fuzz gate: each registered name must route to an implementation, not
        // the unknown-tool fallback. Dispatching with `{}` also forces every
        // tool to tolerate an empty payload (transparent failure, never panic),
        // so future additions must keep empty-payload robustness.
        let caps = crate::capabilities::get_capabilities();
        for tool in &caps.tools {
            let receipt = dispatch_tool(tool.name, serde_json::json!({}));
            assert_eq!(receipt.tool, tool.name, "tool routed to wrong function");
            assert!(
                !receipt.summary.contains("Unknown tool"),
                "{} was not routed to an implementation",
                tool.name
            );
            assert!(
                matches!(
                    receipt.status,
                    ToolStatus::Succeeded | ToolStatus::Failed | ToolStatus::Blocked
                ),
                "{} returned an invalid status",
                tool.name
            );
        }
    }

    #[test]
    fn dispatches_standard_tool_with_typed_params() {
        let receipt = dispatch_tool(
            "get_policy_profile",
            serde_json::json!({ "project_id": "proj_1" }),
        );
        assert_eq!(receipt.tool, "get_policy_profile");
        assert!(matches!(receipt.status, ToolStatus::Succeeded));
    }

    #[test]
    fn unknown_tool_returns_failed_receipt() {
        let receipt = dispatch_tool("no_such_tool", serde_json::json!({}));
        assert!(matches!(receipt.status, ToolStatus::Failed));
        assert!(receipt.summary.contains("Unknown tool"));
        assert_eq!(receipt.tool, "no_such_tool");
    }

    #[test]
    fn bad_params_return_transparent_deserialize_receipt() {
        let receipt = dispatch_tool(
            "get_policy_profile",
            serde_json::json!({ "project_id": 42 }),
        );
        assert!(matches!(receipt.status, ToolStatus::Failed));
        assert!(receipt.summary.contains("Could not deserialize"));
        assert_eq!(receipt.tool, "get_policy_profile");
    }

    #[test]
    fn no_param_tools_dispatch_to_success() {
        for name in ["get_capabilities", "get_contracts", "get_onboarding_packet"] {
            let receipt = dispatch_tool(name, serde_json::json!({}));
            assert_eq!(receipt.tool, name);
            assert!(
                matches!(receipt.status, ToolStatus::Succeeded),
                "{name}: {:?}",
                receipt.status
            );
        }
    }
    #[test]
    fn capabilities_receipt_contains_manifest() {
        let receipt = dispatch_tool("get_capabilities", serde_json::json!({}));
        assert!(receipt.summary.contains("Capabilities manifest"));
        assert!(receipt.summary.contains("run_validation"));
        // Derive the expected count from the catalog so adding a 61st tool
        // never silently breaks this test.
        let expected = crate::capabilities::get_capabilities()
            .tools
            .len()
            .to_string();
        assert_eq!(
            receipt
                .score_delta
                .get("capabilities.tools")
                .map(String::as_str),
            Some(expected.as_str())
        );
    }

    #[test]
    fn run_validation_dispatches_with_project_payload() {
        let project = scorestrata_testkit::melodic_techno_project();
        let receipt = dispatch_tool(
            "run_validation",
            serde_json::json!({
                "project_id": project.id.to_string(),
                "project": project,
                "level": 2,
            }),
        );
        assert_eq!(receipt.tool, "run_validation");
        assert!(receipt.summary.contains("diagnostics"));
        assert_eq!(
            receipt.focus_uris[0],
            format!("project://{}/validation/latest", project.id)
        );
    }

    #[test]
    fn run_validation_without_project_payload_still_receipts() {
        let receipt = dispatch_tool(
            "run_validation",
            serde_json::json!({ "project_id": "proj_1" }),
        );
        assert_eq!(receipt.tool, "run_validation");
        assert!(matches!(
            receipt.status,
            ToolStatus::Succeeded | ToolStatus::Failed
        ));
    }

    #[test]
    fn mutation_tool_dispatches_real_project_payload() {
        let project = scorestrata_testkit::melodic_techno_project();
        let pattern_id = project.patterns[0].id.to_string();
        let receipt = dispatch_tool(
            "apply_motivic_variation",
            serde_json::json!({
                "project_id": project.id.to_string(),
                "pattern_id": pattern_id,
                "variation": "retrograde",
                "semitones": 0,
                "project": project,
            }),
        );
        assert_eq!(receipt.tool, "apply_motivic_variation");
        assert!(
            matches!(receipt.status, ToolStatus::Succeeded | ToolStatus::Failed),
            "expected succeeded or transparent failure, got {:?}",
            receipt.status
        );
    }

    #[test]
    fn project_id_roundtrips_through_dispatcher_typing() {
        let id = ProjectId::new();
        let receipt = dispatch_tool(
            "get_project_summary",
            serde_json::json!({ "project_id": id.to_string() }),
        );
        assert_eq!(receipt.tool, "get_project_summary");
    }

    // The telemetry registry is process-wide, so tests that assert on it must
    // be serialized against each other (via the shared TEST_LOCK) and use
    // probe names no other test dispatches, keeping exact counts immune to
    // parallel-test pollution.

    #[test]
    fn dispatch_records_tool_calls_in_telemetry() {
        let _guard = crate::telemetry::TEST_LOCK.lock().unwrap();
        // §15.5: every dispatched call is accounted in the telemetry registry.
        crate::telemetry::reset();
        dispatch_tool("telemetry_probe_a", serde_json::json!({}));
        dispatch_tool("telemetry_probe_a", serde_json::json!({}));
        dispatch_tool("telemetry_probe_b", serde_json::json!({}));
        dispatch_tool("definitely_not_a_tool", serde_json::json!({}));

        let snap = crate::telemetry::snapshot();
        assert_eq!(snap.tool_calls.get("telemetry_probe_a"), Some(&2));
        assert_eq!(snap.tool_calls.get("telemetry_probe_b"), Some(&1));
        assert_eq!(
            snap.tool_calls.get("definitely_not_a_tool"),
            Some(&1),
            "unknown tools are still measured"
        );
        assert!(snap.total_tool_calls() >= 4);
    }

    #[test]
    fn get_telemetry_tool_serves_snapshot() {
        let _guard = crate::telemetry::TEST_LOCK.lock().unwrap();
        crate::telemetry::reset();
        dispatch_tool("telemetry_probe_c", serde_json::json!({}));
        dispatch_tool("telemetry_probe_c", serde_json::json!({}));

        let receipt = dispatch_tool("get_telemetry", serde_json::json!({}));
        assert_eq!(receipt.tool, "get_telemetry");
        assert!(matches!(receipt.status, ToolStatus::Succeeded));
        assert!(receipt.summary.contains("tool call(s)"));
        assert!(receipt.summary.contains("telemetry_probe_cx2"));
        let tool_calls = receipt
            .score_delta
            .get("telemetry.tool_calls")
            .and_then(|v| v.parse::<u32>().ok())
            .unwrap_or(0);
        assert!(tool_calls >= 2, "telemetry serves the recorded count");
    }
}
