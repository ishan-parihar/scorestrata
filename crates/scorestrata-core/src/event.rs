//! Event ledger types — the append-only log of all mutations.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::ids::{BranchId, CommandId, EventId, ProjectId, SnapshotId};

/// An append-only event ledger that records all mutations.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EventLedger {
    /// Project this ledger belongs to.
    pub project_id: ProjectId,
    /// All events in chronological order.
    pub events: Vec<LedgerEntry>,
}

impl EventLedger {
    /// Create a new empty ledger.
    #[must_use]
    pub const fn new(project_id: ProjectId) -> Self {
        Self {
            project_id,
            events: Vec::new(),
        }
    }

    /// Append an event to the ledger.
    pub fn append(&mut self, entry: LedgerEntry) {
        self.events.push(entry);
    }

    /// Get the total number of events.
    #[must_use]
    pub const fn event_count(&self) -> usize {
        self.events.len()
    }

    /// Get events for a specific command.
    #[must_use]
    pub fn events_for_command(&self, command_id: CommandId) -> Vec<&LedgerEntry> {
        self.events
            .iter()
            .filter(|e| e.command_id == command_id)
            .collect()
    }

    /// Get events for a specific branch.
    #[must_use]
    pub fn events_for_branch(&self, branch_id: BranchId) -> Vec<&LedgerEntry> {
        self.events
            .iter()
            .filter(|e| e.branch_id == Some(branch_id))
            .collect()
    }
}

/// A single entry in the event ledger.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LedgerEntry {
    /// Unique event ID.
    pub event_id: EventId,
    /// Command that produced this event.
    pub command_id: CommandId,
    /// Event type (e.g. `pattern_updated`, `snapshot_created`).
    pub event_type: String,
    /// Branch this event occurred on.
    pub branch_id: Option<BranchId>,
    /// Snapshot at the time of this event.
    pub snapshot_id: Option<SnapshotId>,
    /// Event payload (event-specific data).
    pub payload: serde_json::Value,
    /// When the event occurred.
    pub timestamp: DateTime<Utc>,
}
