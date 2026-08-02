//! Replay journal — records every mutation for deterministic replay.
//!
//! The architecture spec requires: "Every project state should be
//! reconstructible from initial snapshot, event ledger, command history,
//! asset hashes, engine versions, seeds, policy profile."

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

/// A single journal entry recording a mutation.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JournalEntry {
    /// Monotonic sequence number.
    pub seq: u64,
    /// Command that was executed.
    pub command: String,
    /// Actor that performed the action.
    pub actor: String,
    /// Scope of the mutation.
    pub scope: BTreeMap<String, String>,
    /// Timestamp.
    pub timestamp: String,
    /// Snapshot ID before this entry (for rollback).
    pub pre_snapshot_id: Option<String>,
    /// Snapshot ID after this entry.
    pub post_snapshot_id: Option<String>,
    /// Events produced by this command.
    pub events: Vec<String>,
}

/// Replay journal — ordered log of all mutations.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ReplayJournal {
    /// All journal entries in order.
    pub entries: Vec<JournalEntry>,
    /// Next sequence number.
    pub next_seq: u64,
}

impl ReplayJournal {
    /// Create a new empty journal.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Record a new entry with optional snapshot IDs.
    pub fn record(
        &mut self,
        command: &str,
        actor: &str,
        scope: BTreeMap<String, String>,
        pre_snapshot_id: Option<String>,
        post_snapshot_id: Option<String>,
    ) {
        let entry = JournalEntry {
            seq: self.next_seq,
            command: command.to_string(),
            actor: actor.to_string(),
            scope,
            timestamp: chrono::Utc::now().to_rfc3339(),
            pre_snapshot_id,
            post_snapshot_id,
            events: vec![],
        };
        self.entries.push(entry);
        self.next_seq += 1;
    }

    /// Get entries from a given sequence number.
    #[must_use]
    pub fn entries_from(&self, from_seq: u64) -> Vec<&JournalEntry> {
        self.entries.iter().filter(|e| e.seq >= from_seq).collect()
    }

    /// Total entries.
    #[must_use]
    pub const fn len(&self) -> usize {
        self.entries.len()
    }

    /// Whether the journal is empty.
    #[must_use]
    pub const fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn journal_records_entries() {
        let mut journal = ReplayJournal::new();
        journal.record("apply_patch", "agent:composer", BTreeMap::new(), None, None);
        journal.record(
            "run_validation",
            "agent:critic",
            BTreeMap::new(),
            None,
            None,
        );
        assert_eq!(journal.len(), 2);
        assert_eq!(journal.entries[0].seq, 0);
        assert_eq!(journal.entries[1].seq, 1);
    }

    #[test]
    fn journal_entries_from_returns_correct_slice() {
        let mut journal = ReplayJournal::new();
        for i in 0..5 {
            journal.record(&format!("cmd_{i}"), "agent", BTreeMap::new(), None, None);
        }
        let from_3 = journal.entries_from(3);
        assert_eq!(from_3.len(), 2);
        assert_eq!(from_3[0].seq, 3);
    }
}
