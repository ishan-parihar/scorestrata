//! Brief compliance validation — checks that a project matches its production brief.
//!
//! The architecture spec (section 11.2, Level 5) requires checking:
//! - genre fit,
//! - mood fit,
//! - duration target,
//! - delivery target,
//! - section structure,
//! - reference-profile similarity.
//!
//! This module compares a `Brief` against the actual `ProjectConfig` and
//! `Arrangement` to produce a compliance report with diagnostics.

use scorestrata_core::diagnostic::{Diagnostic, MetricScope, SuggestedPatch};
use scorestrata_core::domain::{Arrangement, Brief, ProjectConfig, SegmentRole};
use scorestrata_core::error::Severity;
use scorestrata_core::ids::DiagnosticId;

/// A compliance check result for a single dimension.
#[derive(Debug, Clone)]
pub struct DimensionResult {
    /// Dimension name (e.g. "genre", "mood", "duration", "structure").
    pub dimension: String,
    /// Compliance score (0.0–1.0).
    pub score: f32,
    /// Whether this dimension passed.
    pub passed: bool,
    /// Diagnostics for this dimension.
    pub diagnostics: Vec<Diagnostic>,
}

/// The full brief compliance report.
#[derive(Debug, Clone)]
pub struct BriefComplianceReport {
    /// Overall compliance score (weighted average).
    pub overall_score: f32,
    /// Per-dimension results.
    pub dimensions: Vec<DimensionResult>,
    /// Whether the brief is fully satisfied.
    pub fully_compliant: bool,
    /// Weight configuration.
    pub weights: BriefWeights,
}

/// Weights for each compliance dimension.
#[derive(Debug, Clone)]
pub struct BriefWeights {
    /// Weight for genre fit.
    pub genre: f32,
    /// Weight for mood fit.
    pub mood: f32,
    /// Weight for duration fit.
    pub duration: f32,
    /// Weight for structure fit.
    pub structure: f32,
    /// Weight for delivery target fit.
    pub delivery: f32,
}

impl Default for BriefWeights {
    fn default() -> Self {
        Self {
            genre: 0.25,
            mood: 0.15,
            duration: 0.20,
            structure: 0.25,
            delivery: 0.15,
        }
    }
}

/// Check brief compliance against config and arrangement.
///
/// Returns a [`BriefComplianceReport`] with per-dimension scores,
/// diagnostics, and an overall compliance score.
#[must_use]
pub fn check_brief_compliance(
    brief: &Brief,
    config: &ProjectConfig,
    arrangement: &Arrangement,
    weights: &BriefWeights,
) -> BriefComplianceReport {
    let mut dimensions = Vec::new();

    dimensions.push(check_genre_fit(brief, config));
    dimensions.push(check_mood_fit(brief, config));
    dimensions.push(check_duration_fit(brief, config, arrangement));
    dimensions.push(check_structure_fit(brief, arrangement));
    dimensions.push(check_delivery_fit(brief, config));

    let overall_score = compute_weighted_score(&dimensions, weights);
    let fully_compliant = dimensions.iter().all(|d| d.passed);

    BriefComplianceReport {
        overall_score,
        dimensions,
        fully_compliant,
        weights: weights.clone(),
    }
}

/// Check genre fit: brief genre vs config genre.
fn check_genre_fit(brief: &Brief, config: &ProjectConfig) -> DimensionResult {
    let mut diagnostics = Vec::new();
    let score = match &brief.genre {
        Some(brief_genre) => {
            let config_genre = config.genre.to_lowercase();
            let brief_genre_lower = brief_genre.to_lowercase();

            if config_genre == brief_genre_lower {
                1.0
            } else if config_genre.contains(&brief_genre_lower)
                || brief_genre_lower.contains(&config_genre)
            {
                // Partial match (e.g. "melodic_techno" vs "techno")
                0.7
            } else {
                diagnostics.push(Diagnostic {
                    id: DiagnosticId::new(),
                    code: "BRIEF_GENRE_MISMATCH".to_string(),
                    metric: "brief.genre_fit".to_string(),
                    severity: Severity::Warning,
                    scope: MetricScope::default(),
                    evidence_uris: vec![
                        "project://brief/genre".to_string(),
                        "project://config/genre".to_string(),
                    ],
                    message: format!(
                        "Brief specifies genre '{}' but config has '{}'",
                        brief_genre, config.genre
                    ),
                    root_causes: vec!["genre mismatch between brief and config".to_string()],
                    suggested_patches: vec![SuggestedPatch {
                        op: "update_config_genre".to_string(),
                        params: serde_json::json!({
                            "genre": brief_genre,
                        }),
                        expected_delta: Some("+0.25".to_string()),
                        confidence: 0.9,
                    }],
                    expected_delta: None,
                    confidence: 0.9,
                });
                0.0
            }
        }
        None => {
            // No genre in brief — any config genre is acceptable
            1.0
        }
    };

    DimensionResult {
        dimension: "genre".to_string(),
        score,
        passed: score >= 0.5,
        diagnostics,
    }
}

/// Check mood fit: brief mood tags vs config mood tags.
fn check_mood_fit(brief: &Brief, config: &ProjectConfig) -> DimensionResult {
    let mut diagnostics = Vec::new();

    if brief.mood.is_empty() {
        return DimensionResult {
            dimension: "mood".to_string(),
            score: 1.0,
            passed: true,
            diagnostics,
        };
    }

    let brief_moods: Vec<String> = brief.mood.iter().map(|m| m.to_lowercase()).collect();
    let config_moods: Vec<String> = config.mood.iter().map(|m| m.to_lowercase()).collect();

    let matching = brief_moods
        .iter()
        .filter(|m| {
            config_moods
                .iter()
                .any(|cm| cm.contains(m.as_str()) || m.contains(cm.as_str()))
        })
        .count();

    #[expect(clippy::cast_precision_loss, reason = "Mood count fits in f32")]
    let score: f32 = if brief_moods.is_empty() {
        1.0
    } else {
        matching as f32 / brief_moods.len() as f32
    };

    if score < 0.5 {
        let missing: Vec<&String> = brief_moods
            .iter()
            .filter(|m| {
                !config_moods
                    .iter()
                    .any(|cm| cm.contains(m.as_str()) || m.contains(cm.as_str()))
            })
            .collect();
        diagnostics.push(Diagnostic {
            id: DiagnosticId::new(),
            code: "BRIEF_MOOD_INCOMPLETE".to_string(),
            metric: "brief.mood_fit".to_string(),
            severity: Severity::Warning,
            scope: MetricScope::default(),
            evidence_uris: vec!["project://brief/mood".to_string()],
            message: format!(
                "Brief specifies mood tags {:?} but config only covers {}/{}",
                brief.mood,
                matching,
                brief_moods.len()
            ),
            root_causes: vec!["missing mood tags in config".to_string()],
            suggested_patches: vec![SuggestedPatch {
                op: "update_config_mood".to_string(),
                params: serde_json::json!({
                    "add_mood": missing.iter().map(|s| s.as_str()).collect::<Vec<_>>(),
                }),
                expected_delta: Some(format!("+{:.2}", (0.5 - score).max(0.05))),
                confidence: 0.8,
            }],
            expected_delta: None,
            confidence: 0.8,
        });
    }

    DimensionResult {
        dimension: "mood".to_string(),
        score,
        passed: score >= 0.5,
        diagnostics,
    }
}

/// Check duration fit: brief duration constraints vs actual arrangement.
fn check_duration_fit(
    brief: &Brief,
    config: &ProjectConfig,
    arrangement: &Arrangement,
) -> DimensionResult {
    let mut diagnostics = Vec::new();
    let mut score: f32 = 1.0;

    // Check config duration bars
    if config.duration_bars != arrangement.total_bars {
        let delta = (config.duration_bars as f32 - arrangement.total_bars as f32).abs();
        #[expect(clippy::cast_precision_loss, reason = "Duration bar count fits in f32")]
        let max_expected = config.duration_bars.max(arrangement.total_bars) as f32;
        score = (1.0 - delta / max_expected).max(0.0);

        diagnostics.push(Diagnostic {
            id: DiagnosticId::new(),
            code: "BRIEF_DURATION_MISMATCH".to_string(),
            metric: "brief.duration_fit".to_string(),
            severity: Severity::Warning,
            scope: MetricScope::default(),
            evidence_uris: vec![
                "project://config/duration_bars".to_string(),
                "project://arrangement/total_bars".to_string(),
            ],
            message: format!(
                "Config specifies {} bars but arrangement has {} bars",
                config.duration_bars, arrangement.total_bars
            ),
            root_causes: vec!["arrangement duration does not match config".to_string()],
            suggested_patches: vec![SuggestedPatch {
                op: "adjust_arrangement_duration".to_string(),
                params: serde_json::json!({
                    "target_bars": config.duration_bars,
                }),
                expected_delta: None,
                confidence: 0.85,
            }],
            expected_delta: None,
            confidence: 0.85,
        });
    }

    // Check constraint-based duration
    for constraint in &brief.constraints {
        let lower = constraint.to_lowercase();
        if lower.contains("duration_bars") || lower.contains("bars") {
            // Try to extract a number from the constraint
            if let Some(num_str) = lower.split_whitespace().find_map(|w| w.parse::<u32>().ok())
                && num_str != arrangement.total_bars
            {
                score = score.min(0.5);
                diagnostics.push(Diagnostic {
                    id: DiagnosticId::new(),
                    code: "BRIEF_CONSTRAINT_DURATION_MISMATCH".to_string(),
                    metric: "brief.duration_fit".to_string(),
                    severity: Severity::Warning,
                    scope: MetricScope::default(),
                    evidence_uris: vec![],
                    message: format!(
                        "Brief constraint specifies {} bars but arrangement has {}",
                        num_str, arrangement.total_bars
                    ),
                    root_causes: vec!["arrangement does not satisfy brief constraint".to_string()],
                    suggested_patches: vec![],
                    expected_delta: None,
                    confidence: 0.7,
                });
            }
        }
    }

    DimensionResult {
        dimension: "duration".to_string(),
        score,
        passed: score >= 0.5,
        diagnostics,
    }
}

/// Check structure fit: brief requirements vs actual segment structure.
fn check_structure_fit(brief: &Brief, arrangement: &Arrangement) -> DimensionResult {
    let mut diagnostics = Vec::new();
    let segment_roles: Vec<SegmentRole> = arrangement.segments.iter().map(|s| s.role).collect();

    // Check for required sections based on genre/mood
    let needs_chorus = brief.mood.iter().any(|m| {
        let lower = m.to_lowercase();
        lower.contains("emotional") || lower.contains("uplifting") || lower.contains("anthemic")
    });

    let needs_breakdown = brief.mood.iter().any(|m| {
        let lower = m.to_lowercase();
        lower.contains("dark") || lower.contains("ambient") || lower.contains("atmospheric")
    });

    let mut score: f32 = 1.0;

    if needs_chorus && !segment_roles.contains(&SegmentRole::Chorus) {
        score -= 0.3;
        diagnostics.push(Diagnostic {
            id: DiagnosticId::new(),
            code: "BRIEF_MISSING_CHORUS".to_string(),
            metric: "brief.structure_fit".to_string(),
            severity: Severity::Warning,
            scope: MetricScope::default(),
            evidence_uris: vec!["project://arrangement/segments".to_string()],
            message:
                "Brief mood suggests emotional/uplifting character but no chorus section exists"
                    .to_string(),
            root_causes: vec!["missing chorus section for emotional brief".to_string()],
            suggested_patches: vec![SuggestedPatch {
                op: "add_segment".to_string(),
                params: serde_json::json!({
                    "role": "chorus",
                }),
                expected_delta: Some("+0.3".to_string()),
                confidence: 0.7,
            }],
            expected_delta: None,
            confidence: 0.7,
        });
    }

    if needs_breakdown && !segment_roles.contains(&SegmentRole::Breakdown) {
        score -= 0.2;
        diagnostics.push(Diagnostic {
            id: DiagnosticId::new(),
            code: "BRIEF_MISSING_BREAKDOWN".to_string(),
            metric: "brief.structure_fit".to_string(),
            severity: Severity::Info,
            scope: MetricScope::default(),
            evidence_uris: vec!["project://arrangement/segments".to_string()],
            message:
                "Brief mood suggests dark/atmospheric character but no breakdown section exists"
                    .to_string(),
            root_causes: vec!["missing breakdown section for atmospheric brief".to_string()],
            suggested_patches: vec![],
            expected_delta: None,
            confidence: 0.6,
        });
    }

    // Check minimum segment count
    if arrangement.segments.len() < 2 && !brief.constraints.iter().any(|c| c.contains("minimal")) {
        score -= 0.2;
        diagnostics.push(Diagnostic {
            id: DiagnosticId::new(),
            code: "BRIEF_INSUFFICIENT_SEGMENTS".to_string(),
            metric: "brief.structure_fit".to_string(),
            severity: Severity::Warning,
            scope: MetricScope::default(),
            evidence_uris: vec![],
            message: format!(
                "Arrangement has only {} segment(s) — expected at least 2 for a complete arrangement",
                arrangement.segments.len()
            ),
            root_causes: vec![
                "insufficient arrangement structure".to_string(),
            ],
            suggested_patches: vec![],
            expected_delta: None,
            confidence: 0.8,
        });
    }

    score = score.max(0.0);

    DimensionResult {
        dimension: "structure".to_string(),
        score,
        passed: score >= 0.5,
        diagnostics,
    }
}

/// Check delivery target fit: brief delivery vs config delivery.
fn check_delivery_fit(brief: &Brief, config: &ProjectConfig) -> DimensionResult {
    let diagnostics = Vec::new();

    let score: f32 = match &brief.delivery_target {
        Some(brief_target) => {
            if *brief_target == config.delivery_target {
                1.0
            } else {
                // Mismatch but not critical
                0.4
            }
        }
        None => 1.0, // No delivery target in brief
    };

    DimensionResult {
        dimension: "delivery".to_string(),
        score,
        passed: score >= 0.5,
        diagnostics,
    }
}

/// Compute weighted overall score from dimension results.
fn compute_weighted_score(dimensions: &[DimensionResult], weights: &BriefWeights) -> f32 {
    let mut weighted_sum: f32 = 0.0;
    let mut total_weight: f32 = 0.0;

    for dim in dimensions {
        let weight = match dim.dimension.as_str() {
            "genre" => weights.genre,
            "mood" => weights.mood,
            "duration" => weights.duration,
            "structure" => weights.structure,
            "delivery" => weights.delivery,
            _ => 0.1,
        };
        weighted_sum += dim.score * weight;
        total_weight += weight;
    }

    if total_weight > 0.0 {
        weighted_sum / total_weight
    } else {
        1.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use scorestrata_core::domain::{
        Arrangement, Brief, DeliveryTarget, Layer, ProjectConfig, Segment, SegmentRole,
        TimeSignature,
    };
    use scorestrata_core::ids::{LayerId, SegmentId};

    fn default_config() -> ProjectConfig {
        ProjectConfig {
            name: "test".to_string(),
            genre: "melodic_techno".to_string(),
            bpm: 124,
            key: "C minor".to_string(),
            time_signature: TimeSignature::default(),
            duration_bars: 64,
            delivery_target: DeliveryTarget::Streaming,
            mood: vec!["dark".to_string(), "driving".to_string()],
            references: vec![],
        }
    }

    fn default_arrangement() -> Arrangement {
        Arrangement {
            total_bars: 64,
            time_signature: TimeSignature::default(),
            segments: vec![
                Segment {
                    id: SegmentId::default(),
                    name: "intro".to_string(),
                    role: SegmentRole::Intro,
                    start_bar: 0,
                    bars: 8,
                    layers: vec![Layer {
                        id: LayerId::default(),
                        name: "pad".to_string(),
                        instrument: "synth".to_string(),
                        pattern_instances: vec![],
                    }],
                },
                Segment {
                    id: SegmentId::default(),
                    name: "chorus".to_string(),
                    role: SegmentRole::Chorus,
                    start_bar: 8,
                    bars: 16,
                    layers: vec![Layer {
                        id: LayerId::default(),
                        name: "lead".to_string(),
                        instrument: "synth".to_string(),
                        pattern_instances: vec![],
                    }],
                },
                Segment {
                    id: SegmentId::default(),
                    name: "outro".to_string(),
                    role: SegmentRole::Outro,
                    start_bar: 24,
                    bars: 8,
                    layers: vec![],
                },
            ],
            energy_curve: vec![],
        }
    }

    #[test]
    fn matching_genre_scores_high() {
        let brief = Brief {
            raw_text: "melodic techno".to_string(),
            genre: Some("melodic_techno".to_string()),
            mood: vec![],
            references: vec![],
            delivery_target: None,
            constraints: vec![],
            confidence: 0.9,
        };
        let result = check_genre_fit(&brief, &default_config());
        assert!((result.score - 1.0).abs() < f32::EPSILON);
        assert!(result.passed);
        assert!(result.diagnostics.is_empty());
    }

    #[test]
    fn mismatched_genre_produces_diagnostic() {
        let brief = Brief {
            raw_text: "ambient".to_string(),
            genre: Some("ambient".to_string()),
            mood: vec![],
            references: vec![],
            delivery_target: None,
            constraints: vec![],
            confidence: 0.9,
        };
        let result = check_genre_fit(&brief, &default_config());
        assert!((result.score).abs() < f32::EPSILON);
        assert!(!result.passed);
        assert_eq!(result.diagnostics.len(), 1);
        assert_eq!(result.diagnostics[0].code, "BRIEF_GENRE_MISMATCH");
    }

    #[test]
    fn partial_genre_match_scores_partially() {
        let brief = Brief {
            raw_text: "techno".to_string(),
            genre: Some("techno".to_string()),
            mood: vec![],
            references: vec![],
            delivery_target: None,
            constraints: vec![],
            confidence: 0.9,
        };
        let result = check_genre_fit(&brief, &default_config());
        assert!(result.score >= 0.6 && result.score <= 0.8);
        assert!(result.passed);
    }

    #[test]
    fn matching_mood_scores_high() {
        let brief = Brief {
            raw_text: "dark driving".to_string(),
            genre: None,
            mood: vec!["dark".to_string(), "driving".to_string()],
            references: vec![],
            delivery_target: None,
            constraints: vec![],
            confidence: 0.9,
        };
        let result = check_mood_fit(&brief, &default_config());
        assert!((result.score - 1.0).abs() < f32::EPSILON);
        assert!(result.passed);
    }

    #[test]
    fn partial_mood_match_scores_partially() {
        let brief = Brief {
            raw_text: "dark ethereal".to_string(),
            genre: None,
            mood: vec!["dark".to_string(), "ethereal".to_string()],
            references: vec![],
            delivery_target: None,
            constraints: vec![],
            confidence: 0.9,
        };
        let result = check_mood_fit(&brief, &default_config());
        assert!(result.score >= 0.4 && result.score <= 0.6);
    }

    #[test]
    fn matching_duration_scores_high() {
        let brief = Brief {
            raw_text: "64 bars".to_string(),
            genre: None,
            mood: vec![],
            references: vec![],
            delivery_target: None,
            constraints: vec![],
            confidence: 0.9,
        };
        let result = check_duration_fit(&brief, &default_config(), &default_arrangement());
        assert!((result.score - 1.0).abs() < f32::EPSILON);
        assert!(result.passed);
    }

    #[test]
    fn duration_mismatch_produces_diagnostic() {
        let brief = Brief {
            raw_text: "test".to_string(),
            genre: None,
            mood: vec![],
            references: vec![],
            delivery_target: None,
            constraints: vec![],
            confidence: 0.9,
        };
        let mut config = default_config();
        config.duration_bars = 128;
        let result = check_duration_fit(&brief, &config, &default_arrangement());
        assert!(result.score < 1.0);
        assert!(!result.diagnostics.is_empty());
    }

    #[test]
    fn emotional_brief_requiring_chorus() {
        let brief = Brief {
            raw_text: "emotional anthem".to_string(),
            genre: None,
            mood: vec!["emotional".to_string()],
            references: vec![],
            delivery_target: None,
            constraints: vec![],
            confidence: 0.9,
        };
        let arrangement = Arrangement {
            total_bars: 32,
            time_signature: TimeSignature::default(),
            segments: vec![Segment {
                id: SegmentId::default(),
                name: "verse".to_string(),
                role: SegmentRole::Verse,
                start_bar: 0,
                bars: 16,
                layers: vec![],
            }],
            energy_curve: vec![],
        };
        let result = check_structure_fit(&brief, &arrangement);
        assert!(result.score < 0.8);
        assert!(
            result
                .diagnostics
                .iter()
                .any(|d| d.code == "BRIEF_MISSING_CHORUS")
        );
    }

    #[test]
    fn full_compliance_report_with_matching_brief() {
        let brief = Brief {
            raw_text: "dark melodic techno with chorus".to_string(),
            genre: Some("melodic_techno".to_string()),
            mood: vec!["dark".to_string(), "driving".to_string()],
            references: vec![],
            delivery_target: Some(DeliveryTarget::Streaming),
            constraints: vec![],
            confidence: 0.9,
        };
        let report = check_brief_compliance(
            &brief,
            &default_config(),
            &default_arrangement(),
            &BriefWeights::default(),
        );
        assert!(report.overall_score >= 0.7);
        assert!(!report.dimensions.is_empty());
    }

    #[test]
    fn empty_brief_scores_perfectly() {
        let brief = Brief {
            raw_text: String::new(),
            genre: None,
            mood: vec![],
            references: vec![],
            delivery_target: None,
            constraints: vec![],
            confidence: 0.0,
        };
        let report = check_brief_compliance(
            &brief,
            &default_config(),
            &default_arrangement(),
            &BriefWeights::default(),
        );
        assert!((report.overall_score - 1.0).abs() < 0.01);
        assert!(report.fully_compliant);
    }

    #[test]
    fn delivery_mismatch_reduces_score() {
        let brief = Brief {
            raw_text: "club track".to_string(),
            genre: None,
            mood: vec![],
            references: vec![],
            delivery_target: Some(DeliveryTarget::Club),
            constraints: vec![],
            confidence: 0.9,
        };
        let result = check_delivery_fit(&brief, &default_config());
        assert!(result.score < 0.5);
    }
}
