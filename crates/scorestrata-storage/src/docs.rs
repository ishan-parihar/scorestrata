//! Documentation update ledger.
//!
//! The architecture engineering workflow requires the agent to keep docs in
//! sync with code (architecture doc, schema docs, README). This ledger
//! records every documentation mutation so the engineering plane can audit
//! whether docs stayed current with the code changes they accompanied.

use serde::{Deserialize, Serialize};

/// A single documentation update.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DocUpdate {
    /// Document path being updated (e.g. `docs/architecture/architecture.md`).
    pub path: String,
    /// Section within the document.
    pub section: String,
    /// Summary of what changed.
    pub summary: String,
    /// Feature/spec this update accompanies.
    pub related_spec: String,
    /// Timestamp.
    pub timestamp: String,
}

/// Ledger of documentation updates.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct DocsLedger {
    /// All doc updates in recording order.
    pub updates: Vec<DocUpdate>,
}

impl DocsLedger {
    /// Create a new empty ledger.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Record a documentation update.
    pub fn record(&mut self, path: &str, section: &str, summary: &str, related_spec: &str) {
        self.updates.push(DocUpdate {
            path: path.to_string(),
            section: section.to_string(),
            summary: summary.to_string(),
            related_spec: related_spec.to_string(),
            timestamp: chrono::Utc::now().to_rfc3339(),
        });
    }

    /// All updates for a given document path.
    #[must_use]
    pub fn updates_for_path(&self, path: &str) -> Vec<&DocUpdate> {
        self.updates.iter().filter(|u| u.path == path).collect()
    }

    /// All updates, newest first.
    #[must_use]
    pub fn recent(&self) -> Vec<&DocUpdate> {
        let mut sorted: Vec<&DocUpdate> = self.updates.iter().collect();
        sorted.sort_by(|a, b| b.timestamp.cmp(&a.timestamp));
        sorted
    }

    /// Number of recorded updates.
    #[must_use]
    pub const fn len(&self) -> usize {
        self.updates.len()
    }

    /// Whether the ledger is empty.
    #[must_use]
    pub const fn is_empty(&self) -> bool {
        self.updates.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn records_and_filters_by_path() {
        let mut ledger = DocsLedger::new();
        ledger.record(
            "docs/architecture/architecture.md",
            "§10.5",
            "added tool",
            "SPEC-X",
        );
        ledger.record("README.md", "install", "updated instructions", "SPEC-Y");
        let arch = ledger.updates_for_path("docs/architecture/architecture.md");
        assert_eq!(arch.len(), 1);
        assert_eq!(arch[0].section, "§10.5");
        assert_eq!(ledger.len(), 2);
    }

    #[test]
    fn recent_returns_newest_first() {
        let mut ledger = DocsLedger::new();
        ledger.record("README.md", "a", "first", "SPEC-X");
        ledger.record("README.md", "b", "second", "SPEC-Y");
        let recent = ledger.recent();
        assert_eq!(recent.len(), 2);
        assert_eq!(recent[0].section, "b");
        assert_eq!(recent[1].section, "a");
    }
}
