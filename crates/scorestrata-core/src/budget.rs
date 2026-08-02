//! Budget envelope types for agent sessions.
//!
//! Every agent session must have a budget to prevent runaway costs,
//! infinite loops, and resource exhaustion.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::ids::BudgetId;

/// A budget envelope limiting an agent session.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Budget {
    /// Unique identifier.
    pub id: BudgetId,
    /// Project this budget is for.
    pub project_id: crate::ids::ProjectId,
    /// Maximum iterations (compile/validate cycles).
    pub max_iterations: u32,
    /// Current iteration count.
    pub current_iterations: u32,
    /// Maximum tool calls.
    pub max_tool_calls: u32,
    /// Current tool call count.
    pub current_tool_calls: u32,
    /// Maximum render time in seconds.
    pub max_render_seconds: f64,
    /// Current render time in seconds.
    pub current_render_seconds: f64,
    /// Maximum CPU seconds.
    pub max_cpu_seconds: f64,
    /// Current CPU seconds used.
    pub current_cpu_seconds: f64,
    /// Maximum disk usage in MB.
    pub max_disk_mb: u64,
    /// Maximum candidate branches.
    pub max_candidate_branches: u32,
    /// Current candidate branch count.
    pub current_candidate_branches: u32,
    /// Maximum consecutive rounds without improvement.
    pub max_no_improvement_rounds: u32,
    /// Current no-improvement streak.
    pub current_no_improvement_rounds: u32,
    /// Maximum human escalations.
    pub max_human_escalations: u32,
    /// Current human escalation count.
    pub current_human_escalations: u32,
    /// When this budget expires.
    pub expires_at: Option<DateTime<Utc>>,
}

impl Budget {
    /// Create a new budget with default limits.
    #[must_use]
    pub fn new(project_id: crate::ids::ProjectId) -> Self {
        Self {
            id: BudgetId::new(),
            project_id,
            max_iterations: 12,
            current_iterations: 0,
            max_tool_calls: 250,
            current_tool_calls: 0,
            max_render_seconds: 600.0,
            current_render_seconds: 0.0,
            max_cpu_seconds: 1800.0,
            current_cpu_seconds: 0.0,
            max_disk_mb: 2048,
            max_candidate_branches: 8,
            current_candidate_branches: 0,
            max_no_improvement_rounds: 3,
            current_no_improvement_rounds: 0,
            max_human_escalations: 2,
            current_human_escalations: 0,
            expires_at: None,
        }
    }

    /// Check if the iteration budget is exhausted.
    #[must_use]
    pub const fn iterations_exhausted(&self) -> bool {
        self.current_iterations >= self.max_iterations
    }

    /// Check if any budget limit is exhausted.
    #[must_use]
    pub const fn is_exhausted(&self) -> bool {
        self.iterations_exhausted()
            || self.current_tool_calls >= self.max_tool_calls
            || self.current_render_seconds >= self.max_render_seconds
            || self.current_cpu_seconds >= self.max_cpu_seconds
            || self.current_candidate_branches >= self.max_candidate_branches
    }

    /// Check if the no-improvement limit is reached.
    #[must_use]
    pub const fn stagnation_detected(&self) -> bool {
        self.current_no_improvement_rounds >= self.max_no_improvement_rounds
    }

    /// Get remaining budget as a fraction (0.0 = exhausted, 1.0 = full).
    #[must_use]
    #[expect(
        clippy::cast_precision_loss,
        clippy::cast_possible_truncation,
        reason = "Budget values are small enough for f32 precision"
    )]
    pub fn remaining_fraction(&self) -> f32 {
        let iteration_frac = 1.0 - (self.current_iterations as f32 / self.max_iterations as f32);
        let tool_frac = 1.0 - (self.current_tool_calls as f32 / self.max_tool_calls as f32);
        let render_frac =
            1.0 - (self.current_render_seconds as f32 / self.max_render_seconds as f32);
        let cpu_frac = 1.0 - (self.current_cpu_seconds as f32 / self.max_cpu_seconds as f32);
        iteration_frac
            .min(tool_frac)
            .min(render_frac)
            .min(cpu_frac)
            .max(0.0)
    }

    /// Increment the iteration counter.
    pub const fn record_iteration(&mut self) {
        self.current_iterations += 1;
    }

    /// Increment the tool call counter.
    pub const fn record_tool_call(&mut self) {
        self.current_tool_calls += 1;
    }

    /// Record render time.
    pub fn record_render_seconds(&mut self, seconds: f64) {
        self.current_render_seconds += seconds;
    }

    /// Record CPU time.
    pub fn record_cpu_seconds(&mut self, seconds: f64) {
        self.current_cpu_seconds += seconds;
    }

    /// Reset the no-improvement counter (called when improvement is detected).
    pub const fn reset_no_improvement(&mut self) {
        self.current_no_improvement_rounds = 0;
    }

    /// Increment the no-improvement counter.
    pub const fn record_no_improvement(&mut self) {
        self.current_no_improvement_rounds += 1;
    }
}
