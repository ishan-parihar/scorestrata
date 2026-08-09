//! Multi-agent task board — lease-based claiming, blackboard model.
//!
//! Implements the coordination model from architecture spec §4.4:
//! - Task board with ready/claimed/blocked/review/completed states
//! - Blackboard with current diagnosis, active candidates, top issues
//! - Lease-based claiming to prevent conflicting mutations
//! - Separate critic from mutator roles

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

/// Status of a task on the board.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum TaskBoardStatus {
    /// Task is ready to be claimed.
    Ready,
    /// Task is claimed by an agent.
    Claimed,
    /// Task is blocked by a dependency.
    Blocked,
    /// Task is under review.
    Review,
    /// Task is completed.
    Completed,
    /// Task was abandoned.
    Abandoned,
}

/// A task on the task board.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BoardTask {
    /// Unique task ID.
    pub id: String,
    /// Task title.
    pub title: String,
    /// Description.
    pub description: String,
    /// Task type (e.g. `composition`, `validation`, `engineering`).
    pub task_type: String,
    /// Status.
    pub status: TaskBoardStatus,
    /// Agent that claimed this task.
    pub claimed_by: Option<String>,
    /// Lease expiry timestamp.
    ///
    /// Why: Stored as `DateTime<Utc>` for type safety and zero parsing overhead.
    /// Previously was `Option<String>` which required RFC3339 parsing on every
    /// `is_lease_expired_at()` call.
    pub lease_expires_at: Option<chrono::DateTime<chrono::Utc>>,
    /// Scope of the task (`segment_id`, `layer_id`, etc.).
    pub scope: BTreeMap<String, String>,
    /// Allowed tools for this task.
    pub allowed_tools: Vec<String>,
    /// Allowed mutations (e.g. pattern, note, mix).
    pub allowed_mutations: Vec<String>,
    /// Dependencies (task IDs that must complete first).
    pub dependencies: Vec<String>,
    /// Priority (higher = more urgent).
    pub priority: u32,
    /// Created timestamp.
    pub created_at: String,
}

impl BoardTask {
    /// Create a new task.
    #[must_use]
    pub fn new(id: &str, title: &str, task_type: &str) -> Self {
        Self {
            id: id.to_string(),
            title: title.to_string(),
            description: String::new(),
            task_type: task_type.to_string(),
            status: TaskBoardStatus::Ready,
            claimed_by: None,
            lease_expires_at: None,
            scope: BTreeMap::new(),
            allowed_tools: vec![],
            allowed_mutations: vec![],
            dependencies: vec![],
            priority: 0,
            created_at: chrono::Utc::now().to_rfc3339(),
        }
    }

    /// Check if the lease has expired at the given time.
    #[must_use]
    pub fn is_lease_expired_at(&self, now: chrono::DateTime<chrono::Utc>) -> bool {
        self.lease_expires_at.is_some_and(|expiry| now > expiry)
    }

    /// Check if the lease has expired using the current time.
    #[must_use]
    pub fn is_lease_expired(&self) -> bool {
        self.is_lease_expired_at(chrono::Utc::now())
    }
}

/// Task board — manages task claiming and status transitions.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct TaskBoard {
    /// All tasks, keyed by task ID.
    pub tasks: BTreeMap<String, BoardTask>,
    /// Agent roles and their capabilities.
    pub agent_roles: BTreeMap<String, AgentRole>,
}

/// An agent role with its capabilities and restrictions.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct AgentRole {
    /// Role name (e.g. `composer`, `critic`, `mix_engineer`).
    pub name: String,
    /// Can this role mutate project state?
    pub can_mutate: bool,
    /// Can this role finalize (approve, archive)?
    pub can_finalize: bool,
    /// Can this role escalate to human?
    pub can_escalate: bool,
    /// Allowed tool categories.
    pub allowed_tool_categories: Vec<String>,
}

impl TaskBoard {
    /// Create a new empty task board.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Add a task to the board.
    pub fn add_task(&mut self, task: BoardTask) {
        self.tasks.insert(task.id.clone(), task);
    }

    /// Claim a task with a lease.
    ///
    /// Returns `true` if the claim was successful, `false` if the task
    /// is not ready or the agent doesn't have the required role.
    pub fn claim_task(&mut self, task_id: &str, agent_id: &str, lease_duration_secs: u64) -> bool {
        // First check status and dependencies via immutable borrow
        let can_claim = self.tasks.get(task_id).is_some_and(|task| {
            task.status == TaskBoardStatus::Ready && self.dependencies_met(task)
        });
        if !can_claim {
            return false;
        }
        // Now take mutable borrow to update
        if let Some(task) = self.tasks.get_mut(task_id) {
            task.status = TaskBoardStatus::Claimed;
            task.claimed_by = Some(agent_id.to_string());
            task.lease_expires_at = Some(
                chrono::Utc::now()
                    + chrono::Duration::seconds(
                        i64::try_from(lease_duration_secs).unwrap_or(i64::MAX),
                    ),
            );
            true
        } else {
            false
        }
    }

    /// Check if all dependencies for a task are satisfied.
    fn dependencies_met(&self, task: &BoardTask) -> bool {
        task.dependencies.iter().all(|dep_id| {
            self.tasks
                .get(dep_id)
                .is_none_or(|t| t.status == TaskBoardStatus::Completed)
        })
    }

    /// Start working on a claimed task.
    pub fn start_task(&mut self, task_id: &str) -> bool {
        if let Some(task) = self.tasks.get_mut(task_id) {
            if task.status == TaskBoardStatus::Claimed {
                task.status = TaskBoardStatus::Review;
                true
            } else {
                false
            }
        } else {
            false
        }
    }

    /// Complete a task and clear its lease.
    pub fn complete_task(&mut self, task_id: &str) -> bool {
        if let Some(task) = self.tasks.get_mut(task_id) {
            if matches!(
                task.status,
                TaskBoardStatus::Claimed | TaskBoardStatus::Review
            ) {
                task.status = TaskBoardStatus::Completed;
                task.claimed_by = None;
                task.lease_expires_at = None;
                true
            } else {
                false
            }
        } else {
            false
        }
    }

    /// Abandon a task.
    pub fn abandon_task(&mut self, task_id: &str) -> bool {
        if let Some(task) = self.tasks.get_mut(task_id) {
            task.status = TaskBoardStatus::Abandoned;
            task.claimed_by = None;
            task.lease_expires_at = None;
            true
        } else {
            false
        }
    }

    /// Release a task (unclaim it).
    pub fn release_task(&mut self, task_id: &str) -> bool {
        if let Some(task) = self.tasks.get_mut(task_id) {
            if task.status == TaskBoardStatus::Claimed {
                task.status = TaskBoardStatus::Ready;
                task.claimed_by = None;
                task.lease_expires_at = None;
                true
            } else {
                false
            }
        } else {
            false
        }
    }

    /// Get all ready tasks, sorted by priority descending.
    #[must_use]
    pub fn ready_tasks(&self) -> Vec<&BoardTask> {
        let mut tasks: Vec<&BoardTask> = self
            .tasks
            .values()
            .filter(|t| t.status == TaskBoardStatus::Ready)
            .collect();
        tasks.sort_by_key(|t| std::cmp::Reverse(t.priority));
        tasks
    }

    /// Get the next ready task whose dependencies are met.
    #[must_use]
    pub fn next_ready_task(&self) -> Option<&BoardTask> {
        self.ready_tasks().into_iter().find(|task| {
            task.dependencies.iter().all(|dep_id| {
                self.tasks
                    .get(dep_id)
                    .is_none_or(|t| t.status == TaskBoardStatus::Completed)
            })
        })
    }

    /// Get tasks claimed by a specific agent.
    #[must_use]
    pub fn tasks_claimed_by(&self, agent_id: &str) -> Vec<&BoardTask> {
        self.tasks
            .values()
            .filter(|t| t.claimed_by.as_deref() == Some(agent_id))
            .collect()
    }

    /// Check for expired leases and release those tasks.
    pub fn release_expired_leases(&mut self) -> Vec<String> {
        let mut released = Vec::new();
        for task in self.tasks.values_mut() {
            if task.status == TaskBoardStatus::Claimed && task.is_lease_expired() {
                task.status = TaskBoardStatus::Ready;
                task.claimed_by = None;
                task.lease_expires_at = None;
                released.push(task.id.clone());
            }
        }
        released
    }

    /// Get total task count.
    #[must_use]
    pub fn len(&self) -> usize {
        self.tasks.len()
    }

    /// Whether the board is empty.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.tasks.is_empty()
    }
}

/// Blackboard — per-project coordination state visible to all agents.
///
/// The blackboard is a per-project coordination state where agents post
/// diagnoses, candidates, issues, and escalation flags. It is owned by
/// the project and accessed sequentially by agents, not a concurrent
/// shared-memory model.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Blackboard {
    /// Current top diagnosis (highest severity).
    pub current_diagnosis: Option<String>,
    /// Active candidate IDs being evaluated.
    pub active_candidates: Vec<String>,
    /// Top issues (sorted by severity).
    pub top_issues: Vec<String>,
    /// Lesson hints from past experience.
    pub lesson_hints: Vec<String>,
    /// Escalation flags.
    pub escalation_flags: Vec<String>,
    /// Budget state summary.
    pub budget_state: Option<BudgetStateSummary>,
    /// Last updated timestamp.
    pub last_updated: String,
}

/// Budget state summary for the blackboard.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BudgetStateSummary {
    /// Iterations remaining.
    pub iterations_remaining: u32,
    /// Tool calls remaining.
    pub tool_calls_remaining: u32,
    /// Whether stagnation is detected.
    pub stagnation_detected: bool,
}

impl Blackboard {
    /// Create a new empty blackboard.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Update the blackboard with current state.
    pub fn update(&mut self, diagnosis: Option<String>, top_issues: Vec<String>) {
        self.current_diagnosis = diagnosis;
        self.top_issues = top_issues;
        self.last_updated = chrono::Utc::now().to_rfc3339();
    }

    /// Add an escalation flag.
    pub fn add_escalation(&mut self, flag: String) {
        self.escalation_flags.push(flag);
        self.last_updated = chrono::Utc::now().to_rfc3339();
    }

    /// Clear escalation flags.
    pub fn clear_escalations(&mut self) {
        self.escalation_flags.clear();
        self.last_updated = chrono::Utc::now().to_rfc3339();
    }

    /// Add a lesson hint.
    pub fn add_lesson_hint(&mut self, hint: String) {
        self.lesson_hints.push(hint);
        self.last_updated = chrono::Utc::now().to_rfc3339();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn task_board_add_and_claim() {
        let mut board = TaskBoard::new();
        let mut task = BoardTask::new("task_1", "Fix scale violation", "composition");
        task.priority = 5;
        board.add_task(task);

        assert!(board.claim_task("task_1", "agent:composer", 300));
        let claimed = board.tasks_claimed_by("agent:composer");
        assert_eq!(claimed.len(), 1);
        assert_eq!(claimed[0].id, "task_1");
    }

    #[test]
    fn task_board_prevents_double_claim() {
        let mut board = TaskBoard::new();
        board.add_task(BoardTask::new("task_1", "Task", "test"));
        assert!(board.claim_task("task_1", "agent:a", 300));
        assert!(!board.claim_task("task_1", "agent:b", 300));
    }

    #[test]
    fn task_board_respects_dependencies() {
        let mut board = TaskBoard::new();
        let mut t1 = BoardTask::new("task_1", "First", "test");
        t1.dependencies = vec![];
        let mut t2 = BoardTask::new("task_2", "Second", "test");
        t2.dependencies = vec!["task_1".to_string()];
        board.add_task(t1);
        board.add_task(t2);

        // task_2 should not be claimable until task_1 is completed
        assert!(!board.claim_task("task_2", "agent:a", 300));
        // task_1 should be claimable (no deps)
        assert!(board.claim_task("task_1", "agent:a", 300));
        assert!(board.complete_task("task_1"));
        // Now task_2 should be claimable
        assert!(board.claim_task("task_2", "agent:b", 300));
        // And next_ready_task should return task_2 (now claimed, so not ready)
        let next = board.next_ready_task();
        assert!(next.is_none());
    }

    #[test]
    fn task_board_releases_expired_leases() {
        let mut board = TaskBoard::new();
        let mut task = BoardTask::new("task_1", "Task", "test");
        task.lease_expires_at = Some(chrono::Utc::now() - chrono::Duration::hours(1));
        task.status = TaskBoardStatus::Claimed;
        task.claimed_by = Some("agent:a".to_string());
        board.add_task(task);

        let released = board.release_expired_leases();
        assert_eq!(released.len(), 1);
        assert_eq!(board.tasks["task_1"].status, TaskBoardStatus::Ready);
    }

    #[test]
    fn blackboard_tracks_issues() {
        let mut bb = Blackboard::new();
        bb.update(
            Some("scale_violation".to_string()),
            vec!["low_density".to_string(), "weak_transition".to_string()],
        );
        assert_eq!(bb.current_diagnosis, Some("scale_violation".to_string()));
        assert_eq!(bb.top_issues.len(), 2);
    }

    #[test]
    fn blackboard_escalation_flags() {
        let mut bb = Blackboard::new();
        bb.add_escalation("stagnation_detected".to_string());
        assert_eq!(bb.escalation_flags.len(), 1);
        bb.clear_escalations();
        assert!(bb.escalation_flags.is_empty());
    }
}
