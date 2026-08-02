//! Diagnostic, scorecard, and validation report types.
//!
//! Every metric in Scorestrata is evidence-backed and explainable.
//! Diagnostics point to exact evidence so agents can repair, not guess.

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

use crate::error::Severity;
use crate::ids::{DiagnosticId, LayerId, MetricId, ProjectId, ScorecardId, SegmentId};

/// A single metric sample with full evidence context.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MetricSample {
    /// Unique identifier for this sample.
    pub id: MetricId,
    /// Metric name (e.g. `harmony.scale_compliance`).
    pub name: String,
    /// Metric value (0.0–1.0 for ratios, arbitrary for others).
    pub value: f32,
    /// Severity based on threshold.
    pub severity: Severity,
    /// Scope where this metric applies.
    pub scope: MetricScope,
    /// Evidence URI pointing to source data.
    pub evidence_uri: String,
    /// Human-readable explanation.
    pub explanation: String,
    /// Confidence in this measurement (0.0–1.0).
    #[serde(default = "default_confidence")]
    pub confidence: f32,
    /// Suggested patches to improve this metric.
    #[serde(default)]
    pub suggested_patches: Vec<SuggestedPatch>,
}

const fn default_confidence() -> f32 {
    1.0
}

/// The scope of a metric — what part of the project it measures.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct MetricScope {
    /// Project-level if None.
    pub segment_id: Option<SegmentId>,
    /// Layer-level if None.
    pub layer_id: Option<LayerId>,
    /// Bar range (start, end) if applicable.
    pub bar_range: Option<(u32, u32)>,
}

/// A suggested patch to fix a diagnostic issue.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SuggestedPatch {
    /// Patch operation name (e.g. `quantize_layer_to_scale`).
    pub op: String,
    /// Patch parameters.
    pub params: serde_json::Value,
    /// Expected score delta if this patch is applied.
    pub expected_delta: Option<String>,
    /// Confidence that this patch will help (0.0–1.0).
    pub confidence: f32,
}

/// A delta between two scorecards (what changed).
/// Uses `BTreeMap` for deterministic iteration order.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ScoreDelta {
    /// Per-metric deltas. Key is metric name, value is the delta string (e.g. "+0.14").
    #[serde(default)]
    pub metrics: BTreeMap<String, String>,
}

impl ScoreDelta {
    /// Create an empty delta.
    #[must_use]
    pub const fn empty() -> Self {
        Self {
            metrics: BTreeMap::new(),
        }
    }

    /// Add a metric delta.
    pub fn push(&mut self, metric: impl Into<String>, delta: impl Into<String>) {
        self.metrics.insert(metric.into(), delta.into());
    }
}

/// A scorecard containing multiple metric samples.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Scorecard {
    /// Unique identifier.
    pub id: ScorecardId,
    /// All metric samples in this scorecard.
    pub metrics: Vec<MetricSample>,
    /// Overall score (weighted average of metrics).
    pub overall_score: f32,
    /// Number of hard failures.
    pub hard_failures: u32,
    /// Number of warnings.
    pub warnings: u32,
}

impl Scorecard {
    /// Find a metric by name.
    #[must_use]
    pub fn find_metric(&self, name: &str) -> Option<&MetricSample> {
        self.metrics.iter().find(|m| m.name == name)
    }

    /// Get all metrics with severity >= warning.
    #[must_use]
    pub fn issues(&self) -> Vec<&MetricSample> {
        self.metrics
            .iter()
            .filter(|m| m.severity >= Severity::Warning)
            .collect()
    }
}

/// A diagnostic packet — a structured issue with root causes and fixes.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Diagnostic {
    /// Unique identifier.
    pub id: DiagnosticId,
    /// Diagnostic code (e.g. `HARMONY_SCALE_VIOLATION`).
    pub code: String,
    /// Metric this diagnostic relates to.
    pub metric: String,
    /// Severity.
    pub severity: Severity,
    /// Scope.
    pub scope: MetricScope,
    /// Evidence URIs.
    #[serde(default)]
    pub evidence_uris: Vec<String>,
    /// Human-readable message.
    pub message: String,
    /// Root causes identified.
    #[serde(default)]
    pub root_causes: Vec<String>,
    /// Suggested patches.
    #[serde(default)]
    pub suggested_patches: Vec<SuggestedPatch>,
    /// Expected improvement if patch is applied.
    pub expected_delta: Option<String>,
    /// Confidence in the diagnosis (0.0–1.0).
    pub confidence: f32,
}

/// A validation report containing all diagnostics from a validation run.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ValidationReport {
    /// Project this report is for.
    pub project_id: ProjectId,
    /// Validation level (1=config, 2=symbolic, 3=timeline, 4=audio, 5=brief, 6=regression).
    pub level: u8,
    /// All diagnostics produced.
    pub diagnostics: Vec<Diagnostic>,
    /// Scorecard from this validation.
    pub scorecard: Scorecard,
    /// Whether all hard gates passed.
    pub all_hard_gates_passed: bool,
    /// Total number of diagnostics by severity.
    pub summary: ValidationSummary,
}

/// Summary counts for a validation report.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ValidationSummary {
    /// Number of informational diagnostics.
    pub info: u32,
    /// Number of warning diagnostics.
    pub warnings: u32,
    /// Number of error diagnostics.
    pub errors: u32,
    /// Number of critical diagnostics.
    pub critical: u32,
}

/// An audit report — a high-level summary of project health.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuditReport {
    /// Project ID.
    pub project_id: ProjectId,
    /// Overall health score (0.0–1.0).
    pub health_score: f32,
    /// Top issues (sorted by severity).
    pub top_issues: Vec<Diagnostic>,
    /// Scorecard summary.
    pub scorecard: Scorecard,
    /// Suggested next actions.
    #[serde(default)]
    pub next_actions: Vec<String>,
}

// ── Root-Cause Attribution (§11.4) ─────────────────────────────────

/// A root cause — attributes a metric issue to a specific source.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RootCause {
    /// Cause identifier (e.g. `non_scale_notes_on_strong_beats`).
    pub cause: String,
    /// Severity of this root cause.
    pub severity: Severity,
    /// Scope of the root cause.
    pub scope: MetricScope,
    /// Evidence URI supporting this attribution.
    pub evidence_uri: Option<String>,
    /// Suggested patch to fix this root cause.
    pub suggested_patch: Option<SuggestedPatch>,
}

// ── Calibration Context (§11.5-11.6) ───────────────────────────────

/// Calibration context for a metric — genre-specific interpretation.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CalibrationContext {
    /// Genre this metric is calibrated against.
    pub genre: Option<String>,
    /// Reference profile name.
    pub reference_profile: Option<String>,
    /// Genre-specific target value.
    pub genre_target: Option<f32>,
    /// Genre-specific acceptable range.
    pub genre_range: Option<(f32, f32)>,
    /// Whether the metric passes for this genre.
    pub passes_for_genre: bool,
    /// Deviation from genre target (positive = above target).
    pub deviation_from_target: Option<f32>,
}

// ── Diagnostic Packet (§11.3) ──────────────────────────────────────

/// A full diagnostic packet — the complete issue context for agent repair.
///
/// This extends the basic `Diagnostic` with root-cause attribution,
/// calibration context, cost estimates, and policy implications.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DiagnosticPacket {
    /// Unique identifier.
    pub id: String,
    /// Diagnostic code (e.g. `TRANSITION_ENERGY_DELTA_LOW`).
    pub code: String,
    /// The affected metric.
    pub metric: String,
    /// Severity.
    pub severity: Severity,
    /// Scope.
    pub scope: MetricScope,
    /// Evidence URIs.
    pub evidence_uris: Vec<String>,
    /// Human-readable message.
    pub message: String,
    /// Root causes identified.
    pub root_causes: Vec<RootCause>,
    /// Suggested patches with expected improvements.
    pub suggested_patches: Vec<SuggestedPatch>,
    /// Expected improvement if best patch is applied.
    pub expected_delta: Option<String>,
    /// Confidence in the diagnosis (0.0–1.0).
    pub confidence: f32,
    /// Calibration context for genre-aware interpretation.
    pub calibration: Option<CalibrationContext>,
    /// Estimated cost in CPU seconds to apply the best patch.
    pub estimated_cost_seconds: Option<f32>,
    /// Policy implications (which policies were evaluated).
    pub policy_implications: Vec<String>,
}

impl DiagnosticPacket {
    /// Convert to a basic `Diagnostic` for backward compatibility.
    #[must_use]
    pub fn to_diagnostic(&self) -> Diagnostic {
        Diagnostic {
            id: crate::ids::DiagnosticId::new(),
            code: self.code.clone(),
            metric: self.metric.clone(),
            severity: self.severity,
            scope: self.scope.clone(),
            evidence_uris: self.evidence_uris.clone(),
            message: self.message.clone(),
            root_causes: self.root_causes.iter().map(|rc| rc.cause.clone()).collect(),
            suggested_patches: self.suggested_patches.clone(),
            expected_delta: self.expected_delta.clone(),
            confidence: self.confidence,
        }
    }
}
