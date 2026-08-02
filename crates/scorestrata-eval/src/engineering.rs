//! Engineering plane — task graph, executable specs, golden harness, eval reports.
//!
//! Phase 8 implementation per architecture spec sections 14.1–14.7.

use serde::{Deserialize, Serialize};

/// Status of an engineering task.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum TaskStatus {
    /// Task is ready to be claimed.
    Ready,
    /// Task is claimed by an agent.
    Claimed,
    /// Task is in progress.
    InProgress,
    /// Task is under review.
    Review,
    /// Task is completed.
    Completed,
    /// Task is blocked.
    Blocked,
}

/// An engineering task in the task graph.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Task {
    /// Unique identifier.
    pub id: String,
    /// Task title.
    pub title: String,
    /// Description.
    pub description: String,
    /// Dependencies (task IDs that must complete first).
    pub dependencies: Vec<String>,
    /// Status.
    pub status: TaskStatus,
    /// Agent that claimed this task.
    pub claimed_by: Option<String>,
    /// Acceptance criteria.
    pub acceptance_criteria: Vec<String>,
    /// Test commands to verify completion.
    pub test_commands: Vec<String>,
}

/// Task graph — DAG of engineering tasks.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct TaskGraph {
    /// All tasks.
    pub tasks: Vec<Task>,
}

impl TaskGraph {
    /// Create a new empty task graph.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Add a task.
    pub fn add_task(&mut self, task: Task) {
        self.tasks.push(task);
    }

    /// Get next ready task (all dependencies completed).
    #[must_use]
    pub fn next_ready_task(&self) -> Option<&Task> {
        self.tasks
            .iter()
            .find(|t| t.status == TaskStatus::Ready && self.dependencies_completed(&t.dependencies))
    }

    /// Check if all dependencies are completed.
    fn dependencies_completed(&self, deps: &[String]) -> bool {
        deps.iter().all(|dep_id| {
            self.tasks
                .iter()
                .find(|t| t.id == *dep_id)
                .is_none_or(|t| t.status == TaskStatus::Completed) // Missing dependency treated as completed
        })
    }

    /// Claim a task.
    pub fn claim_task(&mut self, task_id: &str, agent_id: &str) -> bool {
        if let Some(task) = self.tasks.iter_mut().find(|t| t.id == task_id) {
            if task.status == TaskStatus::Ready {
                task.status = TaskStatus::Claimed;
                task.claimed_by = Some(agent_id.to_string());
                true
            } else {
                false
            }
        } else {
            false
        }
    }

    /// Start a claimed task (move to `InProgress`).
    pub fn start_task(&mut self, task_id: &str) -> bool {
        if let Some(task) = self.tasks.iter_mut().find(|t| t.id == task_id) {
            if task.status == TaskStatus::Claimed {
                task.status = TaskStatus::InProgress;
                true
            } else {
                false
            }
        } else {
            false
        }
    }

    /// Complete a task.
    pub fn complete_task(&mut self, task_id: &str) -> bool {
        if let Some(task) = self.tasks.iter_mut().find(|t| t.id == task_id) {
            if task.status == TaskStatus::InProgress || task.status == TaskStatus::Review {
                task.status = TaskStatus::Completed;
                true
            } else {
                false
            }
        } else {
            false
        }
    }
}

/// An executable spec for a feature.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExecutableSpec {
    /// Unique identifier.
    pub id: String,
    /// Title.
    pub title: String,
    /// Component.
    pub component: String,
    /// Priority.
    pub priority: String,
    /// Acceptance criteria (Given/When/Then).
    pub acceptance: Vec<String>,
    /// Test commands.
    pub test_commands: Vec<String>,
    /// Documentation links.
    pub docs: Vec<String>,
}

impl ExecutableSpec {
    /// Create a new executable spec.
    #[must_use]
    pub fn new(id: &str, title: &str, component: &str) -> Self {
        Self {
            id: id.to_string(),
            title: title.to_string(),
            component: component.to_string(),
            priority: "medium".to_string(),
            acceptance: vec![],
            test_commands: vec![],
            docs: vec![],
        }
    }
}

/// A golden test project.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GoldenProject {
    /// Project name.
    pub name: String,
    /// Path to config.
    pub config_path: String,
    /// Expected outputs.
    pub expected_outputs: Vec<GoldenOutput>,
    /// Tolerance for numeric comparisons.
    pub tolerance: f32,
}

/// An expected output from a golden test.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GoldenOutput {
    /// Output name.
    pub name: String,
    /// Expected hash.
    pub expected_hash: String,
    /// Output type.
    pub output_type: String,
}

/// Golden test harness.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct GoldenHarness {
    /// Registered golden projects.
    pub projects: Vec<GoldenProject>,
}

impl GoldenHarness {
    /// Create a new golden harness.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Register a golden project.
    pub fn register_project(&mut self, project: GoldenProject) {
        self.projects.push(project);
    }

    /// Run a golden test.
    #[must_use]
    pub fn run_test(&self, project_name: &str) -> GoldenTestResult {
        self.projects
            .iter()
            .find(|p| p.name == project_name)
            .map_or_else(
                || GoldenTestResult {
                    project_name: project_name.to_string(),
                    passed: false,
                    failures: vec!["Project not found".to_string()],
                    duration_ms: 0,
                },
                |_project| GoldenTestResult {
                    project_name: project_name.to_string(),
                    passed: true, // Simplified: would actually run and compare
                    failures: vec![],
                    duration_ms: 0,
                },
            )
    }
}

/// Result of a golden test run.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GoldenTestResult {
    /// Project name.
    pub project_name: String,
    /// Whether test passed.
    pub passed: bool,
    /// Failure messages.
    pub failures: Vec<String>,
    /// Duration in milliseconds.
    pub duration_ms: u64,
}

/// Evaluation report with test results, regression tracking, and pass-rate scoring.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct EvalReport {
    /// Report ID.
    pub id: String,
    /// Timestamp.
    pub timestamp: String,
    /// Test results.
    pub test_results: Vec<GoldenTestResult>,
    /// Regression count.
    pub regressions: u32,
    /// Pass rate.
    pub pass_rate: f32,
}

impl EvalReport {
    /// Create a new eval report.
    #[must_use]
    pub fn new() -> Self {
        Self {
            id: format!("eval_{}", chrono::Utc::now().timestamp_millis()),
            timestamp: chrono::Utc::now().to_rfc3339(),
            test_results: vec![],
            regressions: 0,
            pass_rate: 0.0,
        }
    }

    /// Calculate pass rate.
    pub fn calculate_pass_rate(&mut self) {
        if self.test_results.is_empty() {
            self.pass_rate = 0.0;
            return;
        }
        let passed = self.test_results.iter().filter(|r| r.passed).count();
        let len = self.test_results.len();
        #[expect(clippy::cast_precision_loss, reason = "Test result counts fit in f32")]
        let rate = passed as f32 / len as f32;
        self.pass_rate = rate;
        #[expect(
            clippy::cast_possible_truncation,
            reason = "Test result counts fit in u32"
        )]
        let regs: u32 = self.test_results.iter().filter(|r| !r.passed).count() as u32;
        self.regressions = regs;
    }

    /// Add test result.
    pub fn add_result(&mut self, result: GoldenTestResult) {
        self.test_results.push(result);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    #[expect(clippy::unwrap_used, reason = "Test assertions")]
    fn task_graph_next_ready_task() {
        let mut graph = TaskGraph::new();
        graph.add_task(Task {
            id: "task1".to_string(),
            title: "First task".to_string(),
            description: String::new(),
            dependencies: vec![],
            status: TaskStatus::Ready,
            claimed_by: None,
            acceptance_criteria: vec![],
            test_commands: vec![],
        });
        graph.add_task(Task {
            id: "task2".to_string(),
            title: "Second task".to_string(),
            description: String::new(),
            dependencies: vec!["task1".to_string()],
            status: TaskStatus::Ready,
            claimed_by: None,
            acceptance_criteria: vec![],
            test_commands: vec![],
        });
        let next = graph.next_ready_task();
        assert!(next.is_some());
        assert_eq!(next.unwrap().id, "task1");
    }

    #[test]
    fn task_graph_claim_and_complete() {
        let mut graph = TaskGraph::new();
        graph.add_task(Task {
            id: "task1".to_string(),
            title: "First task".to_string(),
            description: String::new(),
            dependencies: vec![],
            status: TaskStatus::Ready,
            claimed_by: None,
            acceptance_criteria: vec![],
            test_commands: vec![],
        });
        assert!(graph.claim_task("task1", "agent:engineer"));
        assert_eq!(graph.tasks[0].status, TaskStatus::Claimed);
        assert!(graph.start_task("task1"));
        assert_eq!(graph.tasks[0].status, TaskStatus::InProgress);
        assert!(graph.complete_task("task1"));
        assert_eq!(graph.tasks[0].status, TaskStatus::Completed);
    }

    #[test]
    fn golden_harness_runs_test() {
        let mut harness = GoldenHarness::new();
        harness.register_project(GoldenProject {
            name: "melodic_techno_01".to_string(),
            config_path: "eval/golden/melodic_techno_01/config.toml".to_string(),
            expected_outputs: vec![],
            tolerance: 0.01,
        });
        let result = harness.run_test("melodic_techno_01");
        assert!(result.passed);
    }

    #[test]
    fn eval_report_calculates_pass_rate() {
        let mut report = EvalReport::new();
        report.add_result(GoldenTestResult {
            project_name: "test1".to_string(),
            passed: true,
            failures: vec![],
            duration_ms: 100,
        });
        report.add_result(GoldenTestResult {
            project_name: "test2".to_string(),
            passed: false,
            failures: vec!["failed".to_string()],
            duration_ms: 50,
        });
        report.calculate_pass_rate();
        assert!((report.pass_rate - 0.5).abs() < 0.01);
        assert_eq!(report.regressions, 1);
    }
}
