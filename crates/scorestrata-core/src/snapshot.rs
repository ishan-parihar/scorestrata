//! Snapshot and branch types for rollback and candidate comparison.
//!
//! Snapshots capture the complete project state at a point in time,
//! enabling rollback, comparison, and deterministic replay.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::ids::{ArtifactId, BranchId, CandidateId, CommandId, ProjectId, SnapshotId};

/// A snapshot of the complete project state.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Snapshot {
    /// Unique identifier.
    pub id: SnapshotId,
    /// Project this snapshot belongs to.
    pub project_id: ProjectId,
    /// Branch this snapshot is on.
    pub branch_id: BranchId,
    /// Human-readable label.
    pub label: String,
    /// When the snapshot was created.
    pub created_at: DateTime<Utc>,
    /// The command that produced this snapshot.
    pub command_id: Option<CommandId>,
    /// Hash of the project state at this point.
    pub state_hash: String,
    /// Optional scorecard at snapshot time.
    pub scorecard_hash: Option<String>,
}

/// A branch for isolating experimental mutations.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Branch {
    /// Unique identifier.
    pub id: BranchId,
    /// Human-readable name.
    pub name: String,
    /// Parent branch (None for main).
    pub parent_id: Option<BranchId>,
    /// Snapshot this branch was created from.
    pub base_snapshot_id: SnapshotId,
    /// When the branch was created.
    pub created_at: DateTime<Utc>,
    /// Whether this is the main branch.
    pub is_main: bool,
    /// Status of the branch.
    pub status: BranchStatus,
}

/// Status of a candidate branch.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BranchStatus {
    /// Branch is active and being worked on.
    Active,
    /// Branch has been merged into parent.
    Merged,
    /// Branch has been archived (loser of a tournament).
    Archived,
    /// Branch has been abandoned.
    Abandoned,
}

/// A candidate for selection in a tournament.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Candidate {
    /// Unique identifier.
    pub id: CandidateId,
    /// Branch this candidate lives on.
    pub branch_id: BranchId,
    /// Snapshot this candidate was created from.
    pub parent_snapshot_id: SnapshotId,
    /// Patches applied to create this candidate.
    #[serde(default)]
    pub patch_set: Vec<PatchRef>,
    /// Scorecard for this candidate.
    pub scorecard_hash: Option<String>,
    /// Artifact IDs produced (renders, exports).
    #[serde(default)]
    pub render_artifact_ids: Vec<ArtifactId>,
    /// Status.
    pub status: CandidateStatus,
    /// Human-readable description.
    pub description: String,
}

/// Reference to a patch applied to a candidate.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PatchRef {
    /// Patch operation name.
    pub op: String,
    /// Patch parameters.
    pub params: serde_json::Value,
}

/// Status of a candidate.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CandidateStatus {
    /// Candidate is being evaluated.
    Evaluating,
    /// Candidate passed all gates.
    Passed,
    /// Candidate failed one or more gates.
    Failed,
    /// Candidate was selected as winner.
    Winner,
    /// Candidate was rejected.
    Rejected,
    /// Candidate was archived.
    Archived,
}
