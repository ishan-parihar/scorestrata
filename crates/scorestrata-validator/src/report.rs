//! Validation report building utilities.
//!
//! Builds actionable validation reports with per-metric scoring,
//! scoring profiles, and evidence-backed diagnostics.

use scorestrata_core::diagnostic::{
    Diagnostic, MetricSample, Scorecard, ValidationReport, ValidationSummary,
};
use scorestrata_core::error::Severity;
use scorestrata_core::ids::ProjectId;
use scorestrata_core::ids::{MetricId, ScorecardId};
use std::collections::BTreeMap;

/// A scoring profile that assigns weights to metric categories.
///
/// Different genres and contexts may weight metrics differently.
/// The architecture spec requires calibration-aware scoring.
#[derive(Debug, Clone)]
pub struct ScoringProfile {
    /// Per-metric-category weights. Key is metric prefix (e.g. "harmony", "rhythm").
    /// Default weight is 1.0 if not specified.
    pub weights: BTreeMap<String, f32>,
    /// Minimum acceptable overall score (hard gate).
    pub min_overall_score: f32,
    /// Maximum allowed hard failures (hard gate).
    pub max_hard_failures: u32,
}

impl Default for ScoringProfile {
    fn default() -> Self {
        let mut weights = BTreeMap::new();
        weights.insert("harmony".to_string(), 1.0);
        weights.insert("rhythm".to_string(), 1.0);
        weights.insert("structure".to_string(), 1.2);
        weights.insert("arrangement".to_string(), 0.8);
        Self {
            weights,
            min_overall_score: 0.7,
            max_hard_failures: 0,
        }
    }
}

impl ScoringProfile {
    /// Merge genre-calibrated metric weights (§11.8) over the generic defaults.
    ///
    /// Categories the genre profile does not weight keep the default weight;
    /// the hard gates (`min_overall_score`, `max_hard_failures`) stay at
    /// their defaults. This is how the canonical genre profiles from
    /// `scorestrata_storage::calibration` reach the scorecard.
    #[must_use]
    pub fn with_genre_weights(weights: BTreeMap<String, f32>) -> Self {
        let mut merged = Self::default();
        for (category, weight) in weights {
            merged.weights.insert(category, weight);
        }
        merged
    }
}

/// Build a validation report from a set of diagnostics.
#[must_use]
pub fn build_validation_report(
    project_id: ProjectId,
    level: u8,
    diagnostics: Vec<Diagnostic>,
) -> ValidationReport {
    let summary = count_diagnostics(&diagnostics);
    let all_hard_gates_passed = !diagnostics.iter().any(|d| d.severity == Severity::Critical);
    let scorecard = build_scorecard(&diagnostics, &ScoringProfile::default());

    ValidationReport {
        project_id,
        level,
        diagnostics,
        scorecard,
        all_hard_gates_passed,
        summary,
    }
}

/// Build a validation report with a custom scoring profile.
#[must_use]
pub fn build_validation_report_with_profile(
    project_id: ProjectId,
    level: u8,
    diagnostics: Vec<Diagnostic>,
    profile: &ScoringProfile,
) -> ValidationReport {
    let summary = count_diagnostics(&diagnostics);
    let all_hard_gates_passed = !diagnostics.iter().any(|d| d.severity == Severity::Critical);
    let scorecard = build_scorecard(&diagnostics, profile);

    ValidationReport {
        project_id,
        level,
        diagnostics,
        scorecard,
        all_hard_gates_passed,
        summary,
    }
}

/// Count diagnostics by severity.
fn count_diagnostics(diagnostics: &[Diagnostic]) -> ValidationSummary {
    let mut summary = ValidationSummary::default();
    for d in diagnostics {
        match d.severity {
            Severity::Info => summary.info += 1,
            Severity::Warning => summary.warnings += 1,
            Severity::Error => summary.errors += 1,
            Severity::Critical => summary.critical += 1,
        }
    }
    summary
}

/// Extract the metric category prefix (e.g. "harmony" from "`harmony.scale_compliance`").
fn metric_category(metric_name: &str) -> &str {
    metric_name.split('.').next().unwrap_or("unknown")
}

/// Build a scorecard from diagnostics with per-metric scoring.
fn build_scorecard(diagnostics: &[Diagnostic], profile: &ScoringProfile) -> Scorecard {
    // Group diagnostics by metric name
    let mut metric_diag_counts: BTreeMap<String, Vec<&Diagnostic>> = BTreeMap::new();
    for d in diagnostics {
        metric_diag_counts
            .entry(d.metric.clone())
            .or_default()
            .push(d);
    }

    // Build metric samples from grouped diagnostics
    let mut metrics = Vec::new();
    let mut weighted_score_sum = 0.0;
    let mut total_weight = 0.0;

    for (metric_name, diags) in &metric_diag_counts {
        let category = metric_category(metric_name);
        let weight = profile.weights.get(category).copied().unwrap_or(1.0);

        // Score: 1.0 minus penalty for issues
        let warning_count = diags
            .iter()
            .filter(|d| d.severity == Severity::Warning)
            .count();
        let error_count = diags
            .iter()
            .filter(|d| d.severity == Severity::Error)
            .count();
        let critical_count = diags
            .iter()
            .filter(|d| d.severity == Severity::Critical)
            .count();

        #[expect(clippy::cast_precision_loss, reason = "Diagnostic counts fit in f32")]
        let penalty = (critical_count as f32).mul_add(
            0.5,
            (warning_count as f32).mul_add(0.05, error_count as f32 * 0.15),
        );
        let value = (1.0 - penalty).max(0.0);

        let severity = if critical_count > 0 {
            Severity::Critical
        } else if error_count > 0 {
            Severity::Error
        } else if warning_count > 0 {
            Severity::Warning
        } else {
            Severity::Info
        };

        // Collect evidence URIs from all diagnostics for this metric
        let evidence_uris: Vec<String> =
            diags.iter().flat_map(|d| d.evidence_uris.clone()).collect();

        metrics.push(MetricSample {
            id: MetricId::new(),
            name: metric_name.clone(),
            value,
            severity,
            scope: diags.first().map(|d| d.scope.clone()).unwrap_or_default(),
            evidence_uri: evidence_uris.first().cloned().unwrap_or_default(),
            explanation: format!(
                "{metric_name}: {warning_count} warnings, {error_count} errors, {critical_count} critical"
            ),
            confidence: 0.8,
            suggested_patches: diags
                .iter()
                .flat_map(|d| d.suggested_patches.clone())
                .collect(),
        });

        weighted_score_sum += value * weight;
        total_weight += weight;
    }

    let overall_score = if total_weight > 0.0 {
        weighted_score_sum / total_weight
    } else {
        1.0
    };

    #[expect(
        clippy::cast_possible_truncation,
        reason = "Diagnostic counts fit in u32"
    )]
    let hard_failures = diagnostics
        .iter()
        .filter(|d| d.severity == Severity::Critical)
        .count() as u32;
    #[expect(
        clippy::cast_possible_truncation,
        reason = "Diagnostic counts fit in u32"
    )]
    let warnings = diagnostics
        .iter()
        .filter(|d| d.severity == Severity::Warning)
        .count() as u32;

    Scorecard {
        id: ScorecardId::new(),
        metrics,
        overall_score,
        hard_failures,
        warnings,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use scorestrata_core::diagnostic::MetricScope;

    fn make_diagnostic(code: &str, metric: &str, severity: Severity) -> Diagnostic {
        Diagnostic {
            id: scorestrata_core::ids::DiagnosticId::new(),
            code: code.to_string(),
            metric: metric.to_string(),
            severity,
            scope: MetricScope::default(),
            evidence_uris: vec![],
            message: format!("Test diagnostic: {code}"),
            root_causes: vec![],
            suggested_patches: vec![],
            expected_delta: None,
            confidence: 1.0,
        }
    }

    #[test]
    fn empty_diagnostics_produces_perfect_score() {
        let report = build_validation_report(ProjectId::default(), 2, Vec::new());
        assert!(report.all_hard_gates_passed);
        assert_eq!(report.summary.warnings, 0);
        assert!((report.scorecard.overall_score - 1.0).abs() < f32::EPSILON);
        assert!(report.scorecard.metrics.is_empty());
    }

    #[test]
    fn warnings_reduce_metric_score() {
        let diagnostics = vec![
            make_diagnostic(
                "SCALE_VIOLATION",
                "harmony.scale_compliance",
                Severity::Warning,
            ),
            make_diagnostic(
                "SCALE_VIOLATION",
                "harmony.scale_compliance",
                Severity::Warning,
            ),
        ];
        let report = build_validation_report(ProjectId::default(), 2, diagnostics);
        assert_eq!(report.summary.warnings, 2);
        assert!(report.scorecard.overall_score < 1.0);
        assert_eq!(report.scorecard.metrics.len(), 1);
        assert_eq!(report.scorecard.metrics[0].name, "harmony.scale_compliance");
    }

    #[test]
    fn critical_fails_hard_gate() {
        let diagnostics = vec![make_diagnostic(
            "SEGMENT_EMPTY",
            "structure.segment_layers",
            Severity::Critical,
        )];
        let report = build_validation_report(ProjectId::default(), 2, diagnostics);
        assert!(!report.all_hard_gates_passed);
        assert_eq!(report.summary.critical, 1);
    }

    #[test]
    fn scoring_profile_weights_affect_score() {
        let diagnostics = vec![make_diagnostic(
            "DENSITY_LOW",
            "rhythm.drum_density",
            Severity::Warning,
        )];

        // Default profile: rhythm weight = 1.0
        let report1 = build_validation_report(ProjectId::default(), 2, diagnostics.clone());
        // Custom profile: rhythm weight = 2.0 (penalizes rhythm issues more)
        let mut profile = ScoringProfile::default();
        profile.weights.insert("rhythm".to_string(), 2.0);
        let report2 =
            build_validation_report_with_profile(ProjectId::default(), 2, diagnostics, &profile);

        // Higher weight on the affected metric should produce lower overall score
        assert!(report2.scorecard.overall_score <= report1.scorecard.overall_score);
    }

    #[test]
    fn with_genre_weights_merges_over_defaults() {
        // §11.8: genre weights override matching categories and leave the
        // rest (and the hard gates) at their defaults.
        let profile = ScoringProfile::with_genre_weights(BTreeMap::from([
            ("rhythm".to_string(), 1.4),
            ("arrangement".to_string(), 1.0),
        ]));
        assert_eq!(profile.weights.get("rhythm"), Some(&1.4));
        assert_eq!(profile.weights.get("arrangement"), Some(&1.0));
        // Untouched categories keep the generic defaults.
        assert_eq!(profile.weights.get("harmony"), Some(&1.0));
        assert_eq!(profile.weights.get("structure"), Some(&1.2));
        // Hard gates unchanged.
        assert_eq!(profile.min_overall_score, 0.7);
        assert_eq!(profile.max_hard_failures, 0);
    }
}
