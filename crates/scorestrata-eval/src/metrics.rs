//! Eval harness metrics — system, agent, and engineering performance tracking.
//!
//! Architecture spec §16.8 defines three categories of eval metrics:
//!
//! ### System eval metrics
//! - average validation score, hard failure rate, render determinism
//! - golden pass rate, CPU time, render time, cache hit rate
//! - regression rate, metric stability, schema/contract pass rate
//!
//! ### Agent eval metrics
//! - task success rate, iterations to target, cost per successful patch
//! - rollback rate, escalation rate, no-improvement streak frequency
//! - context efficiency, tool error rate, time to first successful repair
//!
//! ### Engineering eval metrics
//! - spec-to-merge time, CI pass rate, flaky test rate
//! - golden regression rate, docs drift, test coverage, mutation score

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

// ── System Eval Metrics (§16.8) ─────────────────────────────────────

/// System-level evaluation metrics for a Scorestrata project.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct SystemEvalMetrics {
    /// Average validation score across all levels (0.0–1.0).
    pub avg_validation_score: f32,
    /// Hard failure rate (fraction of validation runs with hard failures).
    pub hard_failure_rate: f32,
    /// Render determinism score (0.0–1.0, 1.0 = fully deterministic).
    pub render_determinism: f32,
    /// Golden test pass rate.
    pub golden_pass_rate: f32,
    /// Total CPU time in seconds.
    pub cpu_seconds: f64,
    /// Total render time in seconds.
    pub render_seconds: f64,
    /// Cache hit rate (0.0–1.0).
    pub cache_hit_rate: f32,
    /// Regression rate (fraction of patches that caused regressions).
    pub regression_rate: f32,
    /// Metric stability (stddev of metric scores across runs, lower = more stable).
    pub metric_stability: f32,
    /// Schema/contract pass rate.
    pub schema_pass_rate: f32,
}

impl SystemEvalMetrics {
    /// Create an empty metrics instance.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Compute overall system health score (0.0–1.0).
    ///
    /// Weighted average of key metrics with penalty for regressions.
    #[must_use]
    pub fn health_score(&self) -> f32 {
        let base = self.avg_validation_score.mul_add(
            0.3,
            self.golden_pass_rate.mul_add(
                0.2,
                self.render_determinism.mul_add(
                    0.15,
                    self.cache_hit_rate
                        .mul_add(0.1, self.schema_pass_rate * 0.1),
                ),
            ),
        );
        // Penalize for regressions and hard failures
        let penalty = self
            .regression_rate
            .mul_add(0.05, self.hard_failure_rate * 0.1);
        (base - penalty).clamp(0.0, 1.0)
    }

    /// Serialize to JSON for MCP resource.
    #[must_use]
    pub fn to_json(&self) -> String {
        serde_json::to_string_pretty(self).unwrap_or_default()
    }
}

// ── Agent Eval Metrics (§16.8) ──────────────────────────────────────

/// Agent-level evaluation metrics.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct AgentEvalMetrics {
    /// Task success rate (fraction of tasks completed successfully).
    pub task_success_rate: f32,
    /// Average iterations to reach target score.
    pub avg_iterations_to_target: f32,
    /// Cost per successful patch (CPU seconds).
    pub cost_per_successful_patch: f64,
    /// Rollback rate (fraction of patches that required rollback).
    pub rollback_rate: f32,
    /// Escalation rate (fraction of sessions requiring human escalation).
    pub escalation_rate: f32,
    /// Average no-improvement streak before resolution.
    pub avg_no_improvement_streak: f32,
    /// Context efficiency (useful tokens / total tokens).
    pub context_efficiency: f32,
    /// Tool error rate (fraction of tool calls that failed).
    pub tool_error_rate: f32,
    /// Time to first successful repair in milliseconds.
    pub time_to_first_repair_ms: u64,
    /// Time to finalizable project in milliseconds.
    pub time_to_finalizable_ms: u64,
}

impl AgentEvalMetrics {
    /// Create an empty metrics instance.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Compute overall agent efficiency score (0.0–1.0).
    #[must_use]
    pub fn efficiency_score(&self) -> f32 {
        self.task_success_rate.mul_add(
            0.3,
            (1.0 - self.rollback_rate).mul_add(
                0.2,
                (1.0 - self.escalation_rate).mul_add(
                    0.2,
                    (1.0 - self.tool_error_rate).mul_add(0.15, self.context_efficiency * 0.15),
                ),
            ),
        )
    }
}

// ── Engineering Eval Metrics (§16.8) ────────────────────────────────

/// Engineering-level evaluation metrics.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct EngineeringEvalMetrics {
    /// Average spec-to-merge time in hours.
    pub avg_spec_to_merge_hours: f64,
    /// CI pass rate (fraction of CI runs that pass on first try).
    pub ci_pass_rate: f32,
    /// Flaky test rate (fraction of tests that are non-deterministic).
    pub flaky_test_rate: f32,
    /// Golden regression rate (fraction of golden tests that regress).
    pub golden_regression_rate: f32,
    /// Documentation drift score (0.0 = no drift, 1.0 = severe drift).
    pub docs_drift: f32,
    /// Test coverage (0.0–1.0).
    pub test_coverage: f32,
    /// Mutation score (fraction of mutants killed by tests).
    pub mutation_score: f32,
    /// PR rejection rate.
    pub pr_rejection_rate: f32,
}

impl EngineeringEvalMetrics {
    /// Create an empty metrics instance.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Compute overall engineering health score (0.0–1.0).
    #[must_use]
    pub fn health_score(&self) -> f32 {
        self.ci_pass_rate.mul_add(
            0.25,
            self.test_coverage.mul_add(
                0.2,
                self.mutation_score.mul_add(
                    0.2,
                    (1.0 - self.flaky_test_rate).mul_add(
                        0.15,
                        (1.0 - self.golden_regression_rate)
                            .mul_add(0.1, (1.0 - self.docs_drift) * 0.1),
                    ),
                ),
            ),
        )
    }
}

// ── Combined Eval Report ─────────────────────────────────────────────

/// Complete eval report combining all three metric categories.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FullEvalReport {
    /// System metrics.
    pub system: SystemEvalMetrics,
    /// Agent metrics.
    pub agent: AgentEvalMetrics,
    /// Engineering metrics.
    pub engineering: EngineeringEvalMetrics,
    /// Overall score (weighted combination).
    pub overall_score: f32,
    /// Timestamp.
    pub timestamp: String,
    /// Report version.
    pub version: String,
}

impl FullEvalReport {
    /// Create a new empty report.
    #[must_use]
    pub fn new() -> Self {
        let system = SystemEvalMetrics::new();
        let agent = AgentEvalMetrics::new();
        let engineering = EngineeringEvalMetrics::new();
        let overall = Self::compute_overall(&system, &agent, &engineering);

        Self {
            system,
            agent,
            engineering,
            overall_score: overall,
            timestamp: chrono::Utc::now().to_rfc3339(),
            version: env!("CARGO_PKG_VERSION").to_string(),
        }
    }

    /// Create from individual metric sets.
    #[must_use]
    pub fn from_metrics(
        system: SystemEvalMetrics,
        agent: AgentEvalMetrics,
        engineering: EngineeringEvalMetrics,
    ) -> Self {
        let overall = Self::compute_overall(&system, &agent, &engineering);
        Self {
            system,
            agent,
            engineering,
            overall_score: overall,
            timestamp: chrono::Utc::now().to_rfc3339(),
            version: env!("CARGO_PKG_VERSION").to_string(),
        }
    }

    /// Compute overall score: 40% system, 30% agent, 30% engineering.
    fn compute_overall(
        system: &SystemEvalMetrics,
        agent: &AgentEvalMetrics,
        engineering: &EngineeringEvalMetrics,
    ) -> f32 {
        system.health_score().mul_add(
            0.4,
            agent
                .efficiency_score()
                .mul_add(0.3, engineering.health_score() * 0.3),
        )
    }

    /// Compute overall score from current field values and update timestamp.
    pub fn recompute_overall(&mut self) {
        self.overall_score = Self::compute_overall(&self.system, &self.agent, &self.engineering);
        self.timestamp = chrono::Utc::now().to_rfc3339();
    }

    /// Serialize to JSON.
    #[must_use]
    pub fn to_json(&self) -> String {
        serde_json::to_string_pretty(self).unwrap_or_default()
    }

    /// Get a summary as key-value pairs for MCP resource.
    #[must_use]
    pub fn summary_pairs(&self) -> BTreeMap<String, String> {
        let mut pairs = BTreeMap::new();
        pairs.insert(
            "overall_score".to_string(),
            format!("{:.3}", self.overall_score),
        );
        pairs.insert(
            "system_health".to_string(),
            format!("{:.3}", self.system.health_score()),
        );
        pairs.insert(
            "agent_efficiency".to_string(),
            format!("{:.3}", self.agent.efficiency_score()),
        );
        pairs.insert(
            "engineering_health".to_string(),
            format!("{:.3}", self.engineering.health_score()),
        );
        pairs.insert(
            "golden_pass_rate".to_string(),
            format!("{:.3}", self.system.golden_pass_rate),
        );
        pairs.insert(
            "test_coverage".to_string(),
            format!("{:.3}", self.engineering.test_coverage),
        );
        pairs.insert("timestamp".to_string(), self.timestamp.clone());
        pairs
    }
}

impl Default for FullEvalReport {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn system_metrics_health_score() {
        let mut metrics = SystemEvalMetrics::new();
        metrics.avg_validation_score = 0.9;
        metrics.golden_pass_rate = 1.0;
        metrics.render_determinism = 1.0;
        metrics.cache_hit_rate = 0.8;
        metrics.schema_pass_rate = 1.0;
        metrics.regression_rate = 0.0;
        metrics.hard_failure_rate = 0.0;
        let score = metrics.health_score();
        assert!(score >= 0.8, "Expected health >= 0.8, got {score}");
    }

    #[test]
    fn system_metrics_penalizes_regressions() {
        let mut good = SystemEvalMetrics::new();
        good.avg_validation_score = 0.9;
        good.golden_pass_rate = 1.0;
        good.render_determinism = 1.0;
        let good_score = good.health_score();

        let mut bad = good.clone();
        bad.regression_rate = 0.5;
        let bad_score = bad.health_score();

        assert!(bad_score < good_score, "Regressions should lower score");
    }

    #[test]
    fn agent_metrics_efficiency_score() {
        let mut metrics = AgentEvalMetrics::new();
        metrics.task_success_rate = 0.95;
        metrics.rollback_rate = 0.05;
        metrics.escalation_rate = 0.1;
        metrics.tool_error_rate = 0.02;
        metrics.context_efficiency = 0.8;
        let score = metrics.efficiency_score();
        assert!(score > 0.7, "Expected efficiency > 0.7, got {score}");
    }

    #[test]
    fn engineering_metrics_health_score() {
        let mut metrics = EngineeringEvalMetrics::new();
        metrics.ci_pass_rate = 0.95;
        metrics.test_coverage = 0.85;
        metrics.mutation_score = 0.75;
        metrics.flaky_test_rate = 0.02;
        metrics.golden_regression_rate = 0.0;
        metrics.docs_drift = 0.1;
        let score = metrics.health_score();
        assert!(score > 0.7, "Expected health > 0.7, got {score}");
    }

    #[test]
    fn full_eval_report_overall_score() {
        let mut system = SystemEvalMetrics::new();
        system.avg_validation_score = 0.9;
        system.golden_pass_rate = 1.0;
        system.render_determinism = 1.0;

        let mut agent = AgentEvalMetrics::new();
        agent.task_success_rate = 0.9;

        let mut eng = EngineeringEvalMetrics::new();
        eng.ci_pass_rate = 0.95;
        eng.test_coverage = 0.8;

        let report = FullEvalReport::from_metrics(system, agent, eng);
        assert!(report.overall_score > 0.7);
        assert_eq!(report.version, env!("CARGO_PKG_VERSION"));
    }

    #[test]
    fn eval_report_summary_pairs() {
        let report = FullEvalReport::new();
        let pairs = report.summary_pairs();
        assert!(pairs.contains_key("overall_score"));
        assert!(pairs.contains_key("system_health"));
        assert!(pairs.contains_key("agent_efficiency"));
        assert!(pairs.contains_key("engineering_health"));
        assert!(pairs.contains_key("timestamp"));
    }

    #[test]
    fn eval_report_serializes_to_json() {
        let report = FullEvalReport::new();
        let json = report.to_json();
        assert!(json.contains("overall_score"));
        assert!(json.contains("system"));
        assert!(json.contains("agent"));
        assert!(json.contains("engineering"));
    }
}
