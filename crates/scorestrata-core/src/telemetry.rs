//! Telemetry — cost accounting and performance tracking for agents and systems.
//!
//! Architecture spec §15.5 requires tracking:
//! - tool call counts, CPU seconds, render seconds
//! - disk usage, cache hit rate, iteration count
//! - no-improvement streaks, escalation frequency
//! - rollback frequency, metric regression rate
//! - cost per successful patch, cost per finalized project

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

/// A telemetry snapshot — the current state of all tracked metrics.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[must_use]
pub struct TelemetrySnapshot {
    /// Tool call counts by tool name.
    pub tool_calls: BTreeMap<String, u32>,
    /// Total CPU seconds consumed.
    pub cpu_seconds: f64,
    /// Total render seconds consumed.
    pub render_seconds: f64,
    /// Disk usage in megabytes.
    pub disk_mb: f64,
    /// Cache hit rate (0.0–1.0).
    pub cache_hit_rate: f32,
    /// Total iterations completed.
    pub iteration_count: u32,
    /// Current no-improvement streak.
    pub no_improvement_streak: u32,
    /// Maximum no-improvement streak reached.
    pub max_no_improvement_streak: u32,
    /// Number of escalations to human.
    pub escalation_count: u32,
    /// Number of rollbacks performed.
    pub rollback_count: u32,
    /// Number of metric regressions detected.
    pub regression_count: u32,
    /// Number of successful patches.
    pub successful_patches: u32,
    /// Total patches attempted.
    pub total_patches: u32,
    /// Cost per successful patch (CPU seconds).
    pub cost_per_successful_patch: f64,
    /// Session start timestamp.
    pub session_start: Option<String>,
    /// Last update timestamp.
    pub last_updated: Option<String>,
}

impl TelemetrySnapshot {
    /// Create a new empty snapshot.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Record a tool call.
    pub fn record_tool_call(&mut self, tool_name: &str) {
        *self.tool_calls.entry(tool_name.to_string()).or_insert(0) += 1;
    }

    /// Record a patch attempt.
    pub fn record_patch_attempt(&mut self, successful: bool) {
        self.total_patches += 1;
        if successful {
            self.successful_patches += 1;
        }
    }

    /// Compute the success rate of patches (0.0–1.0).
    #[must_use]
    pub fn patch_success_rate(&self) -> f32 {
        if self.total_patches == 0 {
            return 0.0;
        }
        self.successful_patches as f32 / self.total_patches as f32
    }

    /// Compute the escalation rate (escalations per iteration).
    #[must_use]
    pub fn escalation_rate(&self) -> f32 {
        if self.iteration_count == 0 {
            return 0.0;
        }
        self.escalation_count as f32 / self.iteration_count as f32
    }

    /// Compute the rollback rate (rollbacks per iteration).
    #[must_use]
    pub fn rollback_rate(&self) -> f32 {
        if self.iteration_count == 0 {
            return 0.0;
        }
        self.rollback_count as f32 / self.iteration_count as f32
    }

    /// Compute the regression rate (regressions per iteration).
    #[must_use]
    pub fn regression_rate(&self) -> f32 {
        if self.iteration_count == 0 {
            return 0.0;
        }
        self.regression_count as f32 / self.iteration_count as f32
    }

    /// Get the top N most-called tools.
    #[must_use]
    pub fn top_tools(&self, n: usize) -> Vec<(&str, u32)> {
        let mut tools: Vec<(&str, u32)> = self
            .tool_calls
            .iter()
            .map(|(k, v)| (k.as_str(), *v))
            .collect();
        tools.sort_by(|a, b| b.1.cmp(&a.1));
        tools.into_iter().take(n).collect()
    }

    /// Total tool calls across all tools.
    #[must_use]
    pub fn total_tool_calls(&self) -> u32 {
        self.tool_calls.values().sum()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn telemetry_snapshot_starts_empty() {
        let snap = TelemetrySnapshot::new();
        assert_eq!(snap.total_patches, 0);
        assert_eq!(snap.successful_patches, 0);
        assert!(snap.tool_calls.is_empty());
    }

    #[test]
    fn telemetry_records_tool_calls() {
        let mut snap = TelemetrySnapshot::new();
        snap.record_tool_call("run_validation");
        snap.record_tool_call("run_validation");
        snap.record_tool_call("apply_patch");
        assert_eq!(snap.total_tool_calls(), 3);
        assert_eq!(snap.tool_calls["run_validation"], 2);
        assert_eq!(snap.tool_calls["apply_patch"], 1);
    }

    #[test]
    fn telemetry_records_patch_attempts() {
        let mut snap = TelemetrySnapshot::new();
        snap.record_patch_attempt(true);
        snap.record_patch_attempt(true);
        snap.record_patch_attempt(false);
        assert_eq!(snap.total_patches, 3);
        assert_eq!(snap.successful_patches, 2);
        assert!((snap.patch_success_rate() - 0.6667).abs() < 0.01);
    }

    #[test]
    fn telemetry_computes_rates() {
        let mut snap = TelemetrySnapshot::new();
        snap.iteration_count = 10;
        snap.escalation_count = 2;
        snap.rollback_count = 1;
        snap.regression_count = 3;
        assert!((snap.escalation_rate() - 0.2).abs() < f32::EPSILON);
        assert!((snap.rollback_rate() - 0.1).abs() < f32::EPSILON);
        assert!((snap.regression_rate() - 0.3).abs() < f32::EPSILON);
    }

    #[test]
    fn telemetry_top_tools_sorted() {
        let mut snap = TelemetrySnapshot::new();
        snap.record_tool_call("apply_patch");
        snap.record_tool_call("apply_patch");
        snap.record_tool_call("apply_patch");
        snap.record_tool_call("run_validation");
        snap.record_tool_call("run_validation");
        let top = snap.top_tools(2);
        assert_eq!(top[0], ("apply_patch", 3));
        assert_eq!(top[1], ("run_validation", 2));
    }

    #[test]
    fn telemetry_rates_zero_when_no_iterations() {
        let snap = TelemetrySnapshot::new();
        assert!((snap.escalation_rate()).abs() < f32::EPSILON);
        assert!((snap.rollback_rate()).abs() < f32::EPSILON);
        assert!((snap.regression_rate()).abs() < f32::EPSILON);
        assert!((snap.patch_success_rate()).abs() < f32::EPSILON);
    }

    #[test]
    fn telemetry_serializes() {
        let mut snap = TelemetrySnapshot::new();
        snap.record_tool_call("run_validation");
        snap.cpu_seconds = 42.5;
        snap.render_seconds = 12.3;
        #[expect(clippy::unwrap_used, reason = "Test assertion")]
        let json = serde_json::to_string_pretty(&snap).unwrap();
        assert!(json.contains("run_validation"));
        assert!(json.contains("42.5"));
    }
}
