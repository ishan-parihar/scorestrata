//! Policy-as-code types for governance and safety.
//! Policies are declarative, versioned, and simulated in dry-run.
//! They gate mutations and ensure agent actions are safe.

use serde::{Deserialize, Serialize};

/// A policy profile containing all rules for a project.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PolicyProfile {
    /// Whether destructive commands are allowed.
    #[serde(default)]
    pub allow_destructive_commands: bool,
    /// Whether a snapshot is required before patching.
    #[serde(default = "default_true")]
    pub require_snapshot_before_patch: bool,
    /// Maximum allowed ratio of out-of-scale notes.
    #[serde(default = "default_max_oos")]
    pub max_out_of_scale_ratio: f32,
    /// Minimum chorus energy delta over verse.
    #[serde(default = "default_min_chorus_delta")]
    pub min_chorus_energy_delta: f32,
    /// Maximum true peak in dBTP.
    #[serde(default = "default_max_true_peak")]
    pub max_true_peak_dbtp: f32,
    /// Whether MIDI export is required on finalize.
    #[serde(default = "default_true")]
    pub require_midi_export_on_finalize: bool,
    /// Maximum candidate renders per segment.
    #[serde(default = "default_max_candidates")]
    pub max_candidate_renders_per_segment: u32,
}

const fn default_true() -> bool {
    true
}
const fn default_max_oos() -> f32 {
    0.05
}
const fn default_min_chorus_delta() -> f32 {
    0.15
}
const fn default_max_true_peak() -> f32 {
    -1.0
}
const fn default_max_candidates() -> u32 {
    8
}

impl Default for PolicyProfile {
    fn default() -> Self {
        Self {
            allow_destructive_commands: false,
            require_snapshot_before_patch: true,
            max_out_of_scale_ratio: 0.05,
            min_chorus_energy_delta: 0.15,
            max_true_peak_dbtp: -1.0,
            require_midi_export_on_finalize: true,
            max_candidate_renders_per_segment: 8,
        }
    }
}

/// The result of evaluating a policy against a command.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PolicyCheckResult {
    /// Whether the command is allowed.
    pub allowed: bool,
    /// Rules that were evaluated.
    pub rules_evaluated: Vec<PolicyRuleResult>,
    /// Override applied (if any).
    pub override_reason: Option<String>,
    /// Whether this check should be audited.
    pub audit_trail: bool,
}

/// The result of evaluating a single policy rule.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PolicyRuleResult {
    /// Rule name.
    pub rule: String,
    /// Whether it passed.
    pub passed: bool,
    /// Actual value (if numeric).
    pub value: Option<f32>,
    /// Limit value (if numeric).
    pub limit: Option<f32>,
    /// Explanation.
    pub explanation: Option<String>,
}
