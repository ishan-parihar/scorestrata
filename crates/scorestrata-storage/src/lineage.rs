//! Metric lineage — the history of every metric across validation runs (§8.3).
//!
//! §15.1 requires every metric to have "score, severity, scope, evidence URI,
//! explanation, confidence, calibration context, suggested actions, lineage."
//! The [`MetricLineage`] entity is that lineage: an ordered history of each
//! metric's value, the command that changed it, the delta from the previous
//! run, and the direction (improvement or regression).
//!
//! This is what turns evidence into repair: with a trajectory, an agent can
//! see *which patch moved a metric* and by how much — and can ask "which
//! metric regresses most when density increases?" (§9.6) from real data.

use scorestrata_core::diagnostic::{MetricSample, Scorecard};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

/// One sample in a metric's lineage — a single validation-run measurement.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LineageSample {
    /// The metric sample this lineage point captures.
    pub metric: MetricSample,
    /// Delta from the previous sample (positive = improved).
    pub delta: Option<f32>,
    /// Command that produced this sample.
    pub command_id: Option<String>,
    /// Validation level that produced this sample.
    pub validation_level: u8,
    /// Timestamp.
    pub timestamp: String,
}

impl LineageSample {
    /// Whether this sample is a regression (value dropped below the prior run).
    #[must_use]
    pub fn is_regression(&self) -> bool {
        self.delta.is_some_and(|d| d < 0.0)
    }

    /// Whether this sample improved over the prior run.
    #[must_use]
    pub fn is_improvement(&self) -> bool {
        self.delta.is_some_and(|d| d > 0.0)
    }
}

/// Lineage of a single metric — its value trajectory over validation runs.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct MetricLineage {
    /// Metric name (e.g. `harmony.scale_compliance`).
    pub metric_name: String,
    /// Samples in chronological order (oldest first).
    pub samples: Vec<LineageSample>,
}

impl MetricLineage {
    /// Create an empty lineage for a metric.
    #[must_use]
    pub fn new(metric_name: &str) -> Self {
        Self {
            metric_name: metric_name.to_string(),
            samples: Vec::new(),
        }
    }

    /// Record a sample, computing the delta against the previous one.
    pub fn record(&mut self, metric: MetricSample, validation_level: u8, command_id: Option<&str>) {
        let delta = self
            .samples
            .last()
            .map(|prev| metric.value - prev.metric.value);
        let sample = LineageSample {
            metric,
            delta,
            command_id: command_id.map(str::to_string),
            validation_level,
            timestamp: chrono::Utc::now().to_rfc3339(),
        };
        self.samples.push(sample);
    }

    /// Latest sample, if any.
    #[must_use]
    pub fn latest(&self) -> Option<&LineageSample> {
        self.samples.last()
    }

    /// Latest value, if any.
    #[must_use]
    pub fn latest_value(&self) -> Option<f32> {
        self.samples.last().map(|s| s.metric.value)
    }

    /// Total change from the first sample to the latest.
    #[must_use]
    pub fn total_delta(&self) -> Option<f32> {
        let first = self.samples.first()?;
        let last = self.samples.last()?;
        Some(last.metric.value - first.metric.value)
    }

    /// The worst regression in this lineage (largest negative delta).
    #[must_use]
    pub fn worst_regression(&self) -> Option<f32> {
        self.samples
            .iter()
            .filter_map(|s| s.delta)
            .fold(None, |worst: Option<f32>, d| {
                Some(worst.map_or(d, |w| w.min(d)))
            })
            .filter(|d| *d < 0.0)
    }

    /// Count of regressions (samples that dropped below the prior run).
    #[must_use]
    pub fn regression_count(&self) -> usize {
        self.samples.iter().filter(|s| s.is_regression()).count()
    }

    /// Count of improvements (samples that rose above the prior run).
    #[must_use]
    pub fn improvement_count(&self) -> usize {
        self.samples.iter().filter(|s| s.is_improvement()).count()
    }

    /// Total samples recorded.
    #[must_use]
    pub fn len(&self) -> usize {
        self.samples.len()
    }

    /// Whether the lineage is empty.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.samples.is_empty()
    }
}

/// Lineage store — lineages for every metric, keyed by metric name.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct LineageStore {
    /// Lineages by metric name.
    pub lineages: BTreeMap<String, MetricLineage>,
}

impl LineageStore {
    /// Create a new empty lineage store.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Record every metric in a scorecard into its lineage.
    pub fn record_scorecard(
        &mut self,
        scorecard: &Scorecard,
        validation_level: u8,
        command_id: Option<&str>,
    ) {
        for metric in &scorecard.metrics {
            self.lineages
                .entry(metric.name.clone())
                .or_insert_with(|| MetricLineage::new(&metric.name))
                .record(metric.clone(), validation_level, command_id);
        }
    }

    /// Get the lineage for a metric.
    #[must_use]
    pub fn get(&self, metric_name: &str) -> Option<&MetricLineage> {
        self.lineages.get(metric_name)
    }

    /// Which metrics regressed in their latest sample, worst-first (§9.6
    /// "which metric regresses most").
    #[must_use]
    pub fn latest_regressions(&self) -> Vec<(&MetricLineage, f32)> {
        let mut regressions: Vec<(&MetricLineage, f32)> = self
            .lineages
            .values()
            .filter_map(|lineage| {
                let latest = lineage.latest()?;
                let delta = latest.delta?;
                (delta < 0.0).then_some((lineage, delta))
            })
            .collect();
        // `total_cmp` is fully deterministic even on NaN deltas (NaNs sort
        // last), so "which metric regresses most" never silently reorders.
        regressions.sort_by(|a, b| a.1.total_cmp(&b.1));
        regressions
    }

    /// All metric names with recorded lineage.
    #[must_use]
    pub fn metric_names(&self) -> Vec<&str> {
        self.lineages.keys().map(String::as_str).collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use scorestrata_core::diagnostic::MetricScope;
    use scorestrata_core::error::Severity;
    use scorestrata_core::ids::{MetricId, ScorecardId};

    fn metric(name: &str, value: f32) -> MetricSample {
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

    fn scorecard(metrics: Vec<MetricSample>) -> Scorecard {
        let overall = metrics.iter().map(|m| m.value).sum::<f32>() / metrics.len() as f32;
        Scorecard {
            id: ScorecardId::new(),
            metrics,
            overall_score: overall,
            hard_failures: 0,
            warnings: 0,
        }
    }

    #[test]
    fn lineage_tracks_deltas_and_trajectory() {
        let mut lineage = MetricLineage::new("harmony.scale_compliance");
        lineage.record(metric("harmony.scale_compliance", 0.50), 2, Some("cmd_1"));
        lineage.record(metric("harmony.scale_compliance", 0.80), 2, Some("cmd_2"));
        lineage.record(metric("harmony.scale_compliance", 0.70), 2, Some("cmd_3"));
        assert_eq!(lineage.len(), 3);
        assert!((lineage.samples[1].delta.unwrap() - 0.30).abs() < f32::EPSILON); // improved
        assert!((lineage.samples[2].delta.unwrap() - (-0.10)).abs() < f32::EPSILON); // regressed
        assert!((lineage.total_delta().unwrap() - 0.20).abs() < f32::EPSILON);
        assert_eq!(lineage.latest_value(), Some(0.70));
    }

    #[test]
    fn lineage_classifies_regression_and_improvement() {
        let mut lineage = MetricLineage::new("mix.masking");
        lineage.record(metric("mix.masking", 0.60), 2, Some("cmd_1"));
        lineage.record(metric("mix.masking", 0.90), 2, Some("cmd_2"));
        lineage.record(metric("mix.masking", 0.85), 2, Some("cmd_3"));
        assert_eq!(lineage.regression_count(), 1);
        assert_eq!(lineage.improvement_count(), 1);
        assert!(lineage.worst_regression().is_some());
        assert!((lineage.worst_regression().unwrap() - (-0.05)).abs() < f32::EPSILON);
        assert!(lineage.samples[1].is_improvement());
        assert!(lineage.samples[2].is_regression());
    }

    #[test]
    fn store_records_scorecards_and_ranks_regressions() {
        let mut store = LineageStore::new();
        store.record_scorecard(
            &scorecard(vec![metric("a.metric", 0.80), metric("b.metric", 0.50)]),
            2,
            Some("run_1"),
        );
        store.record_scorecard(
            &scorecard(vec![metric("a.metric", 0.85), metric("b.metric", 0.30)]),
            2,
            Some("run_2"),
        );
        assert_eq!(store.metric_names().len(), 2);
        let regressions = store.latest_regressions();
        assert_eq!(regressions.len(), 1);
        assert_eq!(regressions[0].0.metric_name, "b.metric");
        assert!((regressions[0].1 - (-0.20)).abs() < f32::EPSILON);
    }

    #[test]
    fn store_returns_none_for_unknown_metric() {
        let store = LineageStore::new();
        assert!(store.get("unknown.metric").is_none());
    }

    #[test]
    fn lineage_round_trips_through_json() {
        let mut lineage = MetricLineage::new("audio.true_peak");
        lineage.record(metric("audio.true_peak", -1.0), 4, Some("cmd_1"));
        #[expect(clippy::unwrap_used, reason = "Test assertions")]
        let json = serde_json::to_string(&lineage).unwrap();
        #[expect(clippy::unwrap_used, reason = "Test assertions")]
        let back: MetricLineage = serde_json::from_str(&json).unwrap();
        assert_eq!(back.len(), 1);
        assert_eq!(back.samples[0].delta, None); // first sample has no prior
    }
}
