//! MCP capability discovery — allows agents to discover available tools, resources,
//! and features dynamically via `system://capabilities`.
//!
//! The architecture spec requires: "Agents should be able to ask: <system://capabilities>"

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
    /// Side effects (read-only, mutating).
    pub side_effects: &'static str,
    /// Whether dry-run is supported.
    pub supports_dry_run: bool,
    /// Safety class.
    pub safety_class: &'static str,
    /// Budget class.
    pub budget_class: &'static str,
}

/// Budget capabilities.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BudgetCapabilities {
    /// Whether budgets are supported.
    pub supports_budgets: bool,
    /// Maximum render seconds.
    pub max_render_seconds: u32,
}

/// Get the system capability manifest.
///
/// Returns the full manifest of available features, tools, and resources
/// that an agent can discover and use.
#[must_use]
pub fn get_capabilities() -> CapabilityManifest {
    CapabilityManifest {
        system: "scorestrata",
        version: env!("CARGO_PKG_VERSION"),
        schema_version: "0.3",
        features: vec![
            "symbolic_compiler",
            "validator_v1",
            "midi_export",
            "snapshots",
            "chord_tone_validation",
            "transition_heuristics",
            "scoring_profiles",
        ],
        tools: vec![
            ToolCapability {
                name: "run_validation",
                version: "0.3.0",
                stability: "alpha",
                side_effects: "read-only",
                supports_dry_run: false,
                safety_class: "read",
                budget_class: "low",
            },
            ToolCapability {
                name: "apply_patch",
                version: "0.3.0",
                stability: "alpha",
                side_effects: "mutating",
                supports_dry_run: true,
                safety_class: "write",
                budget_class: "medium",
            },
        ],
        resources: vec![
            "system://capabilities",
            "system://contracts",
            "project://<id>/summary",
            "project://<id>/validation/latest",
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
            assert!(["read-only", "mutating"].contains(&tool.side_effects));
            assert!(["read", "write", "destructive"].contains(&tool.safety_class));
        }
    }

    #[test]
    #[expect(clippy::unwrap_used, reason = "Test assertions")]
    fn manifest_serializes_to_json() {
        let caps = get_capabilities();
        let json = serde_json::to_string_pretty(&caps).unwrap();
        assert!(json.contains("scorestrata"));
        assert!(json.contains("run_validation"));
    }
}
