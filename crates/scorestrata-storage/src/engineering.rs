//! Engineering plane primitives — task results and eval summaries.
//!
//! The architecture tool catalog (§10.5) lists `submit_task_result` and
//! `get_eval_report` under Engineering tools. Rendering a full eval requires
//! the eval harness, which lives above the MCP layer, so this module holds
//! the lightweight, dependency-free task-result ledger the tools can operate
//! on directly: accept a task result, compute pass/fail aggregates, and
//! summarize an eval report from a batch of results.

use serde::{Deserialize, Serialize};

/// Result of a single engineering task (acceptance tests for a feature).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskResult {
    /// Task ID from the engineering plane.
    pub task_id: String,
    /// Agent that submitted the result.
    pub agent_id: String,
    /// Number of acceptance tests that passed.
    pub passed: u32,
    /// Number of acceptance tests that failed.
    pub failed: u32,
    /// Score delta observed after the change.
    pub score_delta: f32,
    /// Evidence URI backing this result.
    pub evidence_uri: Option<String>,
    /// Timestamp.
    pub timestamp: String,
}

impl TaskResult {
    /// Whether all acceptance tests passed.
    ///
    /// Requires at least one test to have run: a task with `passed == 0`
    /// (including `failed == 0`) counts as not-all-passed, so incomplete
    /// work is surfaced rather than silently treated as green.
    #[must_use]
    pub const fn all_passed(&self) -> bool {
        self.failed == 0 && self.passed > 0
    }

    /// Total number of tests run.
    #[must_use]
    pub const fn total(&self) -> u32 {
        self.passed + self.failed
    }
}

/// Board of task results for a project.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct TaskResultBoard {
    /// All task results.
    pub results: Vec<TaskResult>,
}

impl TaskResultBoard {
    /// Create a new empty board.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Record a task result.
    pub fn record(&mut self, result: TaskResult) {
        self.results.push(result);
    }

    /// Latest result for a task.
    #[must_use]
    pub fn latest_for_task(&self, task_id: &str) -> Option<&TaskResult> {
        self.results.iter().rev().find(|r| r.task_id == task_id)
    }

    /// Pass rate across all recorded results (0.0–1.0).
    #[must_use]
    pub fn pass_rate(&self) -> f32 {
        if self.results.is_empty() {
            return 0.0;
        }
        let total: u32 = self.results.iter().map(TaskResult::total).sum();
        if total == 0 {
            return 0.0;
        }
        let passed: u32 = self.results.iter().map(|r| r.passed).sum();
        #[expect(clippy::cast_precision_loss, reason = "Test counts fit in f32")]
        let rate = passed as f32 / total as f32;
        rate
    }

    /// Number of tasks with at least one failing test.
    #[must_use]
    pub fn regressions(&self) -> u32 {
        #[expect(
            clippy::cast_possible_truncation,
            reason = "Result counts fit in u32 for realistic boards"
        )]
        let count = self.results.iter().filter(|r| !r.all_passed()).count() as u32;
        count
    }

    /// Number of recorded results.
    #[must_use]
    pub const fn len(&self) -> usize {
        self.results.len()
    }

    /// Whether the board is empty.
    #[must_use]
    pub const fn is_empty(&self) -> bool {
        self.results.is_empty()
    }
}

/// An eval report summary computed from a batch of test outcomes.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EvalSummary {
    /// Total tests run.
    pub total_tests: u32,
    /// Tests that passed.
    pub passed_tests: u32,
    /// Pass rate (0.0–1.0).
    pub pass_rate: f32,
    /// Number of regressed (failing) tasks.
    pub regressions: u32,
    /// Whether the batch is clean (no regressions).
    pub clean: bool,
}

impl EvalSummary {
    /// Compute a summary from a batch of task results.
    #[must_use]
    pub fn from_results(results: &[TaskResult]) -> Self {
        let total: u32 = results.iter().map(TaskResult::total).sum();
        let passed: u32 = results.iter().map(|r| r.passed).sum();
        #[expect(clippy::cast_precision_loss, reason = "Test counts fit in f32")]
        let pass_rate = if total == 0 {
            0.0
        } else {
            passed as f32 / total as f32
        };
        #[expect(
            clippy::cast_possible_truncation,
            reason = "Result counts fit in u32 for realistic batches"
        )]
        let regressions = results.iter().filter(|r| !r.all_passed()).count() as u32;
        Self {
            total_tests: total,
            passed_tests: passed,
            pass_rate,
            regressions,
            clean: regressions == 0 && total > 0,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn result(task: &str, passed: u32, failed: u32) -> TaskResult {
        TaskResult {
            task_id: task.to_string(),
            agent_id: "agent:composer".to_string(),
            passed,
            failed,
            score_delta: 0.0,
            evidence_uri: None,
            timestamp: chrono::Utc::now().to_rfc3339(),
        }
    }

    #[test]
    fn pass_rate_reflects_all_tests() {
        let mut board = TaskResultBoard::new();
        board.record(result("t1", 3, 1));
        board.record(result("t2", 2, 0));
        let rate = board.pass_rate();
        assert!((rate - 5.0 / 6.0).abs() < 1e-6, "got {rate}");
        assert_eq!(board.regressions(), 1);
    }

    #[test]
    fn all_passed_requires_at_least_one_test_and_no_failures() {
        assert!(result("t1", 2, 0).all_passed());
        assert!(!result("t1", 0, 0).all_passed(), "no tests run");
        assert!(!result("t1", 2, 1).all_passed());
    }

    #[test]
    fn eval_summary_is_clean_only_when_no_regressions() {
        let clean = EvalSummary::from_results(&[result("t1", 2, 0), result("t2", 1, 0)]);
        assert!(clean.clean);
        assert!((clean.pass_rate - 1.0).abs() < 1e-6);
        let dirty = EvalSummary::from_results(&[result("t1", 2, 0), result("t2", 0, 1)]);
        assert!(!dirty.clean);
        assert_eq!(dirty.regressions, 1);
    }
}
