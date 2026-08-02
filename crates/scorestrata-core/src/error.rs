//! Structured error taxonomy for Scorestrata.
//!
//! Every error is actionable: it carries a code, severity, scope, focus URI,
//! suggested actions, and whether it is retryable. This allows agents to
//! diagnose and repair without guessing.

use std::fmt;

/// Severity level for diagnostics and errors.
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, serde::Serialize, serde::Deserialize,
)]
#[serde(rename_all = "snake_case")]
pub enum Severity {
    /// Informational — no action required.
    Info,
    /// Warning — may need attention.
    Warning,
    /// Error — must be addressed.
    Error,
    /// Hard failure — blocks further progress.
    Critical,
}

impl fmt::Display for Severity {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Info => write!(f, "info"),
            Self::Warning => write!(f, "warning"),
            Self::Error => write!(f, "error"),
            Self::Critical => write!(f, "critical"),
        }
    }
}

/// The unified error type for all Scorestrata operations.
///
/// Every variant carries enough context for an agent to understand what went
/// wrong, where, and what to try next.
#[derive(Debug, thiserror::Error)]
pub enum ScorestrataError {
    // ── Config & Schema ─────────────────────────────────────────────
    /// Configuration file is invalid or missing required fields.
    #[error("config invalid: {message}")]
    ConfigInvalid {
        /// Human-readable description of the problem.
        message: String,
        /// Focus URI pointing to the offending config section.
        focus_uri: Option<String>,
    },

    /// A schema validation constraint was violated.
    #[error("schema violation: {message}")]
    SchemaViolation {
        /// Description of the violation.
        message: String,
        /// The schema path that was violated.
        schema_path: Option<String>,
    },

    // ── Asset & File ────────────────────────────────────────────────
    /// A required asset file is missing.
    #[error("asset missing: {path}")]
    AssetMissing {
        /// Path to the missing asset.
        path: String,
    },

    /// A file could not be read or written.
    #[error("io error at {path}: {source}")]
    Io {
        /// Path that caused the error.
        path: String,
        /// Underlying I/O error.
        #[source]
        source: std::io::Error,
    },

    /// A serialization/deserialization error occurred.
    #[error("serialization error: {message}")]
    Serialization {
        /// Description of the problem.
        message: String,
    },

    // ── Theory & Musical ────────────────────────────────────────────
    /// A music theory constraint was violated.
    #[error("theory violation: {message}")]
    TheoryViolation {
        /// Description of the violation.
        message: String,
        /// Focus URI pointing to the offending note/segment.
        focus_uri: Option<String>,
    },

    /// A note is outside the required scale.
    #[error("out-of-scale note: {note_name} at bar {bar} beat {beat}")]
    OutOfScale {
        /// The note that is out of scale.
        note_name: String,
        /// Bar number (0-indexed).
        bar: u32,
        /// Beat number (0-indexed within the bar).
        beat: u32,
    },

    // ── Compiler ────────────────────────────────────────────────────
    /// The compiler encountered an error during a pass.
    #[error("compiler error in pass '{pass}': {message}")]
    CompilerError {
        /// Which compiler pass failed.
        pass: String,
        /// Description of the problem.
        message: String,
    },

    /// A required segment or layer is missing.
    #[error("missing {entity_type} '{entity_id}'")]
    MissingEntity {
        /// Type of entity that is missing.
        entity_type: String,
        /// ID of the missing entity.
        entity_id: String,
    },

    // ── Validation ──────────────────────────────────────────────────
    /// Validation failed with one or more diagnostics.
    #[error("validation failed: {count} diagnostics")]
    ValidationFailed {
        /// Number of diagnostics produced.
        count: usize,
    },

    /// A hard gate failed validation.
    #[error("hard gate failed: {gate_name}")]
    HardGateFailed {
        /// Name of the gate that failed.
        gate_name: String,
    },

    // ── Render ──────────────────────────────────────────────────────
    /// An error occurred during audio rendering.
    #[error("render error: {message}")]
    RenderError {
        /// Description of the problem.
        message: String,
    },

    /// The render timed out.
    #[error("render timeout after {0}s")]
    RenderTimeout(
        /// Seconds before timeout.
        u64,
    ),

    // ── Policy & Governance ─────────────────────────────────────────
    /// A policy rule rejected the operation.
    #[error("policy violation: {rule}")]
    PolicyViolation {
        /// Which policy rule was violated.
        rule: String,
        /// Human-readable explanation.
        explanation: String,
    },

    /// The operation requires human approval.
    #[error("requires human approval: {reason}")]
    RequiresHumanApproval {
        /// Why approval is needed.
        reason: String,
    },

    // ── Budget ──────────────────────────────────────────────────────
    /// A budget limit has been exceeded.
    #[error("budget exceeded: {budget_type} limit of {limit}")]
    BudgetExceeded {
        /// Which budget type was exceeded (iterations, `render_seconds`, etc.).
        budget_type: String,
        /// The limit that was exceeded.
        limit: String,
    },

    // ── Storage ─────────────────────────────────────────────────────
    /// A project bundle is corrupted or incomplete.
    #[error("project corrupted: {message}")]
    ProjectCorrupted {
        /// Description of the corruption.
        message: String,
    },

    /// A snapshot could not be restored.
    #[error("snapshot restore failed: {snapshot_id}")]
    SnapshotRestoreFailed {
        /// The snapshot ID that failed to restore.
        snapshot_id: String,
    },

    // ── Agent ───────────────────────────────────────────────────────
    /// The agent kernel is stuck (stagnation detected).
    #[error("agent stagnation: no improvement for {rounds} rounds")]
    AgentStagnation {
        /// Number of rounds without improvement.
        rounds: u32,
    },

    /// A task lease has expired.
    #[error("lease expired for task {task_id}")]
    LeaseExpired {
        /// The task whose lease expired.
        task_id: String,
    },

    // ── General ─────────────────────────────────────────────────────
    /// An invalid project state transition was attempted.
    #[error("invalid transition: {from} → {to}")]
    InvalidTransition {
        /// The source state.
        from: String,
        /// The target state.
        to: String,
    },

    /// An internal error that should not happen.
    #[error("internal error: {message}")]
    Internal {
        /// Description of the problem.
        message: String,
    },

    /// A feature is not yet implemented.
    #[error("not implemented: {feature}")]
    NotImplemented {
        /// Which feature is missing.
        feature: String,
    },
}

impl ScorestrataError {
    /// Returns the severity level of this error.
    #[must_use]
    pub const fn severity(&self) -> Severity {
        match self {
            Self::ConfigInvalid { .. }
            | Self::SchemaViolation { .. }
            | Self::MissingEntity { .. }
            | Self::ProjectCorrupted { .. }
            | Self::AssetMissing { .. }
            | Self::Io { .. }
            | Self::Serialization { .. }
            | Self::CompilerError { .. }
            | Self::ValidationFailed { .. }
            | Self::HardGateFailed { .. }
            | Self::RenderError { .. }
            | Self::RenderTimeout { .. }
            | Self::SnapshotRestoreFailed { .. }
            | Self::Internal { .. }
            | Self::NotImplemented { .. } => Severity::Error,

            Self::TheoryViolation { .. }
            | Self::OutOfScale { .. }
            | Self::PolicyViolation { .. }
            | Self::RequiresHumanApproval { .. }
            | Self::AgentStagnation { .. }
            | Self::LeaseExpired { .. }
            | Self::InvalidTransition { .. } => Severity::Warning,

            Self::BudgetExceeded { .. } => Severity::Critical,
        }
    }

    /// Returns whether this error is retryable.
    #[must_use]
    pub const fn is_retryable(&self) -> bool {
        matches!(
            self,
            Self::Io { .. }
                | Self::RenderTimeout { .. }
                | Self::LeaseExpired { .. }
                | Self::AgentStagnation { .. }
        )
    }

    /// Returns a focus URI if one is available.
    #[must_use]
    pub fn focus_uri(&self) -> Option<&str> {
        match self {
            Self::ConfigInvalid { focus_uri, .. } | Self::TheoryViolation { focus_uri, .. } => {
                focus_uri.as_deref()
            }
            _ => None,
        }
    }

    /// Returns suggested actions for this error.
    #[must_use]
    pub fn suggested_actions(&self) -> Vec<&'static str> {
        match self {
            Self::ConfigInvalid { .. } => vec!["run `ss config validate`", "check config.toml"],
            Self::AssetMissing { .. } => vec!["check asset paths in config", "run `ss doctor`"],
            Self::TheoryViolation { .. } => {
                vec!["quantize to scale", "check key signature"]
            }
            Self::OutOfScale { .. } => {
                vec!["quantize_layer_to_scale", "check scale configuration"]
            }
            Self::ValidationFailed { .. } => {
                vec![
                    "run `ss validate` for details",
                    "run `ss agent suggest-fixes`",
                ]
            }
            Self::HardGateFailed { .. } => {
                vec!["run `ss validate --gate`", "apply structural patch"]
            }
            Self::PolicyViolation { .. } => {
                vec!["check policy.toml", "run `ss policy show`"]
            }
            Self::BudgetExceeded { .. } => vec!["increase budget", "reduce scope"],
            Self::AgentStagnation { .. } => {
                vec!["widen candidate diversity", "escalate to human"]
            }
            _ => vec![],
        }
    }
}
