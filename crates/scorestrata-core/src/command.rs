//! Command and receipt types — the mutation envelope.
//!
//! Every mutating operation in Scorestrata flows through a Command → Receipt
//! pipeline. Commands carry intent, receipts carry evidence.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::diagnostic::ScoreDelta;
use crate::ids::{
    BranchId, BudgetId, CommandId, EventId, LayerId, PatternId, ProjectId, SegmentId, SnapshotId,
};

/// The safety class of a command.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SafetyClass {
    /// Read-only operations.
    Read,
    /// Mutating operations.
    Write,
    /// Render operations (produces audio).
    Render,
    /// Destructive operations (delete, overwrite).
    Destructive,
    /// Governance operations (policy, budget changes).
    Governance,
    /// Engineering operations (code changes).
    Engineering,
}

/// A command envelope — the intent to perform a mutation.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Command {
    /// Unique command identifier.
    pub id: CommandId,
    /// Idempotency key for deduplication.
    pub idempotency_key: String,
    /// Command type (e.g. `apply_patch`, `create_pattern`).
    pub command_type: String,
    /// Who issued this command.
    pub actor: String,
    /// Project this command targets.
    pub project_id: ProjectId,
    /// Branch this command targets.
    pub branch_id: Option<BranchId>,
    /// Whether this is a dry run.
    #[serde(default)]
    pub dry_run: bool,
    /// Scope of the command.
    pub scope: CommandScope,
    /// Command-specific parameters.
    pub params: serde_json::Value,
    /// Budget ID for cost tracking.
    pub budget_id: Option<BudgetId>,
    /// When the command was created.
    pub created_at: DateTime<Utc>,
}

/// The scope of a command — what part of the project it targets.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct CommandScope {
    /// Target segment (if any).
    pub segment_id: Option<SegmentId>,
    /// Target layer (if any).
    pub layer_id: Option<LayerId>,
    /// Target pattern (if any).
    pub pattern_id: Option<PatternId>,
}

/// The status of a command execution.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CommandStatus {
    /// Command succeeded.
    Succeeded,
    /// Command failed.
    Failed,
    /// Command was rejected by policy.
    Rejected,
    /// Command was a dry run (no mutation).
    DryRun,
}

/// A command receipt — the structured result of a mutation.
///
/// Receipts are the primary feedback mechanism for agents. They answer:
/// "Did it work? What changed? What should I do next?"
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CommandReceipt {
    /// The command this receipt is for.
    pub command_id: CommandId,
    /// Final status.
    pub status: CommandStatus,
    /// Whether this was a dry run.
    pub dry_run: bool,
    /// Snapshot created by this command (if any).
    pub snapshot_id: Option<SnapshotId>,
    /// Branch this command operated on.
    pub branch_id: Option<BranchId>,
    /// Score delta (what changed in the scorecard).
    #[serde(default)]
    pub score_delta: ScoreDelta,
    /// Policy decisions evaluated.
    #[serde(default)]
    pub policy_decisions: Vec<PolicyDecisionRef>,
    /// Events emitted by this command.
    #[serde(default)]
    pub events: Vec<String>,
    /// Evidence URIs pointing to relevant data.
    #[serde(default)]
    pub evidence_uris: Vec<String>,
    /// URIs the agent should inspect next.
    #[serde(default)]
    pub focus_uris: Vec<String>,
    /// Suggested next actions.
    #[serde(default)]
    pub next_actions: Vec<String>,
    /// Cost of this command.
    pub cost: CommandCost,
    /// Warnings produced.
    #[serde(default)]
    pub warnings: Vec<String>,
    /// Replay cursor for deterministic replay.
    pub replay_cursor: Option<EventId>,
}

/// Cost metrics for a command execution.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct CommandCost {
    /// CPU time in milliseconds.
    #[serde(default)]
    pub cpu_ms: u64,
    /// Number of tool calls made.
    #[serde(default)]
    pub tool_calls: u32,
    /// Render time in seconds.
    #[serde(default)]
    pub render_seconds: f64,
}

/// A reference to a policy decision made during command execution.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PolicyDecisionRef {
    /// Policy rule that was evaluated.
    pub rule: String,
    /// Whether it passed.
    pub passed: bool,
    /// Human-readable explanation.
    pub explanation: Option<String>,
}
