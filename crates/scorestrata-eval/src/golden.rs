//! Golden test runner — validates projects against expected outputs.
//!
//! The architecture spec (section 16.5) requires golden tests to detect
//! regressions in compiler, validator, audio engine, export, and scoring.
//!
//! This module loads golden project configs, runs the validation pipeline,
//! and compares actual outputs against expected checksums/values.

use serde::{Deserialize, Serialize};
use std::path::Path;

use scorestrata_core::error::ScorestrataError;

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
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../eval/golden/melodic_techno_64_bar/config.json");
        if path.exists() {
            let config = load_golden_config(&path).unwrap();
            assert_eq!(config.name, "melodic_techno_64_bar");
            assert_eq!(config.expected_outputs.segment_count, 5);
            assert_eq!(config.expected_outputs.total_bars, 64);
            assert!(config.expected_outputs.has_chorus);
            assert!(config.expected_outputs.has_breakdown);
        }
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
        };
        let result = run_golden_assertions(&config, 3, 32, false, true, false, false, false);
        let failed: Vec<&GoldenAssertion> =
            result.assertions.iter().filter(|a| !a.passed).collect();
        assert!(!failed.is_empty());
        for assertion in &failed {
            assert!(assertion.error.is_some());
        }
    }
}
