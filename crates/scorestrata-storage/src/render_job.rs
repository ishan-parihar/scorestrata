//! Render job store — tracks asynchronous render work.
//!
//! The architecture spec §8.2 lists `RenderJob` as a contract entity and the
//! tool catalog (§10.5) exposes `render_candidate_preview`, `get_render_job`,
//! and `cancel_render_job`. Rendering is a long-running, potentially
//! expensive operation, so the system models it as a job with a lifecycle:
//! queued → rendering → completed / failed, with `cancel` allowed while the
//! job is queued or rendering.

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

/// Lifecycle state of a render job.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RenderJobStatus {
    /// Submitted, not yet started.
    Queued,
    /// Actively rendering.
    Rendering,
    /// Render completed with an artifact.
    Completed,
    /// Render failed.
    Failed,
    /// Cancelled before completion.
    Cancelled,
}

impl RenderJobStatus {
    /// Whether the job is in a terminal state (no further transitions).
    #[must_use]
    pub const fn is_terminal(self) -> bool {
        matches!(self, Self::Completed | Self::Failed | Self::Cancelled)
    }
}

/// A single render job.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RenderJob {
    /// Unique job ID.
    pub id: String,
    /// Project the render belongs to.
    pub project_id: String,
    /// Render mode (`master`, `stems`, `preview`).
    pub mode: String,
    /// Current lifecycle status.
    pub status: RenderJobStatus,
    /// Timestamp when the job was submitted.
    pub submitted_at: String,
    /// Timestamp when rendering completed (if terminal-complete).
    pub completed_at: Option<String>,
    /// Artifact URI of the rendered output.
    pub artifact_uri: Option<String>,
    /// Human-readable error if the render failed.
    pub error: Option<String>,
}

impl RenderJob {
    /// Create a new queued render job.
    #[must_use]
    pub fn new(id: &str, project_id: &str, mode: &str) -> Self {
        Self {
            id: id.to_string(),
            project_id: project_id.to_string(),
            mode: mode.to_string(),
            status: RenderJobStatus::Queued,
            submitted_at: chrono::Utc::now().to_rfc3339(),
            completed_at: None,
            artifact_uri: None,
            error: None,
        }
    }
}

/// Store of render jobs keyed by ID.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct RenderJobStore {
    /// All known render jobs.
    pub jobs: BTreeMap<String, RenderJob>,
}

impl RenderJobStore {
    /// Create a new empty render job store.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Submit a new queued render job.
    pub fn submit(&mut self, job: RenderJob) {
        self.jobs.insert(job.id.clone(), job);
    }

    /// Get a render job by ID.
    #[must_use]
    pub fn get(&self, id: &str) -> Option<&RenderJob> {
        self.jobs.get(id)
    }

    /// Mark a queued job as actively rendering.
    ///
    /// Returns `false` if the job does not exist or is not queued.
    pub fn start(&mut self, id: &str) -> bool {
        let Some(job) = self.jobs.get_mut(id) else {
            return false;
        };
        if job.status != RenderJobStatus::Queued {
            return false;
        }
        job.status = RenderJobStatus::Rendering;
        true
    }

    /// Complete a rendering job with its artifact URI.
    ///
    /// Returns `false` if the job does not exist or is not rendering.
    pub fn complete(&mut self, id: &str, artifact_uri: &str) -> bool {
        let Some(job) = self.jobs.get_mut(id) else {
            return false;
        };
        if job.status != RenderJobStatus::Rendering {
            return false;
        }
        job.status = RenderJobStatus::Completed;
        job.completed_at = Some(chrono::Utc::now().to_rfc3339());
        job.artifact_uri = Some(artifact_uri.to_string());
        true
    }

    /// Fail a rendering job with an error message.
    ///
    /// Returns `false` if the job does not exist or is not rendering.
    pub fn fail(&mut self, id: &str, error: &str) -> bool {
        let Some(job) = self.jobs.get_mut(id) else {
            return false;
        };
        if job.status != RenderJobStatus::Rendering {
            return false;
        }
        job.status = RenderJobStatus::Failed;
        job.completed_at = Some(chrono::Utc::now().to_rfc3339());
        job.error = Some(error.to_string());
        true
    }

    /// Cancel a queued or rendering job.
    ///
    /// Returns `false` if the job does not exist or is already terminal.
    pub fn cancel(&mut self, id: &str) -> bool {
        let Some(job) = self.jobs.get_mut(id) else {
            return false;
        };
        if job.status.is_terminal() {
            return false;
        }
        job.status = RenderJobStatus::Cancelled;
        job.completed_at = Some(chrono::Utc::now().to_rfc3339());
        true
    }

    /// All jobs for a project.
    #[must_use]
    pub fn jobs_for_project(&self, project_id: &str) -> Vec<&RenderJob> {
        self.jobs
            .values()
            .filter(|j| j.project_id == project_id)
            .collect()
    }

    /// Total number of tracked jobs.
    #[must_use]
    pub fn len(&self) -> usize {
        self.jobs.len()
    }

    /// Whether the store is empty.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.jobs.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[expect(clippy::expect_used, reason = "Test asserts the job exists")]
    #[test]
    fn lifecycle_progresses_queued_to_completed() {
        let mut store = RenderJobStore::new();
        store.submit(RenderJob::new("job_1", "proj_1", "stems"));
        assert!(store.start("job_1"));
        assert!(store.complete("job_1", "artifact://proj_1/stems.wav"));
        let job = store.get("job_1").expect("job exists");
        assert_eq!(job.status, RenderJobStatus::Completed);
        assert_eq!(
            job.artifact_uri.as_deref(),
            Some("artifact://proj_1/stems.wav")
        );
    }

    #[test]
    fn cancel_allowed_only_before_terminal() {
        let mut store = RenderJobStore::new();
        store.submit(RenderJob::new("job_1", "proj_1", "master"));
        assert!(store.cancel("job_1"), "queued job cancels");
        assert!(!store.cancel("job_1"), "terminal job cannot cancel again");
    }

    #[test]
    fn start_rejects_non_queued_jobs() {
        let mut store = RenderJobStore::new();
        store.submit(RenderJob::new("job_1", "proj_1", "master"));
        assert!(store.start("job_1"));
        assert!(!store.start("job_1"), "cannot start a rendering job");
        assert!(!store.start("missing"), "unknown job cannot start");
    }
}
