//! Memory tiers — structured memory for agent learning.
//!
//! Phase 7 implementation per architecture spec section 8.13.
//!
//! Memory is organized into four tiers:
//! - Working memory: current project state, active candidates, budget
//! - Episodic memory: command history, experiment history, failure history
//! - Semantic memory: lessons, genre profiles, patch templates, metric explanations
//! - Procedural memory: playbooks, repair strategies, composition routines

use std::collections::BTreeMap;
use std::path::Path;

use serde::{Deserialize, Serialize};

use scorestrata_core::error::ScorestrataError;

use crate::StorageResult;

/// Working memory — ephemeral state for the current agent session.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct WorkingMemory {
    /// Current project summary (brief description).
    pub current_summary: String,
    /// Current top diagnosis.
    pub current_diagnosis: Option<String>,
    /// Active candidate IDs.
    pub active_candidates: Vec<String>,
    /// Current budget state snapshot.
    pub budget_snapshot: Option<BudgetSnapshot>,
    /// Current phase in the workflow.
    pub workflow_phase: String,
    /// Focus URIs for drill-down inspection.
    pub focus_uris: Vec<String>,
}

/// Budget snapshot for working memory.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BudgetSnapshot {
    /// Iterations remaining.
    pub iterations_remaining: u32,
    /// Tool calls remaining.
    pub tool_calls_remaining: u32,
    /// Render seconds remaining.
    pub render_seconds_remaining: f32,
    /// No-improvement count.
    pub no_improvement_count: u32,
}

/// Episodic memory — records of past events and outcomes.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct EpisodicMemory {
    /// Command history entries.
    pub command_history: Vec<CommandRecord>,
    /// Experiment history.
    pub experiment_history: Vec<ExperimentRecord>,
    /// Failure history.
    pub failure_history: Vec<FailureRecord>,
    /// Rollback history.
    pub rollback_history: Vec<RollbackRecord>,
}

/// A recorded command execution.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CommandRecord {
    /// Command ID.
    pub command_id: String,
    /// Command type.
    pub command_type: String,
    /// Actor that executed the command.
    pub actor: String,
    /// Whether it succeeded.
    pub success: bool,
    /// Score delta (if applicable).
    pub score_delta: Option<f32>,
    /// Duration in milliseconds.
    pub duration_ms: u64,
    /// Timestamp.
    pub timestamp: String,
}

/// A recorded experiment outcome.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExperimentRecord {
    /// Experiment ID.
    pub experiment_id: String,
    /// Hypothesis tested.
    pub hypothesis: String,
    /// Number of candidates.
    pub candidate_count: u32,
    /// Winner candidate ID.
    pub winner_id: Option<String>,
    /// Score delta from winner.
    pub score_delta: Option<f32>,
    /// Whether a lesson was derived.
    pub lesson_derived: bool,
    /// Timestamp.
    pub timestamp: String,
}

/// A recorded failure.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FailureRecord {
    /// Failure type/code.
    pub failure_type: String,
    /// Scope where failure occurred.
    pub scope: String,
    /// Diagnostic code.
    pub diagnostic_code: String,
    /// Patch that was attempted.
    pub attempted_patch: Option<String>,
    /// Whether the patch succeeded on retry.
    pub retry_succeeded: bool,
    /// Timestamp.
    pub timestamp: String,
}

/// A recorded rollback.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RollbackRecord {
    /// Source snapshot ID.
    pub from_snapshot_id: String,
    /// Target snapshot ID (after rollback).
    pub to_snapshot_id: String,
    /// Reason for rollback.
    pub reason: String,
    /// Timestamp.
    pub timestamp: String,
}

/// Semantic memory — persistent knowledge and patterns.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct SemanticMemory {
    /// Genre profiles (genre name → profile data).
    pub genre_profiles: BTreeMap<String, serde_json::Value>,
    /// Failure patterns (pattern code → frequency and last seen).
    pub failure_patterns: BTreeMap<String, FailurePattern>,
    /// Metric explanations (metric name → explanation text).
    pub metric_explanations: BTreeMap<String, String>,
    /// Patch template rankings.
    pub patch_rankings: Vec<PatchRanking>,
}

/// A tracked failure pattern.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FailurePattern {
    /// Pattern code.
    pub code: String,
    /// How many times observed.
    pub count: u32,
    /// Last seen timestamp.
    pub last_seen: String,
    /// Best known fix.
    pub best_fix: Option<String>,
}

/// A patch template ranking entry.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PatchRanking {
    /// Patch operation name.
    pub operation: String,
    /// Success rate (0.0–1.0).
    pub success_rate: f32,
    /// Number of times used.
    pub usage_count: u32,
    /// Applicable genres.
    pub genres: Vec<String>,
}

/// Procedural memory — playbooks and strategies.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ProceduralMemory {
    /// Repair playbooks (diagnostic → ordered list of steps).
    pub repair_playbooks: BTreeMap<String, Vec<String>>,
    /// Composition routines (genre → composition strategy).
    pub composition_routines: BTreeMap<String, Vec<String>>,
    /// Mixing routines (genre → mixing strategy).
    pub mixing_routines: BTreeMap<String, Vec<String>>,
    /// Engineering workflows (task type → workflow steps).
    pub engineering_workflows: BTreeMap<String, Vec<String>>,
}

/// Complete project memory — combines all four tiers.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ProjectMemory {
    /// Working memory (current session state).
    pub working: WorkingMemory,
    /// Episodic memory (past events).
    pub episodic: EpisodicMemory,
    /// Semantic memory (persistent knowledge).
    pub semantic: SemanticMemory,
    /// Procedural memory (strategies and playbooks).
    pub procedural: ProceduralMemory,
}

impl ProjectMemory {
    /// Create a new empty project memory.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Record a command execution in episodic memory.
    pub fn record_command(&mut self, record: CommandRecord) {
        self.episodic.command_history.push(record);
    }

    /// Record a failure in episodic memory and update semantic failure patterns.
    pub fn record_failure(&mut self, record: FailureRecord) {
        let diagnostic_code = record.diagnostic_code.clone();
        let timestamp = record.timestamp.clone();
        let pattern = self
            .semantic
            .failure_patterns
            .entry(diagnostic_code)
            .or_insert_with(|| FailurePattern {
                code: record.diagnostic_code.clone(),
                count: 0,
                last_seen: record.timestamp.clone(),
                best_fix: None,
            });
        pattern.count += 1;
        pattern.last_seen.clone_from(&timestamp);
        if record.retry_succeeded
            && let Some(patch) = &record.attempted_patch
        {
            pattern.best_fix = Some(patch.clone());
        }
        self.episodic.failure_history.push(record);
    }

    /// Record an experiment outcome in episodic memory.
    pub fn record_experiment(&mut self, record: ExperimentRecord) {
        self.episodic.experiment_history.push(record);
    }

    /// Record a rollback in episodic memory.
    pub fn record_rollback(&mut self, record: RollbackRecord) {
        self.episodic.rollback_history.push(record);
    }

    /// Get the top N most frequent failure patterns.
    #[must_use]
    pub fn top_failure_patterns(&self, n: usize) -> Vec<&FailurePattern> {
        let mut patterns: Vec<&FailurePattern> = self.semantic.failure_patterns.values().collect();
        patterns.sort_by_key(|p| std::cmp::Reverse(p.count));
        patterns.into_iter().take(n).collect()
    }

    /// Get the total command count.
    #[must_use]
    pub const fn command_count(&self) -> usize {
        self.episodic.command_history.len()
    }

    /// Get the total failure count.
    #[must_use]
    pub const fn failure_count(&self) -> usize {
        self.episodic.failure_history.len()
    }
}

/// Save project memory to disk.
///
/// # Errors
///
/// Returns [`ScorestrataError::Io`] if the file cannot be written.
pub fn save_project_memory(path: &Path, memory: &ProjectMemory) -> StorageResult<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| ScorestrataError::Io {
            path: parent.display().to_string(),
            source: e,
        })?;
    }
    let data =
        serde_json::to_string_pretty(memory).map_err(|e| ScorestrataError::Serialization {
            message: format!("Failed to serialize project memory: {e}"),
        })?;
    std::fs::write(path, data).map_err(|e| ScorestrataError::Io {
        path: path.display().to_string(),
        source: e,
    })?;
    Ok(())
}

/// Load project memory from disk.
///
/// Returns empty memory if the file does not exist.
///
/// # Errors
///
/// Returns [`ScorestrataError::Io`] or [`ScorestrataError::Serialization`]
/// if the file cannot be read or parsed.
pub fn load_project_memory(path: &Path) -> StorageResult<ProjectMemory> {
    if !path.exists() {
        return Ok(ProjectMemory::new());
    }
    let content = std::fs::read_to_string(path).map_err(|e| ScorestrataError::Io {
        path: path.display().to_string(),
        source: e,
    })?;
    let memory: ProjectMemory =
        serde_json::from_str(&content).map_err(|e| ScorestrataError::Serialization {
            message: format!("Failed to parse project memory: {e}"),
        })?;
    Ok(memory)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn memory_records_command_history() {
        let mut mem = ProjectMemory::new();
        mem.record_command(CommandRecord {
            command_id: "cmd_1".to_string(),
            command_type: "apply_patch".to_string(),
            actor: "agent:composer".to_string(),
            success: true,
            score_delta: Some(0.05),
            duration_ms: 42,
            timestamp: chrono::Utc::now().to_rfc3339(),
        });
        assert_eq!(mem.command_count(), 1);
    }

    #[test]
    fn memory_tracks_failure_patterns() {
        let mut mem = ProjectMemory::new();
        mem.record_failure(FailureRecord {
            failure_type: "scale_violation".to_string(),
            scope: "harmony".to_string(),
            diagnostic_code: "SCALE_VIOLATION".to_string(),
            attempted_patch: Some("quantize_to_scale".to_string()),
            retry_succeeded: true,
            timestamp: chrono::Utc::now().to_rfc3339(),
        });
        mem.record_failure(FailureRecord {
            failure_type: "scale_violation".to_string(),
            scope: "harmony".to_string(),
            diagnostic_code: "SCALE_VIOLATION".to_string(),
            attempted_patch: Some("quantize_to_scale".to_string()),
            retry_succeeded: true,
            timestamp: chrono::Utc::now().to_rfc3339(),
        });

        assert_eq!(mem.failure_count(), 2);
        let patterns = mem.top_failure_patterns(5);
        assert_eq!(patterns.len(), 1);
        assert_eq!(patterns[0].count, 2);
        assert_eq!(patterns[0].best_fix.as_deref(), Some("quantize_to_scale"));
    }

    #[test]
    fn memory_records_rollback() {
        let mut mem = ProjectMemory::new();
        mem.record_rollback(RollbackRecord {
            from_snapshot_id: "snap_2".to_string(),
            to_snapshot_id: "snap_1".to_string(),
            reason: "Metric regression detected".to_string(),
            timestamp: chrono::Utc::now().to_rfc3339(),
        });
        assert_eq!(mem.episodic.rollback_history.len(), 1);
    }

    #[test]
    #[expect(clippy::unwrap_used, reason = "Test should panic on unexpected error")]
    fn project_memory_roundtrip() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("memory.json");

        let mut mem = ProjectMemory::new();
        mem.working.current_summary = "Test project".to_string();
        mem.record_command(CommandRecord {
            command_id: "cmd_1".to_string(),
            command_type: "test".to_string(),
            actor: "test".to_string(),
            success: true,
            score_delta: None,
            duration_ms: 0,
            timestamp: chrono::Utc::now().to_rfc3339(),
        });

        save_project_memory(&path, &mem).unwrap();
        let loaded = load_project_memory(&path).unwrap();
        assert_eq!(loaded.working.current_summary, "Test project");
        assert_eq!(loaded.command_count(), 1);
    }

    #[test]
    #[expect(clippy::unwrap_used, reason = "Test should panic on unexpected error")]
    fn load_nonexistent_returns_empty() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("nonexistent.json");
        let mem = load_project_memory(&path).unwrap();
        assert_eq!(mem.command_count(), 0);
    }
}
