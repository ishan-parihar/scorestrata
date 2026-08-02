//! Regression detection — compares two scorecards to detect metric regressions after patches.
//!
//! The architecture spec (section 11.7) requires the validator to detect:
//! - metric regressions after patches,
//! - cross-layer side effects,
//! - mix drift,
//! - arrangement flattening,
//! - motif coherence loss,
//! - loudness target drift.
//!
//! This module implements Level 6 validation by comparing a baseline scorecard
//! against a post-patch scorecard and generating diagnostics for any regressions.

use scorestrata_core::diagnostic::{Diagnostic, MetricScope, Scorecard, SuggestedPatch};
use scorestrata_core::error::Severity;
use scorestrata_core::ids::DiagnosticId;

/// Configuration for regression detection thresholds.
#[derive(Debug, Clone)]
pub struct RegressionConfig {
    /// Minimum absolute delta to consider a metric change significant.
    /// Default: 0.01 (1%).
    pub significance_threshold: f32,
    /// Maximum allowed regression before flagging as critical.
    /// Default: 0.10 (10%).
    pub critical_regression_threshold: f32,
    /// Metrics that must never regress (hard gates).
    pub hard_gate_metrics: Vec<String>,
}

impl Default for RegressionConfig {
    fn default() -> Self {
        Self {
            significance_threshold: 0.01,
            critical_regression_threshold: 0.10,
            hard_gate_metrics: vec![
                "audio.integrated_lufs".to_string(),
                "audio.true_peak_dbtp".to_string(),
                "audio.clipping".to_string(),
            ],
        }
    }
}

/// The result of comparing two scorecards for regressions.
#[derive(Debug, Clone)]
pub struct RegressionReport {
    /// Regressions detected (metric got worse).
    pub regressions: Vec<MetricRegression>,
    /// Improvements detected (metric got better).
    pub improvements: Vec<MetricRegression>,
    /// Metrics that were unchanged.
    pub unchanged: Vec<String>,
    /// New metrics that didn't exist in the baseline.
    pub new_metrics: Vec<String>,
    /// Metrics that disappeared from the baseline.
    pub removed_metrics: Vec<String>,
    /// Whether any hard gate metrics regressed.
    pub hard_gate_regressed: bool,
    /// Overall regression severity.
    pub overall_severity: Severity,
}

/// A single metric regression or improvement.
#[derive(Debug, Clone)]
pub struct MetricRegression {
    /// Metric name.
    pub metric: String,
    /// Baseline value.
    pub baseline_value: f32,
    /// Current value.
    pub current_value: f32,
    /// Delta (current - baseline). Negative means regression.
    pub delta: f32,
    /// Absolute delta.
    pub abs_delta: f32,
    /// Whether this is a hard gate metric.
    pub is_hard_gate: bool,
    /// Severity of this regression.
    pub severity: Severity,
}

/// Compare two scorecards and detect regressions.
///
/// The baseline represents the state before a patch was applied.
/// The current represents the state after a patch was applied.
/// Any metric that decreased beyond the significance threshold is flagged.
#[must_use]
pub fn detect_regressions(
    baseline: &Scorecard,
    current: &Scorecard,
    config: &RegressionConfig,
) -> RegressionReport {
    let mut regressions = Vec::new();
    let mut improvements = Vec::new();
    let mut unchanged = Vec::new();
    let mut new_metrics = Vec::new();
    let mut baseline_metrics: std::collections::BTreeMap<String, f32> =
        std::collections::BTreeMap::new();

    // Index baseline metrics
    for metric in &baseline.metrics {
        baseline_metrics.insert(metric.name.clone(), metric.value);
    }

    // Compare each current metric against baseline
    for metric in &current.metrics {
        let is_hard_gate = config.hard_gate_metrics.contains(&metric.name);

        match baseline_metrics.remove(&metric.name) {
            Some(baseline_value) => {
                let delta = metric.value - baseline_value;
                let abs_delta = delta.abs();

                if abs_delta < config.significance_threshold {
                    unchanged.push(metric.name.clone());
                } else if delta < 0.0 {
                    // Regression
                    let severity =
                        if is_hard_gate || abs_delta >= config.critical_regression_threshold {
                            Severity::Error
                        } else {
                            Severity::Warning
                        };

                    regressions.push(MetricRegression {
                        metric: metric.name.clone(),
                        baseline_value,
                        current_value: metric.value,
                        delta,
                        abs_delta,
                        is_hard_gate,
                        severity,
                    });
                } else {
                    // Improvement
                    improvements.push(MetricRegression {
                        metric: metric.name.clone(),
                        baseline_value,
                        current_value: metric.value,
                        delta,
                        abs_delta,
                        is_hard_gate,
                        severity: Severity::Info,
                    });
                }
            }
            None => {
                // New metric not in baseline
                new_metrics.push(metric.name.clone());
            }
        }
    }

    // Metrics that disappeared from baseline
    let removed_metrics: Vec<String> = baseline_metrics.into_keys().collect();

    let hard_gate_regressed = regressions.iter().any(|r| r.is_hard_gate);
    let overall_severity =
        if hard_gate_regressed || regressions.iter().any(|r| r.severity == Severity::Error) {
            Severity::Error
        } else if regressions.iter().any(|r| r.severity == Severity::Warning) {
            Severity::Warning
        } else {
            Severity::Info
        };

    RegressionReport {
        regressions,
        improvements,
        unchanged,
        new_metrics,
        removed_metrics,
        hard_gate_regressed,
        overall_severity,
    }
}

/// Convert a [`RegressionReport`] into diagnostics that can be added to a validation report.
///
/// Each regression becomes a diagnostic with root causes, evidence URIs,
/// and suggested patches.
#[must_use]
pub fn regression_diagnostics(report: &RegressionReport) -> Vec<Diagnostic> {
    let mut diagnostics = Vec::new();

    for regression in &report.regressions {
        let (root_causes, suggested_patches) = if regression.is_hard_gate {
            (
                vec![
                    "hard gate metric regressed after patch".to_string(),
                    "patch may have introduced audio quality issues".to_string(),
                ],
                vec![SuggestedPatch {
                    op: "rollback_to_previous_snapshot".to_string(),
                    params: serde_json::json!({
                        "metric": regression.metric,
                        "regression": regression.abs_delta,
                    }),
                    expected_delta: Some(format!("+{:.3}", regression.abs_delta)),
                    confidence: 0.9,
                }],
            )
        } else {
            let suggested_op = suggest_fix_for_metric(&regression.metric);
            (
                vec![format!(
                    "{} regressed by {:.3} (was {:.3}, now {:.3})",
                    regression.metric,
                    regression.abs_delta,
                    regression.baseline_value,
                    regression.current_value
                )],
                vec![SuggestedPatch {
                    op: suggested_op,
                    params: serde_json::json!({
                        "metric": regression.metric,
                    }),
                    expected_delta: Some(format!("+{:.3}", regression.abs_delta)),
                    confidence: 0.7,
                }],
            )
        };

        diagnostics.push(Diagnostic {
            id: DiagnosticId::new(),
            code: "METRIC_REGRESSION_DETECTED".to_string(),
            metric: regression.metric.clone(),
            severity: regression.severity,
            scope: MetricScope::default(),
            evidence_uris: vec![format!(
                "project://validation/regression/{}",
                regression.metric
            )],
            message: format!(
                "Metric '{}' regressed from {:.3} to {:.3} (delta: {:+.3})",
                regression.metric,
                regression.baseline_value,
                regression.current_value,
                regression.delta
            ),
            root_causes,
            suggested_patches,
            expected_delta: Some(format!("{:+.3}", regression.delta)),
            confidence: 0.95,
        });
    }

    // If hard gates regressed, add a critical diagnostic
    if report.hard_gate_regressed {
        let hard_gate_names: Vec<&str> = report
            .regressions
            .iter()
            .filter(|r| r.is_hard_gate)
            .map(|r| r.metric.as_str())
            .collect();

        diagnostics.push(Diagnostic {
            id: DiagnosticId::new(),
            code: "HARD_GATE_REGRESSION".to_string(),
            metric: "regression.hard_gate".to_string(),
            severity: Severity::Critical,
            scope: MetricScope::default(),
            evidence_uris: vec!["project://validation/regression/hard_gates".to_string()],
            message: format!(
                "Hard gate metrics regressed: {}. Patch should be rolled back.",
                hard_gate_names.join(", ")
            ),
            root_causes: vec!["patch introduced critical quality regression".to_string()],
            suggested_patches: vec![SuggestedPatch {
                op: "rollback_to_previous_snapshot".to_string(),
                params: serde_json::json!({
                    "hard_gates": hard_gate_names,
                }),
                expected_delta: None,
                confidence: 0.95,
            }],
            expected_delta: None,
            confidence: 0.95,
        });
    }

    diagnostics
}

/// A snapshot of a render's Level-4 signature for regression comparison.
///
/// Captures the deterministic checksum plus the loudness and peak metrics
/// that must not drift between iterations (§11.7 Level 6).
#[derive(Debug, Clone, PartialEq)]
pub struct RenderSignature {
    /// SHA-256 checksum of the WAV bytes.
    pub checksum: String,
    /// Integrated loudness in LUFS.
    pub integrated_lufs: f32,
    /// True peak in dBTP.
    pub true_peak_dbtp: f32,
}

/// Configuration for render-regression thresholds (§11.7).
#[derive(Debug, Clone)]
pub struct RenderRegressionConfig {
    /// Maximum acceptable loudness drift in LUFS before warning.
    /// Default: 1.0 LUFS.
    pub loudness_drift_lufs: f32,
    /// Maximum acceptable true-peak drift in dB before warning.
    /// Default: 0.5 dB.
    pub peak_drift_db: f32,
}

impl Default for RenderRegressionConfig {
    fn default() -> Self {
        Self {
            loudness_drift_lufs: 1.0,
            peak_drift_db: 0.5,
        }
    }
}

/// The result of comparing two render signatures.
#[derive(Debug, Clone)]
pub struct RenderRegression {
    /// Loudness drift in LUFS (current - baseline).
    pub loudness_drift_lufs: f32,
    /// True-peak drift in dB (current - baseline).
    pub peak_drift_db: f32,
    /// Whether the checksum changed between the two renders.
    pub checksum_changed: bool,
    /// Whether loudness drifted beyond the configured tolerance.
    pub loudness_drifted: bool,
    /// Whether the true peak drifted beyond the configured tolerance.
    pub peak_drifted: bool,
}

/// Detect render regression between a baseline and a current render.
///
/// Per §11.7 Level 6: a patch must not silently change the render output
/// (checksum) or drift the mix loudness/peak beyond tolerance — otherwise
/// the patch introduced a cross-section regression.
#[must_use]
pub fn detect_render_regression(
    baseline: &RenderSignature,
    current: &RenderSignature,
    config: &RenderRegressionConfig,
) -> RenderRegression {
    let loudness_drift_lufs = current.integrated_lufs - baseline.integrated_lufs;
    let peak_drift_db = current.true_peak_dbtp - baseline.true_peak_dbtp;
    let checksum_changed = baseline.checksum != current.checksum;
    let loudness_drifted = loudness_drift_lufs.abs() > config.loudness_drift_lufs;
    let peak_drifted = peak_drift_db.abs() > config.peak_drift_db;

    RenderRegression {
        loudness_drift_lufs,
        peak_drift_db,
        checksum_changed,
        loudness_drifted,
        peak_drifted,
    }
}

/// Convert a [`RenderRegression`] into actionable diagnostics.
#[must_use]
pub fn render_regression_diagnostics(regression: &RenderRegression) -> Vec<Diagnostic> {
    let mut diagnostics = Vec::new();

    if regression.loudness_drifted {
        diagnostics.push(Diagnostic {
            id: DiagnosticId::new(),
            code: "RENDER_LOUDNESS_DRIFT".to_string(),
            metric: "audio.integrated_lufs".to_string(),
            severity: Severity::Warning,
            scope: MetricScope::default(),
            evidence_uris: vec!["project://render/regression/loudness".to_string()],
            message: format!(
                "Mix loudness drifted by {:+.2} LUFS across render iterations",
                regression.loudness_drift_lufs
            ),
            root_causes: vec![format!(
                "Mix loudness drifted by {:+.2} LUFS across render iterations",
                regression.loudness_drift_lufs
            )],
            suggested_patches: vec![SuggestedPatch {
                op: "normalize_loudness_target".to_string(),
                params: serde_json::json!({}),
                expected_delta: Some("+0.15".to_string()),
                confidence: 0.85,
            }],
            expected_delta: None,
            confidence: 0.9,
        });
    }

    if regression.peak_drifted {
        diagnostics.push(Diagnostic {
            id: DiagnosticId::new(),
            code: "RENDER_PEAK_DRIFT".to_string(),
            metric: "audio.true_peak_dbtp".to_string(),
            severity: Severity::Warning,
            scope: MetricScope::default(),
            evidence_uris: vec!["project://render/regression/true_peak".to_string()],
            message: format!(
                "True peak drifted by {:+.2} dB across render iterations",
                regression.peak_drift_db
            ),
            root_causes: vec![format!(
                "True peak drifted by {:+.2} dB across render iterations",
                regression.peak_drift_db
            )],
            suggested_patches: vec![SuggestedPatch {
                op: "normalize_loudness_target".to_string(),
                params: serde_json::json!({}),
                expected_delta: Some("+0.1".to_string()),
                confidence: 0.8,
            }],
            expected_delta: None,
            confidence: 0.9,
        });
    }

    if regression.checksum_changed {
        diagnostics.push(Diagnostic {
            id: DiagnosticId::new(),
            code: "RENDER_CHECKSUM_CHANGED".to_string(),
            metric: "render.determinism".to_string(),
            severity: if regression.loudness_drifted || regression.peak_drifted {
                Severity::Warning
            } else {
                Severity::Info
            },
            scope: MetricScope::default(),
            evidence_uris: vec!["project://render/regression/checksum".to_string()],
            message: "Render checksum changed across iterations — mix is not byte-identical"
                .to_string(),
            root_causes: vec![
                "Render checksum changed across iterations — mix is not byte-identical".to_string(),
            ],
            suggested_patches: vec![],
            expected_delta: None,
            confidence: 0.99,
        });
    }

    diagnostics
}

/// Suggest a fix operation based on the metric name.
fn suggest_fix_for_metric(metric: &str) -> String {
    if metric.starts_with("harmony.") {
        "quantize_layer_to_scale".to_string()
    } else if metric.starts_with("rhythm.") {
        "add_drum_fill".to_string()
    } else if metric.starts_with("arrangement.") {
        "add_transition_package".to_string()
    } else if metric.starts_with("audio.") {
        "normalize_loudness_target".to_string()
    } else {
        "apply_patch".to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use scorestrata_core::error::Severity;
    use scorestrata_core::ids::{MetricId, ScorecardId};

    fn make_scorecard(metrics: Vec<(&str, f32)>) -> Scorecard {
        let metric_samples = metrics
            .into_iter()
            .map(|(name, value)| scorestrata_core::diagnostic::MetricSample {
                id: MetricId::new(),
                name: name.to_string(),
                value,
                severity: Severity::Info,
                scope: MetricScope::default(),
                evidence_uri: String::new(),
                explanation: String::new(),
                confidence: 1.0,
                suggested_patches: vec![],
            })
            .collect();
        Scorecard {
            id: ScorecardId::new(),
            overall_score: 0.8,
            metrics: metric_samples,
            hard_failures: 0,
            warnings: 0,
        }
    }

    #[test]
    fn no_regressions_when_metrics_improve() {
        let baseline = make_scorecard(vec![
            ("harmony.scale_compliance", 0.8),
            ("rhythm.density", 0.6),
        ]);
        let current = make_scorecard(vec![
            ("harmony.scale_compliance", 0.9),
            ("rhythm.density", 0.7),
        ]);
        let config = RegressionConfig::default();
        let report = detect_regressions(&baseline, &current, &config);
        assert!(report.regressions.is_empty());
        assert_eq!(report.improvements.len(), 2);
    }

    #[test]
    fn detects_simple_regression() {
        let baseline = make_scorecard(vec![("harmony.scale_compliance", 0.8)]);
        let current = make_scorecard(vec![("harmony.scale_compliance", 0.7)]);
        let config = RegressionConfig::default();
        let report = detect_regressions(&baseline, &current, &config);
        assert_eq!(report.regressions.len(), 1);
        assert!((report.regressions[0].delta - (-0.1)).abs() < f32::EPSILON);
    }

    #[test]
    fn ignores_small_changes_below_threshold() {
        let baseline = make_scorecard(vec![("harmony.scale_compliance", 0.800)]);
        let current = make_scorecard(vec![("harmony.scale_compliance", 0.805)]);
        let config = RegressionConfig::default();
        let report = detect_regressions(&baseline, &current, &config);
        assert!(report.regressions.is_empty());
        assert!(report.improvements.is_empty());
        assert_eq!(report.unchanged.len(), 1);
    }

    #[test]
    fn hard_gate_regression_is_critical() {
        let baseline = make_scorecard(vec![("audio.integrated_lufs", -14.0)]);
        let current = make_scorecard(vec![("audio.integrated_lufs", -16.0)]);
        let config = RegressionConfig::default();
        let report = detect_regressions(&baseline, &current, &config);
        assert!(report.hard_gate_regressed);
        assert_eq!(report.regressions.len(), 1);
        assert_eq!(report.regressions[0].severity, Severity::Error);
    }

    #[test]
    fn mixed_improvements_and_regressions() {
        let baseline = make_scorecard(vec![
            ("harmony.scale_compliance", 0.8),
            ("rhythm.density", 0.7),
            ("arrangement.transition_strength", 0.5),
        ]);
        let current = make_scorecard(vec![
            ("harmony.scale_compliance", 0.9),        // improved
            ("rhythm.density", 0.5),                  // regressed
            ("arrangement.transition_strength", 0.5), // unchanged
        ]);
        let config = RegressionConfig::default();
        let report = detect_regressions(&baseline, &current, &config);
        assert_eq!(report.regressions.len(), 1);
        assert_eq!(report.improvements.len(), 1);
        assert_eq!(report.unchanged.len(), 1);
    }

    #[test]
    fn new_metrics_not_flagged_as_regression() {
        let baseline = make_scorecard(vec![("harmony.scale_compliance", 0.8)]);
        let current = make_scorecard(vec![
            ("harmony.scale_compliance", 0.8),
            ("audio.clipping", 1.0), // new metric
        ]);
        let config = RegressionConfig::default();
        let report = detect_regressions(&baseline, &current, &config);
        assert!(report.regressions.is_empty());
        assert_eq!(report.unchanged.len(), 1); // unchanged: scale_compliance
        assert_eq!(report.new_metrics.len(), 1); // new: audio.clipping
    }

    #[test]
    fn regression_diagnostics_produce_actionable_output() {
        let baseline = make_scorecard(vec![("harmony.scale_compliance", 0.8)]);
        let current = make_scorecard(vec![("harmony.scale_compliance", 0.6)]);
        let config = RegressionConfig::default();
        let report = detect_regressions(&baseline, &current, &config);
        let diagnostics = regression_diagnostics(&report);
        assert_eq!(diagnostics.len(), 1);
        assert_eq!(diagnostics[0].code, "METRIC_REGRESSION_DETECTED");
        assert!(!diagnostics[0].suggested_patches.is_empty());
        assert!(!diagnostics[0].evidence_uris.is_empty());
    }

    #[test]
    fn hard_gate_regression_provides_rollback_patch() {
        let baseline = make_scorecard(vec![("audio.clipping", 1.0)]);
        let current = make_scorecard(vec![("audio.clipping", 0.0)]);
        let config = RegressionConfig::default();
        let report = detect_regressions(&baseline, &current, &config);
        let diagnostics = regression_diagnostics(&report);
        assert_eq!(diagnostics.len(), 2); // one for the metric, one for hard gate
        assert!(diagnostics.iter().any(|d| d.code == "HARD_GATE_REGRESSION"));
        assert!(diagnostics.iter().any(|d| {
            d.suggested_patches
                .iter()
                .any(|p| p.op == "rollback_to_previous_snapshot")
        }));
    }

    #[test]
    fn critical_threshold_triggers_error_severity() {
        let baseline = make_scorecard(vec![("harmony.scale_compliance", 0.8)]);
        let current = make_scorecard(vec![
            ("harmony.scale_compliance", 0.5), // 0.3 regression = critical
        ]);
        let config = RegressionConfig {
            critical_regression_threshold: 0.10,
            ..RegressionConfig::default()
        };
        let report = detect_regressions(&baseline, &current, &config);
        assert_eq!(report.regressions[0].severity, Severity::Error);
    }

    #[test]
    fn identical_renders_have_no_regression() {
        let base = RenderSignature {
            checksum: "abc123".to_string(),
            integrated_lufs: -12.4,
            true_peak_dbtp: -1.8,
        };
        let current = base.clone();
        let config = RenderRegressionConfig::default();
        let regression = detect_render_regression(&base, &current, &config);
        assert!(!regression.checksum_changed);
        assert!(!regression.loudness_drifted);
        assert!(!regression.peak_drifted);
        assert!(render_regression_diagnostics(&regression).is_empty());
    }

    #[test]
    fn loudness_drift_beyond_tolerance_is_detected() {
        let base = RenderSignature {
            checksum: "abc123".to_string(),
            integrated_lufs: -14.0,
            true_peak_dbtp: -3.0,
        };
        let current = RenderSignature {
            checksum: "def456".to_string(),
            integrated_lufs: -9.0, // 5 LUFS louder
            true_peak_dbtp: -3.0,
        };
        let config = RenderRegressionConfig::default();
        let regression = detect_render_regression(&base, &current, &config);
        assert!(regression.loudness_drifted);
        assert!((regression.loudness_drift_lufs - 5.0).abs() < f32::EPSILON);
        let diagnostics = render_regression_diagnostics(&regression);
        assert!(
            diagnostics
                .iter()
                .any(|d| d.code == "RENDER_LOUDNESS_DRIFT"),
            "loudness drift diagnostic present"
        );
        assert!(
            diagnostics
                .iter()
                .any(|d| d.code == "RENDER_CHECKSUM_CHANGED")
        );
    }

    #[test]
    fn peak_drift_within_tolerance_is_not_flagged() {
        let base = RenderSignature {
            checksum: "abc".to_string(),
            integrated_lufs: -14.0,
            true_peak_dbtp: -1.8,
        };
        let current = RenderSignature {
            checksum: "abc".to_string(),
            integrated_lufs: -14.1,
            true_peak_dbtp: -1.9, // 0.1 dB drift < 0.5 tolerance
        };
        let config = RenderRegressionConfig::default();
        let regression = detect_render_regression(&base, &current, &config);
        assert!(!regression.peak_drifted);
        assert!(!regression.loudness_drifted);
    }

    #[test]
    fn peak_drift_beyond_tolerance_is_detected() {
        let base = RenderSignature {
            checksum: "abc".to_string(),
            integrated_lufs: -14.0,
            true_peak_dbtp: -2.0,
        };
        let current = RenderSignature {
            checksum: "abc".to_string(),
            integrated_lufs: -14.0,
            true_peak_dbtp: -0.5, // 1.5 dB drift
        };
        let config = RenderRegressionConfig::default();
        let regression = detect_render_regression(&base, &current, &config);
        assert!(regression.peak_drifted);
        let diagnostics = render_regression_diagnostics(&regression);
        assert!(diagnostics.iter().any(|d| d.code == "RENDER_PEAK_DRIFT"));
    }

    #[test]
    fn checksum_change_alone_is_informational() {
        let base = RenderSignature {
            checksum: "abc".to_string(),
            integrated_lufs: -14.0,
            true_peak_dbtp: -3.0,
        };
        let current = RenderSignature {
            checksum: "def".to_string(),
            integrated_lufs: -14.0,
            true_peak_dbtp: -3.0,
        };
        let config = RenderRegressionConfig::default();
        let regression = detect_render_regression(&base, &current, &config);
        assert!(regression.checksum_changed);
        let diagnostics = render_regression_diagnostics(&regression);
        // Only the informational checksum diagnostic.
        assert_eq!(diagnostics.len(), 1);
        assert_eq!(diagnostics[0].code, "RENDER_CHECKSUM_CHANGED");
        assert_eq!(diagnostics[0].severity, Severity::Info);
    }

    #[test]
    fn suggest_fix_for_metric_returns_appropriate_operation() {
        assert_eq!(
            suggest_fix_for_metric("harmony.scale_compliance"),
            "quantize_layer_to_scale"
        );
        assert_eq!(suggest_fix_for_metric("rhythm.density"), "add_drum_fill");
        assert_eq!(
            suggest_fix_for_metric("arrangement.transition_strength"),
            "add_transition_package"
        );
        assert_eq!(
            suggest_fix_for_metric("audio.loudness"),
            "normalize_loudness_target"
        );
        assert_eq!(suggest_fix_for_metric("unknown.metric"), "apply_patch");
    }
}
