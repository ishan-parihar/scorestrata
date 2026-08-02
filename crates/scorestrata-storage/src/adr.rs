//! Architecture decision record (ADR) ledger.
//!
//! The architecture spec §8.2 lists `ADR` as a contract entity and the
//! engineering workflow (§16.3, step 12) requires "Updated architecture doc
//! or ADR" after each feature. This ledger persists the decisions an agent
//! makes so the engineering plane stays auditable and self-documenting.

use serde::{Deserialize, Serialize};

/// Status of an architecture decision record.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AdrStatus {
    /// Decision proposed, not yet accepted.
    Proposed,
    /// Decision accepted and in effect.
    Accepted,
    /// Decision superseded by a later ADR.
    Superseded,
}

/// A single architecture decision record.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AdrRecord {
    /// Unique ADR ID (e.g. `adr_0007`).
    pub id: String,
    /// Short title of the decision.
    pub title: String,
    /// Current status.
    pub status: AdrStatus,
    /// Context motivating the decision.
    pub context: String,
    /// The decision itself.
    pub decision: String,
    /// Consequences (positive and negative).
    pub consequences: Vec<String>,
    /// Spec/architecture section this decision implements.
    pub related_spec: String,
    /// Timestamp.
    pub timestamp: String,
}

/// Ordered ledger of architecture decision records.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct AdrLedger {
    /// All ADRs in recording order.
    pub records: Vec<AdrRecord>,
}

impl AdrLedger {
    /// Create a new empty ledger.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Append a new ADR and return its assigned ID.
    pub fn record(
        &mut self,
        title: &str,
        context: &str,
        decision: &str,
        consequences: Vec<String>,
        related_spec: &str,
    ) -> String {
        let id = format!("adr_{:04}", self.records.len() + 1);
        let record = AdrRecord {
            id: id.clone(),
            title: title.to_string(),
            status: AdrStatus::Accepted,
            context: context.to_string(),
            decision: decision.to_string(),
            consequences,
            related_spec: related_spec.to_string(),
            timestamp: chrono::Utc::now().to_rfc3339(),
        };
        self.records.push(record);
        id
    }

    /// Get an ADR by ID.
    #[must_use]
    pub fn get(&self, id: &str) -> Option<&AdrRecord> {
        self.records.iter().find(|r| r.id == id)
    }

    /// Mark an ADR as superseded by a later decision.
    pub fn supersede(&mut self, id: &str) -> bool {
        let Some(record) = self.records.iter_mut().find(|r| r.id == id) else {
            return false;
        };
        if record.status == AdrStatus::Superseded {
            return false;
        }
        record.status = AdrStatus::Superseded;
        true
    }

    /// All records in recording order.
    #[must_use]
    pub fn all(&self) -> &[AdrRecord] {
        &self.records
    }

    /// Number of records.
    #[must_use]
    pub const fn len(&self) -> usize {
        self.records.len()
    }

    /// Whether the ledger is empty.
    #[must_use]
    pub const fn is_empty(&self) -> bool {
        self.records.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn record_assigns_incrementing_ids() {
        let mut ledger = AdrLedger::new();
        let first = ledger.record("One", "ctx", "decision", vec![], "§10");
        let second = ledger.record("Two", "ctx", "decision", vec![], "§11");
        assert_eq!(first, "adr_0001");
        assert_eq!(second, "adr_0002");
        assert_eq!(ledger.len(), 2);
    }

    #[expect(clippy::expect_used, reason = "Test asserts the record exists")]
    #[test]
    fn supersede_updates_status_once() {
        let mut ledger = AdrLedger::new();
        let id = ledger.record("One", "ctx", "decision", vec![], "§10");
        assert!(ledger.supersede(&id));
        assert!(!ledger.supersede(&id), "already superseded");
        assert_eq!(
            ledger.get(&id).expect("record").status,
            AdrStatus::Superseded
        );
    }
}
