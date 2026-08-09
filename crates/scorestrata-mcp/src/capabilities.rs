//! MCP capability discovery — allows agents to discover available tools, resources,
//! and features dynamically via `system://capabilities`.
//!
//! The architecture spec requires: "Agents should be able to ask: <system://capabilities>"
//!
//! Implements §10.5's full tool catalog with contract metadata (stability,
//! side effects, safety class, dry-run support, budget class) so agents can
//! discover valid actions, avoid unsafe calls, and understand costs.

use serde::{Deserialize, Serialize};

/// System capability manifest.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CapabilityManifest {
    /// System name.
    pub system: &'static str,
    /// System version.
    pub version: &'static str,
    /// Schema version.
    pub schema_version: &'static str,
    /// Available features.
    pub features: Vec<&'static str>,
    /// Available MCP tools.
    pub tools: Vec<ToolCapability>,
    /// Available MCP resources.
    pub resources: Vec<&'static str>,
    /// Budget support info.
    pub budgets: BudgetCapabilities,
}

/// A tool's capability metadata.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolCapability {
    /// Tool name.
    pub name: &'static str,
    /// Tool version.
    pub version: &'static str,
    /// Stability level (alpha, beta, stable).
    pub stability: &'static str,
    /// Side effects (read-only, mutating, render).
    pub side_effects: &'static str,
    /// Whether dry-run is supported.
    pub supports_dry_run: bool,
    /// Safety class.
    pub safety_class: &'static str,
    /// Budget class.
    pub budget_class: &'static str,
    /// Tool category (governance, project, composition, patch, validation, render, debug, learning, engineering).
    pub category: &'static str,
}

/// Budget capabilities.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BudgetCapabilities {
    /// Whether budgets are supported.
    pub supports_budgets: bool,
    /// Maximum render seconds.
    pub max_render_seconds: u32,
}

/// Build the complete tool catalog from the architecture spec §10.5.
fn tool_catalog() -> Vec<ToolCapability> {
    // (name, side_effects, supports_dry_run, safety_class, budget_class, category)
    const TOOLS: &[(&str, &str, bool, &str, &str, &str)] = &[
        // Governance tools
        (
            "get_capabilities",
            "read-only",
            false,
            "read",
            "low",
            "governance",
        ),
        (
            "get_contracts",
            "read-only",
            false,
            "read",
            "low",
            "governance",
        ),
        (
            "get_telemetry",
            "read-only",
            false,
            "read",
            "low",
            "governance",
        ),
        (
            "get_policy_profile",
            "read-only",
            false,
            "read",
            "low",
            "governance",
        ),
        (
            "check_budget",
            "read-only",
            false,
            "read",
            "low",
            "governance",
        ),
        // Project tools
        (
            "create_project",
            "mutating",
            false,
            "write",
            "low",
            "project",
        ),
        (
            "generate_config_from_brief",
            "mutating",
            true,
            "write",
            "low",
            "project",
        ),
        (
            "validate_config",
            "read-only",
            false,
            "read",
            "low",
            "project",
        ),
        (
            "approve_config",
            "mutating",
            false,
            "write",
            "low",
            "project",
        ),
        (
            "get_project_summary",
            "read-only",
            false,
            "read",
            "low",
            "project",
        ),
        (
            "get_onboarding_packet",
            "read-only",
            false,
            "read",
            "low",
            "project",
        ),
        // Composition tools
        (
            "plan_arrangement",
            "mutating",
            true,
            "write",
            "low",
            "composition",
        ),
        (
            "create_pattern",
            "mutating",
            true,
            "write",
            "low",
            "composition",
        ),
        (
            "write_pattern_notes",
            "mutating",
            true,
            "write",
            "medium",
            "composition",
        ),
        (
            "write_pattern_steps",
            "mutating",
            true,
            "write",
            "medium",
            "composition",
        ),
        (
            "apply_motivic_variation",
            "mutating",
            true,
            "write",
            "medium",
            "composition",
        ),
        (
            "place_pattern_in_segment",
            "mutating",
            true,
            "write",
            "medium",
            "composition",
        ),
        (
            "set_layer_instrument",
            "mutating",
            true,
            "write",
            "low",
            "composition",
        ),
        (
            "set_segment_goals",
            "mutating",
            true,
            "write",
            "medium",
            "composition",
        ),
        (
            "set_energy_curve",
            "mutating",
            true,
            "write",
            "medium",
            "composition",
        ),
        // Patch tools
        ("apply_patch", "mutating", true, "write", "medium", "patch"),
        ("dry_run_patch", "read-only", false, "read", "low", "patch"),
        (
            "quantize_layer_to_scale",
            "mutating",
            true,
            "write",
            "medium",
            "patch",
        ),
        (
            "harmonize_melody",
            "mutating",
            true,
            "write",
            "medium",
            "patch",
        ),
        (
            "add_drum_fill",
            "mutating",
            true,
            "write",
            "medium",
            "patch",
        ),
        (
            "add_transition_package",
            "mutating",
            true,
            "write",
            "medium",
            "patch",
        ),
        (
            "increase_segment_density",
            "mutating",
            true,
            "write",
            "medium",
            "patch",
        ),
        (
            "reduce_masking",
            "mutating",
            true,
            "write",
            "medium",
            "patch",
        ),
        (
            "normalize_loudness_target",
            "mutating",
            true,
            "write",
            "medium",
            "patch",
        ),
        // Validation tools
        (
            "run_validation",
            "read-only",
            false,
            "read",
            "low",
            "validation",
        ),
        (
            "check_regressions",
            "read-only",
            false,
            "read",
            "low",
            "validation",
        ),
        (
            "get_validation_report",
            "read-only",
            false,
            "read",
            "low",
            "validation",
        ),
        (
            "get_audit_summary",
            "read-only",
            false,
            "read",
            "low",
            "validation",
        ),
        (
            "explain_metric",
            "read-only",
            false,
            "read",
            "low",
            "validation",
        ),
        (
            "get_root_causes",
            "read-only",
            false,
            "read",
            "low",
            "validation",
        ),
        (
            "get_metric_evidence",
            "read-only",
            false,
            "read",
            "low",
            "validation",
        ),
        (
            "get_render_manifest",
            "read-only",
            false,
            "read",
            "low",
            "render",
        ),
        (
            "suggest_fixes",
            "read-only",
            false,
            "read",
            "low",
            "validation",
        ),
        (
            "compare_candidates",
            "read-only",
            false,
            "read",
            "low",
            "validation",
        ),
        // Render tools
        (
            "render_project",
            "render",
            false,
            "render",
            "high",
            "render",
        ),
        (
            "render_segment",
            "render",
            false,
            "render",
            "high",
            "render",
        ),
        ("render_stems", "render", false, "render", "high", "render"),
        (
            "render_candidate_preview",
            "render",
            false,
            "render",
            "high",
            "render",
        ),
        (
            "get_render_job",
            "read-only",
            false,
            "read",
            "low",
            "render",
        ),
        (
            "cancel_render_job",
            "mutating",
            false,
            "write",
            "low",
            "render",
        ),
        // Debug tools
        (
            "get_command_trace",
            "read-only",
            false,
            "read",
            "low",
            "debug",
        ),
        (
            "get_snapshot_diff",
            "read-only",
            false,
            "read",
            "low",
            "debug",
        ),
        (
            "get_candidate_diff",
            "read-only",
            false,
            "read",
            "low",
            "debug",
        ),
        ("isolate_layer", "read-only", false, "read", "low", "debug"),
        (
            "get_timeline_view",
            "read-only",
            false,
            "read",
            "low",
            "debug",
        ),
        (
            "get_arrangement_templates",
            "read-only",
            false,
            "read",
            "low",
            "debug",
        ),
        (
            "write_automation",
            "mutating",
            false,
            "write",
            "low",
            "composition",
        ),
        ("set_track_fx", "mutating", false, "write", "low", "mix"),
        ("set_track_mix", "mutating", false, "write", "low", "mix"),
        (
            "apply_genre_mix_preset",
            "mutating",
            false,
            "write",
            "low",
            "mix",
        ),
        (
            "apply_arrangement_template",
            "mutating",
            false,
            "write",
            "low",
            "arrangement",
        ),
        (
            "extend_arrangement",
            "mutating",
            false,
            "write",
            "low",
            "arrangement",
        ),
        (
            "import_reference_profile",
            "mutating",
            false,
            "write",
            "low",
            "render",
        ),
        (
            "set_layer_register",
            "mutating",
            false,
            "write",
            "low",
            "composition",
        ),
        (
            "repeat_pattern_in_segment",
            "mutating",
            false,
            "write",
            "low",
            "composition",
        ),
        (
            "clone_pattern_transposed",
            "mutating",
            false,
            "write",
            "low",
            "composition",
        ),
        ("solo_segment", "read-only", false, "read", "low", "debug"),
        (
            "get_failure_report",
            "read-only",
            false,
            "read",
            "low",
            "debug",
        ),
        (
            "create_human_review_packet",
            "mutating",
            false,
            "write",
            "low",
            "debug",
        ),
        // Learning tools
        ("get_lessons", "read-only", false, "read", "low", "learning"),
        (
            "record_lesson",
            "mutating",
            false,
            "write",
            "low",
            "learning",
        ),
        (
            "get_repair_templates",
            "read-only",
            false,
            "read",
            "low",
            "learning",
        ),
        (
            "update_calibration_profile",
            "mutating",
            false,
            "write",
            "low",
            "learning",
        ),
        // Engineering tools
        (
            "get_next_task",
            "read-only",
            false,
            "read",
            "low",
            "engineering",
        ),
        (
            "claim_task",
            "mutating",
            false,
            "engineering",
            "low",
            "engineering",
        ),
        (
            "submit_task_result",
            "mutating",
            false,
            "engineering",
            "low",
            "engineering",
        ),
        (
            "run_acceptance_tests",
            "read-only",
            false,
            "read",
            "medium",
            "engineering",
        ),
        (
            "get_eval_report",
            "read-only",
            false,
            "read",
            "low",
            "engineering",
        ),
        (
            "update_docs",
            "mutating",
            false,
            "engineering",
            "low",
            "engineering",
        ),
        (
            "create_adr",
            "mutating",
            false,
            "engineering",
            "low",
            "engineering",
        ),
    ];

    TOOLS
        .iter()
        .map(
            |&(name, side_effects, dry_run, safety, budget, category)| ToolCapability {
                name,
                version: env!("CARGO_PKG_VERSION"),
                stability: "alpha",
                side_effects,
                supports_dry_run: dry_run,
                safety_class: safety,
                budget_class: budget,
                category,
            },
        )
        .collect()
}

/// Get the system capability manifest.
///
/// Returns the full manifest of available features, tools, and resources
/// that an agent can discover and use.
#[must_use]
pub fn get_capabilities() -> CapabilityManifest {
    let tools = tool_catalog();

    CapabilityManifest {
        system: "scorestrata",
        version: env!("CARGO_PKG_VERSION"),
        schema_version: "0.3",
        features: vec![
            "symbolic_compiler",
            "validator_v1",
            "midi_export",
            "snapshots",
            "candidate_branches",
            "eval_harness",
            "agent_kernel_l0_l4",
            "event_ledger",
            "replay_journal",
            "evidence_warehouse",
            "calibration_profiles",
            "patch_templates",
            "memory_tiers",
            "experiment_runner",
            "schema_registry",
            "offline_render",
            "audio_metrics",
            "engineering_plane",
            "capability_discovery",
            "multi_agent_coordination",
        ],
        tools,
        resources: vec![
            "system://capabilities",
            "system://contracts",
            "system://onboarding",
            "system://playbooks",
            "system://failure-taxonomy",
            "system://policy-profiles",
            "system://budget-templates",
            "system://calibration-profiles",
            "project://<id>/summary",
            "project://<id>/budget",
            "project://<id>/policy",
            "project://<id>/state",
            "project://<id>/provenance",
            "project://<id>/ledger",
            "project://<id>/branches",
            "project://<id>/candidates",
            "project://<id>/memory/lessons",
            "project://<id>/memory/repair-templates",
            "project://<id>/eval/latest",
            "project://<id>/evidence/latest",
            "project://<id>/telemetry/latest",
            "project://<id>/diff/snapshots/<a>/<b>",
            "project://<id>/diff/candidates/<a>/<b>",
            "project://<id>/trace/command/<command_id>",
            "project://<id>/replay/cursor/<cursor>",
            "project://<id>/lens/segment/<segment_id>",
            "project://<id>/lens/layer/<layer_id>",
            "project://<id>/lens/metric/<metric_id>",
            "project://<id>/lens/transition/<from>/<to>",
            "project://<id>/arrangement/templates",
            "project://<id>/render/jobs",
            "project://<id>/render/jobs/<job_id>",
            "project://<id>/human-review/latest",
            "engineering://specs",
            "engineering://specs/<spec_id>",
            "engineering://tasks/next",
            "engineering://tasks/<task_id>",
            "engineering://eval/latest",
            "engineering://golden/status",
            "engineering://traceability/<feature_id>",
        ],
        budgets: BudgetCapabilities {
            supports_budgets: true,
            max_render_seconds: 3600,
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn capabilities_manifest_is_valid() {
        let caps = get_capabilities();
        assert_eq!(caps.system, "scorestrata");
        assert!(!caps.features.is_empty());
        assert!(!caps.tools.is_empty());
        assert!(!caps.resources.is_empty());
        assert!(caps.budgets.supports_budgets);
    }

    #[test]
    fn every_tool_has_required_fields() {
        let caps = get_capabilities();
        for tool in &caps.tools {
            assert!(!tool.name.is_empty());
            assert!(!tool.version.is_empty());
            assert!(["alpha", "beta", "stable"].contains(&tool.stability));
            assert!(["read-only", "mutating", "render"].contains(&tool.side_effects));
            assert!(
                ["read", "write", "render", "engineering", "governance"]
                    .contains(&tool.safety_class)
            );
            assert!(!tool.category.is_empty());
        }
    }

    #[test]
    fn tool_catalog_covers_all_categories() {
        let caps = get_capabilities();
        let mut categories: Vec<&str> = caps.tools.iter().map(|t| t.category).collect();
        categories.sort();
        categories.dedup();
        for expected in [
            "governance",
            "project",
            "composition",
            "patch",
            "validation",
            "render",
            "debug",
            "learning",
            "engineering",
        ] {
            assert!(
                categories.contains(&expected),
                "missing category {expected}"
            );
        }
    }

    #[test]
    fn tool_names_are_unique() {
        let caps = get_capabilities();
        let mut names: Vec<&str> = caps.tools.iter().map(|t| t.name).collect();
        let original = names.len();
        names.sort();
        names.dedup();
        assert_eq!(names.len(), original, "tool names must be unique");
    }

    #[test]
    fn mutating_tools_support_dry_run_where_declared() {
        let caps = get_capabilities();
        for tool in caps
            .tools
            .iter()
            .filter(|t| t.side_effects == "mutating" && t.budget_class == "medium")
        {
            assert!(
                tool.supports_dry_run,
                "medium-cost mutating tool {} should support dry-run",
                tool.name
            );
        }
    }

    #[test]
    #[expect(clippy::unwrap_used, reason = "Test assertions")]
    fn manifest_serializes_to_json() {
        let caps = get_capabilities();
        let json = serde_json::to_string_pretty(&caps).unwrap();
        assert!(json.contains("scorestrata"));
        assert!(json.contains("run_validation"));
        assert!(json.contains("quantize_layer_to_scale"));
        assert!(json.contains("get_next_task"));
    }

    #[test]
    fn system_resources_are_discoverable() {
        let caps = get_capabilities();
        for expected in [
            "system://capabilities",
            "system://contracts",
            "system://onboarding",
        ] {
            assert!(caps.resources.contains(&expected), "missing {expected}");
        }
    }

    #[test]
    fn features_include_all_planes() {
        let caps = get_capabilities();
        for expected in [
            "symbolic_compiler",
            "validator_v1",
            "agent_kernel_l0_l4",
            "schema_registry",
            "engineering_plane",
            "calibration_profiles",
        ] {
            assert!(
                caps.features.contains(&expected),
                "missing feature {expected}"
            );
        }
    }
}
