//! Golden test runner — validates projects against expected outputs.
//!
//! The architecture spec (section 16.5) requires golden tests to detect
//! regressions in compiler, validator, audio engine, export, and scoring.
//!
//! This module loads golden project configs, runs the validation pipeline,
//! and compares actual outputs against expected checksums/values.

use serde::{Deserialize, Serialize};
use std::path::Path;

use scorestrata_core::error::{ScorestrataError, Severity};

/// A golden test project configuration loaded from disk.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GoldenConfig {
    /// Project name.
    pub name: String,
    /// Description.
    pub description: String,
    /// Expected segment count.
    pub expected_outputs: ExpectedOutputs,
    /// Tolerance for numeric comparisons.
    pub tolerance: f32,
    /// Validation gates (§16.5): diagnostic budgets and audio limits.
    /// Mirrors the `validation_gates` block of the on-disk golden config.
    #[serde(default)]
    pub validation_gates: ValidationGates,
    /// Per-metric scorecard ranges enforced by the harness.
    /// Mirrors the `expected_metrics` block of the on-disk golden config.
    #[serde(default)]
    pub expected_metrics: std::collections::BTreeMap<String, MetricRange>,
}

/// Validation gates enforced by the golden harness (§16.5 validator and
/// audio planes) — the checks the structural assertions cannot see.
///
/// Every field has its own `#[serde(default)]` so a *partial* gates object
/// in a config degrades gracefully instead of failing to parse.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ValidationGates {
    /// Maximum error + critical diagnostics the golden may produce.
    #[serde(default)]
    pub max_errors: u32,
    /// Maximum warning diagnostics the golden may produce.
    #[serde(default)]
    pub max_warnings: u32,
    /// Maximum true peak in dBTP (0.0 = must not clip).
    #[serde(default)]
    pub max_true_peak_dbtp: f32,
    /// Minimum integrated loudness in LUFS.
    #[serde(default = "default_min_lufs")]
    pub min_lufs: f32,
    /// Maximum integrated loudness in LUFS.
    #[serde(default = "default_max_lufs")]
    pub max_lufs: f32,
}

const fn default_min_lufs() -> f32 {
    -24.0
}

const fn default_max_lufs() -> f32 {
    -1.0
}

impl Default for ValidationGates {
    fn default() -> Self {
        Self {
            max_errors: 0,
            max_warnings: 0,
            max_true_peak_dbtp: 0.0,
            min_lufs: default_min_lufs(),
            max_lufs: default_max_lufs(),
        }
    }
}

impl ValidationGates {
    /// Whether any audio gate deviates from the strict defaults — used to
    /// detect when the harness expects audio enforcement.
    #[must_use]
    pub fn declares_audio_gates(&self) -> bool {
        let defaults = Self::default();
        self.max_true_peak_dbtp != defaults.max_true_peak_dbtp
            || self.min_lufs != defaults.min_lufs
            || self.max_lufs != defaults.max_lufs
    }
}

/// A numeric range a scorecard metric must fall within.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MetricRange {
    /// Inclusive minimum.
    pub min: f32,
    /// Inclusive maximum.
    pub max: f32,
}

/// Expected outputs from a golden test.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExpectedOutputs {
    /// Expected number of segments.
    pub segment_count: u32,
    /// Expected total bars.
    pub total_bars: u32,
    /// Whether a chorus segment should exist.
    pub has_chorus: bool,
    /// Whether a breakdown segment should exist.
    pub has_breakdown: bool,
    /// Whether genre matches brief.
    pub genre_matches_brief: bool,
    /// Whether mood matches brief.
    pub mood_matches_brief: bool,
    /// Whether duration matches config.
    pub duration_matches_config: bool,
}

/// Result of a single golden test assertion.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GoldenAssertion {
    /// Assertion name.
    pub name: String,
    /// Whether it passed.
    pub passed: bool,
    /// Expected value (stringified).
    pub expected: String,
    /// Actual value (stringified).
    pub actual: String,
    /// Error message if failed.
    pub error: Option<String>,
}

/// Result of a golden test run.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GoldenRunResult {
    /// Project name.
    pub project_name: String,
    /// All assertions.
    pub assertions: Vec<GoldenAssertion>,
    /// Overall pass/fail.
    pub passed: bool,
    /// Duration in milliseconds.
    pub duration_ms: u64,
}

/// Run validation-plane golden assertions against a real validation run.
///
/// Enforces the §16.5 validator, scoring, and audio gates that the
/// structural assertions cannot see: diagnostic severity budgets, scorecard
/// metric ranges, and render loudness/peak limits.
#[must_use]
pub fn run_validation_golden_assertions(
    config: &GoldenConfig,
    diagnostics: &[scorestrata_core::diagnostic::Diagnostic],
    scorecard: &scorestrata_core::diagnostic::Scorecard,
    true_peak_dbtp: Option<f32>,
    integrated_lufs: Option<f32>,
) -> GoldenRunResult {
    let start = std::time::Instant::now();
    let gates = &config.validation_gates;
    let tolerance = config.tolerance;
    let mut assertions = Vec::new();

    // Validation plane: diagnostic severity budgets.
    let errors_total = diagnostics
        .iter()
        .filter(|d| matches!(d.severity, Severity::Error | Severity::Critical))
        .count() as u32;
    let warnings = diagnostics
        .iter()
        .filter(|d| d.severity == Severity::Warning)
        .count() as u32;

    assertions.push(GoldenAssertion {
        name: "validation_errors".to_string(),
        passed: errors_total <= gates.max_errors,
        expected: format!("<= {}", gates.max_errors),
        actual: errors_total.to_string(),
        error: if errors_total <= gates.max_errors {
            None
        } else {
            Some(format!(
                "Golden produced {errors_total} error/critical diagnostics, budget is {}",
                gates.max_errors
            ))
        },
    });

    assertions.push(GoldenAssertion {
        name: "validation_warnings".to_string(),
        passed: warnings <= gates.max_warnings,
        expected: format!("<= {}", gates.max_warnings),
        actual: warnings.to_string(),
        error: if warnings <= gates.max_warnings {
            None
        } else {
            Some(format!(
                "Golden produced {warnings} warnings, budget is {}",
                gates.max_warnings
            ))
        },
    });

    // Scoring plane: per-metric scorecard ranges.
    for (metric_name, range) in &config.expected_metrics {
        let passed = scorecard
            .find_metric(metric_name)
            .is_some_and(|m| m.value >= range.min - tolerance && m.value <= range.max + tolerance);
        assertions.push(GoldenAssertion {
            name: format!("metric_{metric_name}"),
            passed,
            expected: format!("[{}, {}]", range.min, range.max),
            actual: scorecard
                .find_metric(metric_name)
                .map(|m| format!("{:.3}", m.value))
                .unwrap_or_else(|| "missing".to_string()),
            error: if passed {
                None
            } else {
                Some(format!(
                    "Scorecard metric '{metric_name}' outside [{}, {}]",
                    range.min, range.max
                ))
            },
        });
    }

    // Audio plane: loudness and true-peak gates. When the config declares
    // non-default audio limits but no analysis is supplied, the harness must
    // surface that instead of silently passing.
    if gates.declares_audio_gates() && true_peak_dbtp.is_none() && integrated_lufs.is_none() {
        assertions.push(GoldenAssertion {
            name: "audio_analysis_required".to_string(),
            passed: false,
            expected: "render analysis".to_string(),
            actual: "none supplied".to_string(),
            error: Some(
                "Config declares audio gates but no render analysis was supplied".to_string(),
            ),
        });
    }
    if let Some(peak) = true_peak_dbtp {
        let passed = peak <= gates.max_true_peak_dbtp + tolerance;
        assertions.push(GoldenAssertion {
            name: "audio_true_peak_dbtp".to_string(),
            passed,
            expected: format!("<= {:.2}", gates.max_true_peak_dbtp),
            actual: format!("{peak:.2}"),
            error: if passed {
                None
            } else {
                Some(format!(
                    "True peak {peak:.2} dBTP exceeds gate {:.2}",
                    gates.max_true_peak_dbtp
                ))
            },
        });
    }
    if let Some(lufs) = integrated_lufs {
        let passed = lufs >= gates.min_lufs - tolerance && lufs <= gates.max_lufs + tolerance;
        assertions.push(GoldenAssertion {
            name: "audio_integrated_lufs".to_string(),
            passed,
            expected: format!("[{}, {}]", gates.min_lufs, gates.max_lufs),
            actual: format!("{lufs:.2}"),
            error: if passed {
                None
            } else {
                Some(format!(
                    "Integrated loudness {lufs:.2} LUFS outside [{}, {}]",
                    gates.min_lufs, gates.max_lufs
                ))
            },
        });
    }

    let passed = assertions.iter().all(|a| a.passed);

    GoldenRunResult {
        project_name: config.name.clone(),
        assertions,
        passed,
        duration_ms: start.elapsed().as_millis() as u64,
    }
}

/// Load a golden config from a JSON file.
///
/// # Errors
///
/// Returns [`ScorestrataError::Io`] if the file cannot be read,
/// or [`ScorestrataError::Serialization`] if the JSON is invalid.
pub fn load_golden_config(path: &Path) -> Result<GoldenConfig, ScorestrataError> {
    let content = std::fs::read_to_string(path).map_err(|e| ScorestrataError::Io {
        path: path.display().to_string(),
        source: e,
    })?;
    serde_json::from_str(&content).map_err(|e| ScorestrataError::Serialization {
        message: format!("Failed to parse golden config: {e}"),
    })
}

/// Run golden assertions against a project summary.
///
/// Compares the actual project state against the expected outputs
/// defined in the golden config.
#[must_use]
pub fn run_golden_assertions(
    config: &GoldenConfig,
    actual_segments: u32,
    actual_total_bars: u32,
    actual_has_chorus: bool,
    actual_has_breakdown: bool,
    actual_genre_matches: bool,
    actual_mood_matches: bool,
    actual_duration_matches: bool,
) -> GoldenRunResult {
    let start = std::time::Instant::now();
    let mut assertions = Vec::new();

    // Segment count
    assertions.push(GoldenAssertion {
        name: "segment_count".to_string(),
        passed: actual_segments == config.expected_outputs.segment_count,
        expected: config.expected_outputs.segment_count.to_string(),
        actual: actual_segments.to_string(),
        error: if actual_segments == config.expected_outputs.segment_count {
            None
        } else {
            Some(format!(
                "Expected {} segments, got {}",
                config.expected_outputs.segment_count, actual_segments
            ))
        },
    });

    // Total bars
    assertions.push(GoldenAssertion {
        name: "total_bars".to_string(),
        passed: actual_total_bars == config.expected_outputs.total_bars,
        expected: config.expected_outputs.total_bars.to_string(),
        actual: actual_total_bars.to_string(),
        error: if actual_total_bars == config.expected_outputs.total_bars {
            None
        } else {
            Some(format!(
                "Expected {} bars, got {}",
                config.expected_outputs.total_bars, actual_total_bars
            ))
        },
    });

    // Has chorus
    assertions.push(GoldenAssertion {
        name: "has_chorus".to_string(),
        passed: actual_has_chorus == config.expected_outputs.has_chorus,
        expected: config.expected_outputs.has_chorus.to_string(),
        actual: actual_has_chorus.to_string(),
        error: if actual_has_chorus == config.expected_outputs.has_chorus {
            None
        } else {
            Some(format!(
                "Expected has_chorus={}, got {}",
                config.expected_outputs.has_chorus, actual_has_chorus
            ))
        },
    });

    // Has breakdown
    assertions.push(GoldenAssertion {
        name: "has_breakdown".to_string(),
        passed: actual_has_breakdown == config.expected_outputs.has_breakdown,
        expected: config.expected_outputs.has_breakdown.to_string(),
        actual: actual_has_breakdown.to_string(),
        error: if actual_has_breakdown == config.expected_outputs.has_breakdown {
            None
        } else {
            Some(format!(
                "Expected has_breakdown={}, got {}",
                config.expected_outputs.has_breakdown, actual_has_breakdown
            ))
        },
    });

    // Genre matches
    assertions.push(GoldenAssertion {
        name: "genre_matches_brief".to_string(),
        passed: actual_genre_matches == config.expected_outputs.genre_matches_brief,
        expected: config.expected_outputs.genre_matches_brief.to_string(),
        actual: actual_genre_matches.to_string(),
        error: if actual_genre_matches == config.expected_outputs.genre_matches_brief {
            None
        } else {
            Some("Genre mismatch between brief and config".to_string())
        },
    });

    // Mood matches
    assertions.push(GoldenAssertion {
        name: "mood_matches_brief".to_string(),
        passed: actual_mood_matches == config.expected_outputs.mood_matches_brief,
        expected: config.expected_outputs.mood_matches_brief.to_string(),
        actual: actual_mood_matches.to_string(),
        error: if actual_mood_matches == config.expected_outputs.mood_matches_brief {
            None
        } else {
            Some("Mood mismatch between brief and config".to_string())
        },
    });

    // Duration matches
    assertions.push(GoldenAssertion {
        name: "duration_matches_config".to_string(),
        passed: actual_duration_matches == config.expected_outputs.duration_matches_config,
        expected: config.expected_outputs.duration_matches_config.to_string(),
        actual: actual_duration_matches.to_string(),
        error: if actual_duration_matches == config.expected_outputs.duration_matches_config {
            None
        } else {
            Some("Duration mismatch between config and arrangement".to_string())
        },
    });

    let passed = assertions.iter().all(|a| a.passed);

    GoldenRunResult {
        project_name: config.name.clone(),
        assertions,
        passed,
        duration_ms: start.elapsed().as_millis() as u64,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn load_golden_config_from_file() {
        // Repo-root eval/golden/... (CARGO_MANIFEST_DIR is crates/scorestrata-eval).
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../eval/golden/melodic_techno_64_bar/config.json");
        assert!(path.exists(), "golden config missing at {path:?}");
        let config = load_golden_config(&path).unwrap();
        assert_eq!(config.name, "melodic_techno_64_bar");
        assert_eq!(config.expected_outputs.segment_count, 4);
        assert_eq!(config.expected_outputs.total_bars, 64);
        assert!(config.expected_outputs.has_chorus);
        assert!(!config.expected_outputs.has_breakdown);
        // The §16.5 validation gates and metric ranges must be honored.
        // The fixed audio engine (limiter ceiling −1.0 dBTP) no longer clips
        // or exceeds the peak gate, so the golden expects zero errors and a
        // strict peak ceiling instead of the old lenient 2.0 dBTP budget.
        assert_eq!(config.validation_gates.max_errors, 0);
        assert_eq!(config.validation_gates.max_warnings, 3);
        assert_eq!(config.validation_gates.max_true_peak_dbtp, -0.5);
        assert_eq!(config.validation_gates.min_lufs, -16.0);
        assert_eq!(config.validation_gates.max_lufs, -4.0);
        assert!(
            config
                .expected_metrics
                .contains_key("arrangement.transition_strength")
        );
        assert!(
            config
                .expected_metrics
                .contains_key("audio.stereo_correlation")
        );
    }

    #[test]
    fn golden_assertions_pass_when_matching() {
        let config = GoldenConfig {
            name: "test".to_string(),
            description: "test".to_string(),
            expected_outputs: ExpectedOutputs {
                segment_count: 5,
                total_bars: 64,
                has_chorus: true,
                has_breakdown: true,
                genre_matches_brief: true,
                mood_matches_brief: true,
                duration_matches_config: true,
            },
            tolerance: 0.01,
            validation_gates: ValidationGates::default(),
            expected_metrics: std::collections::BTreeMap::new(),
        };
        let result = run_golden_assertions(&config, 5, 64, true, true, true, true, true);
        assert!(result.passed);
        assert!(result.assertions.iter().all(|a| a.passed));
    }

    #[test]
    fn golden_assertions_fail_when_mismatched() {
        let config = GoldenConfig {
            name: "test".to_string(),
            description: "test".to_string(),
            expected_outputs: ExpectedOutputs {
                segment_count: 5,
                total_bars: 64,
                has_chorus: true,
                has_breakdown: true,
                genre_matches_brief: true,
                mood_matches_brief: true,
                duration_matches_config: true,
            },
            tolerance: 0.01,
            validation_gates: ValidationGates::default(),
            expected_metrics: std::collections::BTreeMap::new(),
        };
        let result = run_golden_assertions(&config, 3, 32, false, false, false, false, false);
        assert!(!result.passed);
        assert!(result.assertions.iter().any(|a| !a.passed));
    }

    #[test]
    fn golden_assertions_produce_error_messages() {
        let config = GoldenConfig {
            name: "test".to_string(),
            description: "test".to_string(),
            expected_outputs: ExpectedOutputs {
                segment_count: 5,
                total_bars: 64,
                has_chorus: true,
                has_breakdown: false,
                genre_matches_brief: true,
                mood_matches_brief: true,
                duration_matches_config: true,
            },
            tolerance: 0.01,
            validation_gates: ValidationGates::default(),
            expected_metrics: std::collections::BTreeMap::new(),
        };
        let result = run_golden_assertions(&config, 3, 32, false, true, false, false, false);
        let failed: Vec<&GoldenAssertion> =
            result.assertions.iter().filter(|a| !a.passed).collect();
        assert!(!failed.is_empty());
        for assertion in &failed {
            assert!(assertion.error.is_some());
        }
    }

    fn config_with_gates() -> GoldenConfig {
        GoldenConfig {
            name: "gates".to_string(),
            description: "validation gates".to_string(),
            expected_outputs: ExpectedOutputs {
                segment_count: 5,
                total_bars: 64,
                has_chorus: true,
                has_breakdown: true,
                genre_matches_brief: true,
                mood_matches_brief: true,
                duration_matches_config: true,
            },
            tolerance: 0.01,
            validation_gates: ValidationGates {
                max_errors: 1,
                max_warnings: 1,
                max_true_peak_dbtp: -1.0,
                min_lufs: -16.0,
                max_lufs: -10.0,
            },
            expected_metrics: std::collections::BTreeMap::from([(
                "harmony.scale_compliance".to_string(),
                MetricRange {
                    min: 0.85,
                    max: 1.0,
                },
            )]),
        }
    }

    fn diagnostic(severity: Severity, metric: &str) -> scorestrata_core::diagnostic::Diagnostic {
        scorestrata_core::diagnostic::Diagnostic {
            id: scorestrata_core::ids::DiagnosticId::new(),
            code: "TEST".to_string(),
            metric: metric.to_string(),
            severity,
            scope: scorestrata_core::diagnostic::MetricScope::default(),
            evidence_uris: vec![],
            message: "test".to_string(),
            root_causes: vec![],
            suggested_patches: vec![],
            expected_delta: None,
            confidence: 1.0,
        }
    }

    fn scorecard_with(metric: &str, value: f32) -> scorestrata_core::diagnostic::Scorecard {
        use scorestrata_core::diagnostic::{MetricSample, MetricScope, Scorecard};
        use scorestrata_core::ids::{MetricId, ScorecardId};
        Scorecard {
            id: ScorecardId::new(),
            metrics: vec![MetricSample {
                id: MetricId::new(),
                name: metric.to_string(),
                value,
                severity: Severity::Info,
                scope: MetricScope::default(),
                evidence_uri: String::new(),
                explanation: String::new(),
                confidence: 1.0,
                suggested_patches: vec![],
            }],
            overall_score: value,
            hard_failures: 0,
            warnings: 0,
        }
    }

    #[test]
    fn validation_golden_passes_within_gates() {
        let config = config_with_gates();
        let diags = vec![diagnostic(Severity::Warning, "audio.stereo_correlation")];
        let result = run_validation_golden_assertions(
            &config,
            &diags,
            &scorecard_with("harmony.scale_compliance", 0.9),
            Some(-1.5),
            Some(-12.0),
        );
        assert!(
            result.passed,
            "gates within budget must pass: {:?}",
            result
                .assertions
                .iter()
                .map(|a| (a.name.as_str(), a.passed))
        );
    }

    #[test]
    fn validation_golden_fails_on_error_budget() {
        let config = config_with_gates();
        let diags = vec![
            diagnostic(Severity::Error, "audio.clipping"),
            diagnostic(Severity::Critical, "regression.hard_gate"),
        ];
        let result = run_validation_golden_assertions(
            &config,
            &diags,
            &scorecard_with("harmony.scale_compliance", 0.9),
            Some(-1.5),
            Some(-12.0),
        );
        assert!(!result.passed);
        assert!(
            result
                .assertions
                .iter()
                .any(|a| a.name == "validation_errors" && !a.passed),
            "error budget violation must be reported"
        );
    }

    #[test]
    fn validation_golden_enforces_metric_ranges() {
        let config = config_with_gates();
        // 0.5 is below the 0.85 floor for harmony.scale_compliance.
        let result = run_validation_golden_assertions(
            &config,
            &[],
            &scorecard_with("harmony.scale_compliance", 0.5),
            Some(-1.5),
            Some(-12.0),
        );
        assert!(
            result
                .assertions
                .iter()
                .any(|a| a.name == "metric_harmony.scale_compliance" && !a.passed)
        );
    }

    #[test]
    fn validation_golden_requires_audio_when_gates_declared() {
        // The config declares audio gates but no render analysis was supplied
        // — the harness must surface that instead of silently passing.
        let config = config_with_gates();
        let result = run_validation_golden_assertions(
            &config,
            &[],
            &scorecard_with("harmony.scale_compliance", 0.9),
            None,
            None,
        );
        assert!(!result.passed);
        assert!(
            result
                .assertions
                .iter()
                .any(|a| a.name == "audio_analysis_required" && !a.passed)
        );
    }

    #[test]
    fn validation_golden_enforces_audio_gates() {
        let config = config_with_gates();
        // True peak above -1.0 and LUFS above -10.0 both violate gates.
        let result = run_validation_golden_assertions(
            &config,
            &[],
            &scorecard_with("harmony.scale_compliance", 0.9),
            Some(1.5),
            Some(-6.0),
        );
        assert!(
            result
                .assertions
                .iter()
                .any(|a| a.name == "audio_true_peak_dbtp" && !a.passed)
        );
        assert!(
            result
                .assertions
                .iter()
                .any(|a| a.name == "audio_integrated_lufs" && !a.passed)
        );
    }
}
