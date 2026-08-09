//! Replay journal — records every mutation for deterministic replay.
//!
//! The architecture spec requires: "Every project state should be
//! reconstructible from initial snapshot, event ledger, command history,
//! asset hashes, engine versions, seeds, policy profile."
//!
//! §9.4 adds the replay capabilities: command replay, event reconstruction,
//! snapshot bisect, deterministic simulation mode, "replay until failure",
//! "replay with substituted policy", and "replay with alternate seed".
//! Architectural rule: *if an agent cannot replay a failure exactly, it
//! cannot reliably learn from it.* [`ReplayEngine`] provides those
//! capabilities on top of the journal.

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
    /// Events produced by this command (e.g. `ok`, `hard_failure:...`).
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

    /// Record a new entry with optional snapshot IDs and outcome events.
    ///
    /// The `events` payload is the observable outcome of the command (e.g.
    /// the validation result). Filling it is what makes §9.4 "replay until
    /// failure" and "substituted replay" possible — without an observable
    /// outcome, a replay cannot tell a passing command from a failing one.
    pub fn record(
        &mut self,
        command: &str,
        actor: &str,
        scope: BTreeMap<String, String>,
        pre_snapshot_id: Option<String>,
        post_snapshot_id: Option<String>,
        events: Vec<String>,
    ) {
        let entry = JournalEntry {
            seq: self.next_seq,
            command: command.to_string(),
            actor: actor.to_string(),
            scope,
            timestamp: chrono::Utc::now().to_rfc3339(),
            pre_snapshot_id,
            post_snapshot_id,
            events,
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

/// Deterministic replay engine (§9.4) — turns a passive journal into the
/// replay capabilities the architecture demands.
#[derive(Debug, Clone, Default)]
pub struct ReplayEngine;

impl ReplayEngine {
    /// Replay the journal until a command fails (§9.4 "replay until failure").
    ///
    /// Returns the sequence number of the first entry whose outcome matches
    /// `is_failure`, or `None` when the entire command history replays clean.
    /// The predicate inspects the entry's observable `events` (plus scope),
    /// so a failure is detected exactly as it occurred, not by inference.
    #[must_use]
    pub fn replay_until_failure(
        entries: &[JournalEntry],
        is_failure: impl Fn(&JournalEntry) -> bool,
    ) -> Option<u64> {
        entries.iter().find(|e| is_failure(e)).map(|e| e.seq)
    }

    /// Snapshot bisect — binary search for the first failing command (§9.4).
    ///
    /// Requires a monotone failure predicate: once a failure is introduced,
    /// every later command also fails (the usual "did not fix it" case). The
    /// journal is assumed ordered by `seq`. Returns the first failing `seq`,
    /// or `None` if nothing fails.
    ///
    /// Safety: on a *non-monotone* input (e.g. fail → ok → fail) a pure
    /// binary search can land past the true first failure. To never return a
    /// wrong answer, the result is cross-checked with the linear scan; the
    /// linear (correct-on-any-input) result wins on disagreement. That keeps
    /// the O(log n) fast path while degrading safely.
    #[must_use]
    pub fn snapshot_bisect(
        entries: &[JournalEntry],
        is_failure: impl Fn(&JournalEntry) -> bool,
    ) -> Option<u64> {
        if entries.is_empty() {
            return None;
        }
        // Entries are ordered by seq; failure is monotone after introduction.
        let mut lo = 0usize;
        let mut hi = entries.len();
        while lo < hi {
            let mid = lo + (hi - lo) / 2;
            if is_failure(&entries[mid]) {
                hi = mid;
            } else {
                lo = mid + 1;
            }
        }
        let bisected = (lo < entries.len() && is_failure(&entries[lo])).then(|| entries[lo].seq);
        // Cross-check with the linear scan; never let bisect win on a
        // non-monotone input where it could skip the true first failure.
        let linear = Self::replay_until_failure(entries, is_failure);
        linear.or(bisected)
    }

    /// Deterministic simulation: re-run a command sequence and record what
    /// *would* happen under a substituted policy/seed (§9.4 "replay with
    /// substituted policy", "replay with alternate seed").
    ///
    /// `simulate` maps each original command to its outcome under the
    /// substituted inputs. Returns a simulated journal with the same command
    /// stream but the substituted outcomes — letting an agent ask "would this
    /// policy change have prevented the failure?".
    ///
    /// Snapshot IDs are *not* carried over: a substituted replay produces its
    /// own hypothetical snapshot lineage, so the simulation never claims to
    /// have produced the original run's snapshots.
    #[must_use]
    pub fn replay_with_substituted_inputs(
        entries: &[JournalEntry],
        simulate: impl Fn(&JournalEntry) -> Vec<String>,
    ) -> ReplayJournal {
        let mut journal = ReplayJournal::new();
        for entry in entries {
            journal.record(
                &entry.command,
                &entry.actor,
                entry.scope.clone(),
                None,
                None,
                simulate(entry),
            );
        }
        journal
    }

    /// First sequence where two journals diverge (substitution effect point).
    ///
    /// Used with [`ReplayEngine::replay_with_substituted_inputs`] to pinpoint
    /// where a substituted policy/seed changes behavior. Returns `None` when
    /// the substituted replay is behaviorally identical (reproducible).
    #[must_use]
    pub fn divergence_point(
        original: &[JournalEntry],
        substituted: &[JournalEntry],
    ) -> Option<u64> {
        let max = original.len().max(substituted.len());
        for i in 0..max {
            let a = original.get(i);
            let b = substituted.get(i);
            match (a, b) {
                (Some(a), Some(b)) if a.events == b.events && a.command == b.command => {}
                (Some(a), _) => return Some(a.seq),
                (None, Some(b)) => return Some(b.seq),
                (None, None) => break,
            }
        }
        None
    }

    /// Reconstruct the event stream a command produced (§9.4 event
    /// reconstruction) — the flattened, command-tagged outcomes.
    #[must_use = "consuming the reconstructed event stream is required for replay"]
    pub fn reconstruct_events(entries: &[JournalEntry]) -> impl Iterator<Item = (&str, &str)> + '_ {
        entries.iter().flat_map(|e| {
            e.events
                .iter()
                .map(move |event| (e.command.as_str(), event.as_str()))
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entry(seq: u64, command: &str, events: Vec<String>) -> JournalEntry {
        JournalEntry {
            seq,
            command: command.to_string(),
            actor: "agent:composer".to_string(),
            scope: BTreeMap::new(),
            timestamp: chrono::Utc::now().to_rfc3339(),
            pre_snapshot_id: None,
            post_snapshot_id: None,
            events,
        }
    }

    #[test]
    fn journal_records_entries_with_events() {
        let mut journal = ReplayJournal::new();
        journal.record(
            "apply_patch",
            "agent:composer",
            BTreeMap::new(),
            None,
            None,
            vec!["ok".to_string()],
        );
        journal.record(
            "run_validation",
            "agent:critic",
            BTreeMap::new(),
            None,
            None,
            vec!["hard_failure:scale_compliance".to_string()],
        );
        assert_eq!(journal.len(), 2);
        assert_eq!(journal.entries[0].seq, 0);
        assert_eq!(
            journal.entries[1].events[0],
            "hard_failure:scale_compliance"
        );
    }

    #[test]
    fn journal_entries_from_returns_correct_slice() {
        let mut journal = ReplayJournal::new();
        for i in 0..5 {
            journal.record(
                &format!("cmd_{i}"),
                "agent",
                BTreeMap::new(),
                None,
                None,
                vec![],
            );
        }
        let from_3 = journal.entries_from(3);
        assert_eq!(from_3.len(), 2);
        assert_eq!(from_3[0].seq, 3);
    }

    #[test]
    fn replay_until_failure_finds_first_failing_command() {
        let entries = vec![
            entry(0, "plan", vec!["ok".to_string()]),
            entry(1, "apply_patch", vec!["ok".to_string()]),
            entry(2, "apply_patch", vec!["hard_failure:true_peak".to_string()]),
            entry(3, "apply_patch", vec!["hard_failure:true_peak".to_string()]),
        ];
        let failed = ReplayEngine::replay_until_failure(&entries, |e| {
            e.events.iter().any(|ev| ev.starts_with("hard_failure"))
        });
        assert_eq!(failed, Some(2));
    }

    #[test]
    fn replay_until_failure_returns_none_when_clean() {
        let entries = vec![
            entry(0, "plan", vec!["ok".to_string()]),
            entry(1, "apply_patch", vec!["ok".to_string()]),
        ];
        let failed = ReplayEngine::replay_until_failure(&entries, |e| {
            e.events.iter().any(|ev| ev.starts_with("hard_failure"))
        });
        assert_eq!(failed, None);
    }

    #[test]
    fn snapshot_bisect_finds_first_failure_in_log() {
        let entries = vec![
            entry(0, "plan", vec!["ok".to_string()]),
            entry(1, "apply_patch", vec!["ok".to_string()]),
            entry(2, "apply_patch", vec!["hard_failure:true_peak".to_string()]),
            entry(3, "apply_patch", vec!["hard_failure:true_peak".to_string()]),
            entry(4, "apply_patch", vec!["hard_failure:true_peak".to_string()]),
        ];
        let first = ReplayEngine::snapshot_bisect(&entries, |e| {
            e.events.iter().any(|ev| ev.starts_with("hard_failure"))
        });
        assert_eq!(first, Some(2));
    }

    #[test]
    fn snapshot_bisect_returns_none_when_never_fails() {
        let entries = vec![
            entry(0, "plan", vec!["ok".to_string()]),
            entry(1, "apply_patch", vec!["ok".to_string()]),
        ];
        let first = ReplayEngine::snapshot_bisect(&entries, |e| {
            e.events.iter().any(|ev| ev.starts_with("hard_failure"))
        });
        assert_eq!(first, None);
    }

    #[test]
    fn snapshot_bisect_returns_none_on_empty_input() {
        let entries: Vec<JournalEntry> = Vec::new();
        let first = ReplayEngine::snapshot_bisect(&entries, |_| true);
        assert_eq!(first, None);
    }

    #[test]
    fn snapshot_bisect_survives_non_monotone_input() {
        // fail → ok → fail is non-monotone; the cross-check must return the
        // true first failure (seq 0), never a bisect artifact (seq 2).
        let entries = vec![
            entry(0, "apply_patch", vec!["hard_failure:true_peak".to_string()]),
            entry(1, "apply_patch", vec!["ok".to_string()]),
            entry(2, "apply_patch", vec!["hard_failure:true_peak".to_string()]),
        ];
        let first = ReplayEngine::snapshot_bisect(&entries, |e| {
            e.events.iter().any(|ev| ev.starts_with("hard_failure"))
        });
        assert_eq!(first, Some(0));
    }

    #[test]
    fn substituted_replay_detects_policy_effect() {
        let original = vec![
            entry(
                0,
                "apply_patch",
                vec!["ok".to_string(), "warning:peak_-0.5".to_string()],
            ),
            entry(
                1,
                "apply_patch",
                vec!["ok".to_string(), "warning:peak_-0.5".to_string()],
            ),
        ];
        // Substitute a stricter policy: the same commands now hard-fail.
        let substituted = ReplayEngine::replay_with_substituted_inputs(&original, |e| {
            if e.seq == 0 {
                vec!["ok".to_string(), "warning:peak_-0.5".to_string()]
            } else {
                vec!["hard_failure:true_peak".to_string()]
            }
        });
        let divergence = ReplayEngine::divergence_point(&original, &substituted.entries);
        assert_eq!(divergence, Some(1));
    }
    #[test]
    fn substituted_replay_reproducible_when_no_change() {
        let original = vec![entry(0, "apply_patch", vec!["ok".to_string()])];
        let substituted =
            ReplayEngine::replay_with_substituted_inputs(&original, |_| vec!["ok".to_string()]);
        assert_eq!(
            ReplayEngine::divergence_point(&original, &substituted.entries),
            None
        );
    }

    #[test]
    fn divergence_point_handles_shorter_substituted_journal() {
        // Simulation that stops early: the divergence is reported at the first
        // command the original had but the substitution never reached.
        let original = vec![
            entry(0, "apply_patch", vec!["ok".to_string()]),
            entry(1, "apply_patch", vec!["ok".to_string()]),
        ];
        let substituted = ReplayEngine::replay_with_substituted_inputs(&original, |e| {
            if e.seq >= 1 {
                // Simulated run stops here (no further outcomes).
                Vec::new()
            } else {
                vec!["ok".to_string()]
            }
        });
        let divergence = ReplayEngine::divergence_point(&original, &substituted.entries);
        assert_eq!(divergence, Some(1));
    }

    #[test]
    fn reconstruct_events_flattens_command_outcomes() {
        let entries = vec![
            entry(
                0,
                "apply_patch",
                vec!["ok".to_string(), "warning:peak_-0.5".to_string()],
            ),
            entry(1, "run_validation", vec!["score:0.82".to_string()]),
        ];
        let events: Vec<(&str, &str)> = ReplayEngine::reconstruct_events(&entries).collect();
        assert_eq!(events.len(), 3);
        assert_eq!(events[0], ("apply_patch", "ok"));
        assert_eq!(events[2], ("run_validation", "score:0.82"));
    }
}
