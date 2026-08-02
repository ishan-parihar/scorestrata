//! Experiment runner — manages multi-candidate experiments.
//!
//! Phase 7 implementation per architecture spec sections 8.11 and 13.7.
//!
//! An experiment groups competing candidates under a single hypothesis,
//! evaluates them, selects a winner, and records the outcome as a lesson.

use std::collections::BTreeMap;
use std::path::Path;

use serde::{Deserialize, Serialize};

use scorestrata_core::diagnostic::Scorecard;
use scorestrata_core::error::ScorestrataError;

use crate::StorageResult;

/// Status of an experiment.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ExperimentStatus {
    /// Experiment is being set up.
    Drafting,
    /// Candidates are being evaluated.
    Running,
    /// A winner has been selected.
    Completed,
    /// Experiment was aborted (budget, escalation, etc.).
    Aborted,
}

/// An experiment — groups competing candidates under a hypothesis.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Experiment {
    /// Unique identifier.
    pub id: String,
    /// Hypothesis being tested (e.g. "Adding a riser improves transition energy").
    pub hypothesis: String,
    /// Project this experiment belongs to.
    pub project_id: String,
    /// IDs of candidates in this experiment.
    pub candidate_ids: Vec<String>,
    /// ID of the winning candidate (if any).
    pub winner_id: Option<String>,
    /// ID of the lesson derived from this experiment (if any).
    pub lesson_id: Option<String>,
    /// Current status.
    pub status: ExperimentStatus,
    /// Scope of the experiment (e.g. `transition`, `chorus_density`).
    pub scope: String,
    /// Genre context.
    pub genre: Option<String>,
    /// Trigger diagnostic code that prompted this experiment.
    pub trigger: String,
    /// Scorecards for each candidate, keyed by candidate ID.
    pub scorecards: BTreeMap<String, Scorecard>,
    /// Creation timestamp.
    pub created_at: String,
    /// Completion timestamp.
    pub completed_at: Option<String>,
}

impl Experiment {
    /// Create a new experiment with a caller-provided ID.
    #[must_use]
    pub fn new(id: &str, project_id: &str, hypothesis: &str, trigger: &str, scope: &str) -> Self {
        Self {
            id: id.to_string(),
            hypothesis: hypothesis.to_string(),
            project_id: project_id.to_string(),
            candidate_ids: vec![],
            winner_id: None,
            lesson_id: None,
            status: ExperimentStatus::Drafting,
            scope: scope.to_string(),
            genre: None,
            trigger: trigger.to_string(),
            scorecards: BTreeMap::new(),
            created_at: chrono::Utc::now().to_rfc3339(),
            completed_at: None,
        }
    }

    /// Add a candidate to the experiment.
    pub fn add_candidate(&mut self, candidate_id: &str) {
        self.candidate_ids.push(candidate_id.to_string());
        if self.status == ExperimentStatus::Drafting {
            self.status = ExperimentStatus::Running;
        }
    }

    /// Record a scorecard for a candidate.
    pub fn record_scorecard(&mut self, candidate_id: &str, scorecard: Scorecard) {
        self.scorecards.insert(candidate_id.to_string(), scorecard);
    }

    /// Select the winner based on highest overall score.
    ///
    /// Returns the winner's candidate ID.
    #[must_use]
    pub fn select_winner(&self) -> Option<String> {
        self.candidate_ids
            .iter()
            .filter_map(|id| self.scorecards.get(id).map(|sc| (id, sc.overall_score)))
            .max_by(|a, b| a.1.partial_cmp(&b.1).unwrap_or(std::cmp::Ordering::Equal))
            .map(|(id, _)| id.clone())
    }

    /// Complete the experiment with the given winner.
    pub fn complete(&mut self, winner_id: Option<String>) {
        self.winner_id = winner_id;
        self.status = ExperimentStatus::Completed;
        self.completed_at = Some(chrono::Utc::now().to_rfc3339());
    }

    /// Abort the experiment.
    pub fn abort(&mut self) {
        self.status = ExperimentStatus::Aborted;
        self.completed_at = Some(chrono::Utc::now().to_rfc3339());
    }
}

/// Experiment store — manages experiments for a project.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ExperimentStore {
    /// All experiments, keyed by ID.
    experiments: BTreeMap<String, Experiment>,
}

impl ExperimentStore {
    /// Create a new empty experiment store.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Record a new experiment.
    pub fn record(&mut self, experiment: Experiment) {
        let id = experiment.id.clone();
        self.experiments.insert(id, experiment);
    }

    /// Get an experiment by ID.
    #[must_use]
    pub fn get(&self, id: &str) -> Option<&Experiment> {
        self.experiments.get(id)
    }

    /// Get a mutable reference to an experiment.
    pub fn get_mut(&mut self, id: &str) -> Option<&mut Experiment> {
        self.experiments.get_mut(id)
    }

    /// Get all experiments for a project.
    #[must_use]
    pub fn get_by_project(&self, project_id: &str) -> Vec<&Experiment> {
        self.experiments
            .values()
            .filter(|e| e.project_id == project_id)
            .collect()
    }

    /// Get experiments by status.
    #[must_use]
    pub fn get_by_status(&self, status: ExperimentStatus) -> Vec<&Experiment> {
        self.experiments
            .values()
            .filter(|e| e.status == status)
            .collect()
    }

    /// Get the total number of experiments.
    #[must_use]
    pub fn len(&self) -> usize {
        self.experiments.len()
    }

    /// Whether the store is empty.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.experiments.is_empty()
    }
}

/// Save the experiment store to disk.
///
/// # Errors
///
/// Returns [`ScorestrataError::Io`] if the file cannot be written.
pub fn save_experiment_store(path: &Path, store: &ExperimentStore) -> StorageResult<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| ScorestrataError::Io {
            path: parent.display().to_string(),
            source: e,
        })?;
    }
    let data =
        serde_json::to_string_pretty(store).map_err(|e| ScorestrataError::Serialization {
            message: format!("Failed to serialize experiment store: {e}"),
        })?;
    std::fs::write(path, data).map_err(|e| ScorestrataError::Io {
        path: path.display().to_string(),
        source: e,
    })?;
    Ok(())
}

/// Load the experiment store from disk.
///
/// # Errors
///
/// Returns [`ScorestrataError::Io`] or [`ScorestrataError::Serialization`]
/// if the file cannot be read or parsed.
pub fn load_experiment_store(path: &Path) -> StorageResult<ExperimentStore> {
    if !path.exists() {
        return Ok(ExperimentStore::new());
    }
    let content = std::fs::read_to_string(path).map_err(|e| ScorestrataError::Io {
        path: path.display().to_string(),
        source: e,
    })?;
    let store: ExperimentStore =
        serde_json::from_str(&content).map_err(|e| ScorestrataError::Serialization {
            message: format!("Failed to parse experiment store: {e}"),
        })?;
    Ok(store)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn experiment_lifecycle() {
        let mut exp = Experiment::new(
            "exp_test_1",
            "proj_1",
            "Adding riser improves energy",
            "TRANSITION_LOW",
            "transition",
        );
        assert_eq!(exp.status, ExperimentStatus::Drafting);

        exp.add_candidate("cand_1");
        assert_eq!(exp.status, ExperimentStatus::Running);

        exp.add_candidate("cand_2");

        // Record scorecards
        exp.record_scorecard(
            "cand_1",
            Scorecard {
                id: scorestrata_core::ids::ScorecardId::new(),
                metrics: vec![],
                overall_score: 0.7,
                hard_failures: 0,
                warnings: 0,
            },
        );
        exp.record_scorecard(
            "cand_2",
            Scorecard {
                id: scorestrata_core::ids::ScorecardId::new(),
                metrics: vec![],
                overall_score: 0.85,
                hard_failures: 0,
                warnings: 0,
            },
        );

        let winner = exp.select_winner();
        assert_eq!(winner, Some("cand_2".to_string()));

        exp.complete(winner);
        assert_eq!(exp.status, ExperimentStatus::Completed);
        assert_eq!(exp.winner_id, Some("cand_2".to_string()));
        assert!(exp.completed_at.is_some());
    }

    #[test]
    fn experiment_store_records_and_retrieves() {
        let mut store = ExperimentStore::new();
        let exp = Experiment::new(
            "exp_test_2",
            "proj_1",
            "Test hypothesis",
            "SCALE_VIOLATION",
            "harmony",
        );
        let id = exp.id.clone();
        store.record(exp);

        assert_eq!(store.len(), 1);
        assert!(store.get(&id).is_some());
        assert_eq!(store.get_by_project("proj_1").len(), 1);
    }

    #[test]
    fn experiment_abort() {
        let mut exp = Experiment::new("exp_test_3", "proj_1", "Test", "A", "scope");
        exp.add_candidate("c1");
        exp.abort();
        assert_eq!(exp.status, ExperimentStatus::Aborted);
    }

    #[test]
    #[expect(clippy::unwrap_used, reason = "Test should panic on unexpected error")]
    fn experiment_store_roundtrip() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("experiments.json");

        let mut store = ExperimentStore::new();
        store.record(Experiment::new(
            "exp_rt_1",
            "proj_1",
            "Hypothesis 1",
            "TRIGGER_A",
            "scope_a",
        ));
        store.record(Experiment::new(
            "exp_rt_2",
            "proj_1",
            "Hypothesis 2",
            "TRIGGER_B",
            "scope_b",
        ));

        save_experiment_store(&path, &store).unwrap();
        let loaded = load_experiment_store(&path).unwrap();
        assert_eq!(loaded.len(), 2);
    }
}
