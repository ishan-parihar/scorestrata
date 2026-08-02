//! Evidence warehouse — stores metric samples, diagnostics, and evidence URIs.
//!
//! The architecture spec requires: "Every metric must have: score, severity,
//! scope, evidence URI, explanation, confidence, calibration context,
//! suggested actions, lineage."

use scorestrata_core::diagnostic::{Diagnostic, MetricSample, Scorecard};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

/// An evidence entry — a single metric sample with full context.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EvidenceEntry {
    /// Unique identifier for this evidence.
    pub id: String,
    /// The metric sample this evidence supports.
    pub metric: MetricSample,
    /// Diagnostics that produced this metric.
    pub diagnostics: Vec<Diagnostic>,
    /// Timestamp when this evidence was recorded.
    pub timestamp: String,
    /// The validation level that produced this evidence.
    pub validation_level: u8,
    /// Command ID that produced this metric (for lineage tracking).
    pub command_id: Option<String>,
}

/// Evidence warehouse — stores all metric samples and diagnostics.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct EvidenceWarehouse {
    /// All evidence entries, keyed by metric name.
    pub entries: BTreeMap<String, Vec<EvidenceEntry>>,
    /// Latest scorecard.
    pub latest_scorecard: Option<Scorecard>,
    /// Total evidence count.
    pub total_entries: u32,
}

impl EvidenceWarehouse {
    /// Create a new empty warehouse.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Record a scorecard's metrics as evidence.
    pub fn record_scorecard(
        &mut self,
        scorecard: &Scorecard,
        level: u8,
        command_id: Option<&str>,
        diagnostics: &[Diagnostic],
    ) {
        self.latest_scorecard = Some(scorecard.clone());
        for metric in &scorecard.metrics {
            let entry = EvidenceEntry {
                id: format!("ev_{}", self.total_entries),
                metric: metric.clone(),
                diagnostics: diagnostics.to_vec(),
                command_id: command_id.map(str::to_string),
                timestamp: chrono::Utc::now().to_rfc3339(),
                validation_level: level,
            };
            self.entries
                .entry(metric.name.clone())
                .or_default()
                .push(entry);
            self.total_entries += 1;
        }
    }

    /// Get all evidence for a metric.
    #[must_use]
    pub fn get_evidence_for_metric(&self, metric_name: &str) -> Vec<&EvidenceEntry> {
        self.entries
            .get(metric_name)
            .map_or_else(Vec::new, |v| v.iter().collect())
    }

    /// Get the latest evidence for a metric.
    #[must_use]
    pub fn get_latest_evidence(&self, metric_name: &str) -> Option<&EvidenceEntry> {
        self.entries.get(metric_name)?.last()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use scorestrata_core::diagnostic::MetricScope;
    use scorestrata_core::error::Severity;
    use scorestrata_core::ids::*;

    fn make_metric_sample(name: &str, value: f32) -> MetricSample {
        MetricSample {
            id: MetricId::new(),
            name: name.to_string(),
            value,
            severity: Severity::Warning,
            scope: MetricScope::default(),
            evidence_uri: format!("project://metrics/{name}"),
            explanation: format!("Test metric: {name}"),
            confidence: 0.8,
            suggested_patches: vec![],
        }
    }

    #[test]
    fn warehouse_records_scorecard_metrics() {
        let mut warehouse = EvidenceWarehouse::new();
        let scorecard = Scorecard {
            id: ScorecardId::new(),
            metrics: vec![make_metric_sample("harmony.scale_compliance", 0.85)],
            overall_score: 0.85,
            hard_failures: 0,
            warnings: 1,
        };
        warehouse.record_scorecard(&scorecard, 2, Some("cmd_test"), &[]);
        assert_eq!(warehouse.total_entries, 1);
        assert!(
            warehouse
                .get_latest_evidence("harmony.scale_compliance")
                .is_some()
        );
    }

    #[test]
    fn warehouse_returns_none_for_unknown_metric() {
        let warehouse = EvidenceWarehouse::new();
        assert!(warehouse.get_latest_evidence("unknown.metric").is_none());
    }
}
