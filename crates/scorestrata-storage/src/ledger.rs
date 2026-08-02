//! Event ledger persistence — read/write append-only events to disk.
//!
//! The architecture spec requires: "Every mutation should append an event.
//! Benefits: rollback, audit, debugging, reproducibility, branch comparison,
//! agent traceability."

use std::io::Write as _;
use std::path::Path;

use scorestrata_core::error::ScorestrataError;
use scorestrata_core::event::{EventLedger, LedgerEntry};

use crate::StorageResult;

/// Append a single ledger entry to the events.jsonl file.
///
/// Each line is a self-contained JSON object (JSONL format).
///
/// # Errors
///
/// Returns [`ScorestrataError::Io`] if the file cannot be written.
pub fn append_event(path: &Path, entry: &LedgerEntry) -> StorageResult<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| ScorestrataError::Io {
            path: parent.display().to_string(),
            source: e,
        })?;
    }
    let mut line = serde_json::to_string(entry).map_err(|e| ScorestrataError::Serialization {
        message: format!("Failed to serialize ledger entry: {e}"),
    })?;
    line.push('\n');
    std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)
        .map_err(|e| ScorestrataError::Io {
            path: path.display().to_string(),
            source: e,
        })?
        .write_all(line.as_bytes())
        .map_err(|e| ScorestrataError::Io {
            path: path.display().to_string(),
            source: e,
        })?;
    Ok(())
}

/// Read all ledger entries from the events.jsonl file.
///
/// Malformed lines are skipped with a warning-style count.
///
/// # Errors
///
/// Returns [`ScorestrataError::Io`] if the file cannot be read,
/// or [`ScorestrataError::Serialization`] if the file content is not valid UTF-8.
pub fn read_ledger(path: &Path) -> StorageResult<Vec<LedgerEntry>> {
    if !path.exists() {
        return Ok(Vec::new());
    }
    let content = std::fs::read_to_string(path).map_err(|e| ScorestrataError::Io {
        path: path.display().to_string(),
        source: e,
    })?;
    let mut entries = Vec::new();
    for (line_num, line) in content.lines().enumerate() {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }
        match serde_json::from_str::<LedgerEntry>(trimmed) {
            Ok(entry) => entries.push(entry),
            Err(e) => {
                eprintln!(
                    "Warning: skipping malformed ledger entry at line {}: {e}",
                    line_num + 1
                );
            }
        }
    }
    Ok(entries)
}

/// Reconstruct an [`EventLedger`] from the on-disk events.jsonl.
///
/// # Errors
///
/// Returns [`ScorestrataError::Io`] or [`ScorestrataError::Serialization`]
/// if the file cannot be read or parsed.
pub fn load_event_ledger(
    project_path: &Path,
    project_id: scorestrata_core::ids::ProjectId,
) -> StorageResult<EventLedger> {
    let path = project_path.join("ledger").join("events.jsonl");
    let events = read_ledger(&path)?;
    Ok(EventLedger { project_id, events })
}

/// Persist an entire [`EventLedger`] to disk (overwrite).
///
/// # Errors
///
/// Returns [`ScorestrataError::Io`] or [`ScorestrataError::Serialization`]
/// if the file cannot be written.
pub fn save_event_ledger(project_path: &Path, ledger: &EventLedger) -> StorageResult<()> {
    let path = project_path.join("ledger").join("events.jsonl");
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| ScorestrataError::Io {
            path: parent.display().to_string(),
            source: e,
        })?;
    }
    let mut content = String::new();
    for entry in &ledger.events {
        let line = serde_json::to_string(entry).map_err(|e| ScorestrataError::Serialization {
            message: format!("Failed to serialize ledger entry: {e}"),
        })?;
        content.push_str(&line);
        content.push('\n');
    }
    std::fs::write(&path, content).map_err(|e| ScorestrataError::Io {
        path: path.display().to_string(),
        source: e,
    })?;
    Ok(())
}

/// Filter ledger entries by event type.
#[must_use]
pub fn events_of_type<'a>(entries: &'a [LedgerEntry], event_type: &str) -> Vec<&'a LedgerEntry> {
    entries
        .iter()
        .filter(|e| e.event_type == event_type)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;
    use scorestrata_core::ids::*;

    fn make_entry(event_type: &str) -> LedgerEntry {
        LedgerEntry {
            event_id: EventId::new(),
            command_id: CommandId::new(),
            event_type: event_type.to_string(),
            branch_id: None,
            snapshot_id: None,
            payload: serde_json::json!({"test": true}),
            timestamp: Utc::now(),
        }
    }

    #[test]
    #[expect(clippy::unwrap_used, reason = "Test should panic on unexpected error")]
    fn append_and_read_events() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("events.jsonl");

        let e1 = make_entry("pattern_updated");
        let e2 = make_entry("snapshot_created");
        append_event(&path, &e1).unwrap();
        append_event(&path, &e2).unwrap();

        let entries = read_ledger(&path).unwrap();
        assert_eq!(entries.len(), 2);
        assert_eq!(entries[0].event_type, "pattern_updated");
        assert_eq!(entries[1].event_type, "snapshot_created");
    }

    #[test]
    #[expect(clippy::unwrap_used, reason = "Test should panic on unexpected error")]
    fn read_nonexistent_ledger_returns_empty() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("nonexistent.jsonl");
        let entries = read_ledger(&path).unwrap();
        assert!(entries.is_empty());
    }

    #[test]
    fn events_of_type_filters_correctly() {
        let entries = vec![
            make_entry("pattern_updated"),
            make_entry("snapshot_created"),
            make_entry("pattern_updated"),
        ];
        let filtered = events_of_type(&entries, "pattern_updated");
        assert_eq!(filtered.len(), 2);
    }

    #[test]
    #[expect(clippy::unwrap_used, reason = "Test should panic on unexpected error")]
    fn save_and_load_ledger_roundtrip() {
        let dir = tempfile::tempdir().unwrap();
        let project_id = ProjectId::new();

        let mut ledger = EventLedger::new(project_id);
        ledger.append(make_entry("pattern_updated"));
        ledger.append(make_entry("snapshot_created"));

        save_event_ledger(dir.path(), &ledger).unwrap();
        let loaded = load_event_ledger(dir.path(), project_id).unwrap();
        assert_eq!(loaded.events.len(), 2);
        assert_eq!(loaded.events[0].event_type, "pattern_updated");
    }
}
