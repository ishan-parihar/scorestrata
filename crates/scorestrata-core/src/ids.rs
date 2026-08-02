//! Newtyped identifiers for every entity in Scorestrata.
//!
//! Each ID is a wrapper around `Uuid` with a distinct type, preventing
//! accidental misuse (e.g. passing a `SegmentId` where a `LayerId` is expected).

use std::fmt;
use uuid::Uuid;

/// Macro to generate a newtyped ID struct with Display, `FromStr`, and serde support.
macro_rules! define_id {
    ($name:ident, $doc:expr) => {
        #[doc = $doc]
        #[derive(
            Debug,
            Clone,
            Copy,
            PartialEq,
            Eq,
            Hash,
            PartialOrd,
            Ord,
            serde::Serialize,
            serde::Deserialize,
        )]
        pub struct $name(Uuid);

        impl $name {
            /// Create a new random ID.
            #[must_use]
            pub fn new() -> Self {
                Self(Uuid::new_v4())
            }

            /// Create an ID from a fixed UUID (useful for deterministic tests).
            #[must_use]
            pub const fn from_uuid(uuid: Uuid) -> Self {
                Self(uuid)
            }

            /// Get the inner UUID.
            #[must_use]
            pub const fn as_uuid(&self) -> &Uuid {
                &self.0
            }

            /// Get the ID as a hex string.
            pub fn as_hex(&self) -> String {
                self.0.simple().to_string()
            }
        }

        impl Default for $name {
            fn default() -> Self {
                Self::new()
            }
        }

        impl fmt::Display for $name {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                write!(f, "{}", self.0.simple())
            }
        }

        impl std::str::FromStr for $name {
            type Err = uuid::Error;

            fn from_str(s: &str) -> Result<Self, Self::Err> {
                Ok(Self(Uuid::parse_str(s)?))
            }
        }
    };
}

// ── Musical entity IDs ──────────────────────────────────────────────

define_id!(ProjectId, "Unique identifier for a project.");
define_id!(
    SegmentId,
    "Unique identifier for a segment (intro, verse, chorus, etc.)."
);
define_id!(LayerId, "Unique identifier for a layer within a segment.");
define_id!(TrackId, "Unique identifier for a track.");
define_id!(PatternId, "Unique identifier for a pattern.");
define_id!(
    PatternInstanceId,
    "Unique identifier for a pattern instance placed in a segment."
);
define_id!(NoteId, "Unique identifier for a note.");
define_id!(StepId, "Unique identifier for a step in a pattern.");
define_id!(MotifId, "Unique identifier for a motif.");
define_id!(MotifInstanceId, "Unique identifier for a motif instance.");

// ── Operational entity IDs ──────────────────────────────────────────

define_id!(CommandId, "Unique identifier for a command.");
define_id!(SnapshotId, "Unique identifier for a snapshot.");
define_id!(BranchId, "Unique identifier for a candidate branch.");
define_id!(CandidateId, "Unique identifier for a candidate.");
define_id!(ExperimentId, "Unique identifier for an experiment.");
define_id!(BudgetId, "Unique identifier for a budget envelope.");
define_id!(PolicyId, "Unique identifier for a policy profile.");
define_id!(LeaseId, "Unique identifier for a task lease.");
define_id!(TaskId, "Unique identifier for an engineering task.");

// ── Evidence entity IDs ─────────────────────────────────────────────

define_id!(DiagnosticId, "Unique identifier for a diagnostic.");
define_id!(MetricId, "Unique identifier for a metric sample.");
define_id!(EvidenceId, "Unique identifier for an evidence record.");
define_id!(ScorecardId, "Unique identifier for a scorecard.");
define_id!(
    ArtifactId,
    "Unique identifier for an artifact (render, export, etc.)."
);
define_id!(EventId, "Unique identifier for a ledger event.");

// ── Learning entity IDs ─────────────────────────────────────────────

define_id!(LessonId, "Unique identifier for a lesson learned.");
define_id!(RepairTemplateId, "Unique identifier for a repair template.");
define_id!(
    CalibrationProfileId,
    "Unique identifier for a calibration profile."
);
