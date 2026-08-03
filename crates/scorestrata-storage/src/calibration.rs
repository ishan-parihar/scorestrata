//! Calibration, preference, and memory — genre profiles, preference signals,
//! lesson confidence, and calibration drift detection.
//!
//! Phase 9 implementation per architecture spec sections 11.5–11.8 and 8.13.

use serde::{Deserialize, Serialize, de::DeserializeOwned};
use std::collections::BTreeMap;
use std::path::Path;
use std::sync::atomic::{AtomicU64, Ordering};

use scorestrata_core::error::ScorestrataError;

use crate::StorageResult;

/// Monotonic counter for generating unique template IDs.
static TEMPLATE_COUNTER: AtomicU64 = AtomicU64::new(1);

/// Monotonic counter for generating unique preference signal IDs.
static PREF_COUNTER: AtomicU64 = AtomicU64::new(1);

/// Monotonic counter for generating unique lesson IDs.
static LESSON_COUNTER: AtomicU64 = AtomicU64::new(1);

/// Genre profile — calibration targets for a specific genre.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GenreProfile {
    /// Genre name.
    pub name: String,
    /// BPM range.
    pub bpm_range: (u32, u32),
    /// Kick pattern type.
    pub kick_pattern: String,
    /// Bass syncopation level.
    pub bass_syncopation: String,
    /// Minimum chorus energy delta.
    pub chorus_energy_delta_min: f32,
    /// Minimum drum density.
    pub drum_density_min: f32,
    /// Spectral brightness target.
    pub spectral_brightness_target: f32,
    /// Loudness target in LUFS.
    pub loudness_target_lufs: f32,
    /// Metric weight overrides.
    pub metric_weights: BTreeMap<String, f32>,
}

impl GenreProfile {
    /// Create a new genre profile with defaults.
    #[must_use]
    pub fn new(name: &str) -> Self {
        Self {
            name: name.to_string(),
            bpm_range: (120, 130),
            kick_pattern: "four_on_floor".to_string(),
            bass_syncopation: "medium".to_string(),
            chorus_energy_delta_min: 0.15,
            drum_density_min: 0.7,
            spectral_brightness_target: 0.6,
            loudness_target_lufs: -14.0,
            metric_weights: BTreeMap::new(),
        }
    }
}

/// The default loudness target (LUFS) for genres without a dedicated profile.
///
/// Matches the streaming-master convention used across the validator.
pub const DEFAULT_LOUDNESS_TARGET_LUFS: f32 = -14.0;

/// Canonical genre → loudness-target table (LUFS), from architecture §11.6.
///
/// Kept as a `const` table so [`loudness_target_for`] — called on every
/// `normalize_loudness_target` patch — does not allocate profile structs.
/// Must stay in sync with [`default_genre_profiles`].
const GENRE_LOUDNESS_TARGETS: &[(&str, f32)] = &[
    ("melodic_techno", -10.0),
    ("techno", -10.0),
    ("house", -12.0),
    ("ambient", -16.0),
    ("cinematic", -16.0),
    ("trance", -9.0),
    ("dubstep", -8.0),
    ("lo-fi", -16.0),
];

/// Build the curated default genre profiles from the architecture spec §11.6.
///
/// The spec's example TOML defines two reference profiles; we extend the set
/// to cover every genre the config generator can produce so loudness
/// normalization and audio validation always have a calibrated target.
/// Note: the full literals are intentionally duplicated rather than built
/// from `GenreProfile::new()` + overrides — this keeps the curated spec
/// values visible at a glance and prevents accidental drift.
#[must_use]
pub fn default_genre_profiles() -> Vec<GenreProfile> {
    vec![
        GenreProfile {
            name: "melodic_techno".to_string(),
            bpm_range: (120, 126),
            kick_pattern: "four_on_floor".to_string(),
            bass_syncopation: "medium_high".to_string(),
            chorus_energy_delta_min: 0.15,
            drum_density_min: 0.75,
            spectral_brightness_target: 0.62,
            loudness_target_lufs: -10.0,
            // §11.8 metric weighting by genre: driving dance floor, so
            // rhythm and structure dominate the scorecard.
            metric_weights: BTreeMap::from([
                ("rhythm".to_string(), 1.3),
                ("structure".to_string(), 1.2),
                ("harmony".to_string(), 0.9),
                ("arrangement".to_string(), 1.0),
            ]),
        },
        GenreProfile {
            name: "techno".to_string(),
            bpm_range: (128, 140),
            kick_pattern: "four_on_floor".to_string(),
            bass_syncopation: "medium".to_string(),
            chorus_energy_delta_min: 0.12,
            drum_density_min: 0.7,
            spectral_brightness_target: 0.58,
            loudness_target_lufs: -10.0,
            metric_weights: BTreeMap::from([
                ("rhythm".to_string(), 1.4),
                ("structure".to_string(), 1.1),
                ("harmony".to_string(), 0.8),
                ("arrangement".to_string(), 1.0),
            ]),
        },
        GenreProfile {
            name: "house".to_string(),
            bpm_range: (118, 126),
            kick_pattern: "four_on_floor".to_string(),
            bass_syncopation: "medium".to_string(),
            chorus_energy_delta_min: 0.12,
            drum_density_min: 0.7,
            spectral_brightness_target: 0.55,
            loudness_target_lufs: -12.0,
            metric_weights: BTreeMap::from([
                ("rhythm".to_string(), 1.3),
                ("structure".to_string(), 1.1),
                ("harmony".to_string(), 1.0),
                ("arrangement".to_string(), 1.0),
            ]),
        },
        GenreProfile {
            name: "ambient".to_string(),
            bpm_range: (70, 100),
            kick_pattern: "sparse_or_none".to_string(),
            bass_syncopation: "low".to_string(),
            chorus_energy_delta_min: 0.05,
            drum_density_min: 0.10,
            spectral_brightness_target: 0.48,
            loudness_target_lufs: -16.0,
            metric_weights: BTreeMap::from([
                ("harmony".to_string(), 1.4),
                ("structure".to_string(), 1.2),
                ("rhythm".to_string(), 0.6),
                ("arrangement".to_string(), 1.0),
            ]),
        },
        GenreProfile {
            name: "cinematic".to_string(),
            bpm_range: (80, 100),
            kick_pattern: "sparse_or_none".to_string(),
            bass_syncopation: "low".to_string(),
            chorus_energy_delta_min: 0.10,
            drum_density_min: 0.3,
            spectral_brightness_target: 0.5,
            loudness_target_lufs: -16.0,
            metric_weights: BTreeMap::from([
                ("harmony".to_string(), 1.3),
                ("arrangement".to_string(), 1.2),
                ("structure".to_string(), 1.0),
                ("rhythm".to_string(), 0.7),
            ]),
        },
        GenreProfile {
            name: "trance".to_string(),
            bpm_range: (132, 144),
            kick_pattern: "four_on_floor".to_string(),
            bass_syncopation: "medium".to_string(),
            chorus_energy_delta_min: 0.15,
            drum_density_min: 0.75,
            spectral_brightness_target: 0.6,
            loudness_target_lufs: -9.0,
            metric_weights: BTreeMap::from([
                ("structure".to_string(), 1.3),
                ("rhythm".to_string(), 1.2),
                ("harmony".to_string(), 1.0),
                ("arrangement".to_string(), 1.0),
            ]),
        },
        GenreProfile {
            name: "dubstep".to_string(),
            bpm_range: (136, 146),
            kick_pattern: "half_time".to_string(),
            bass_syncopation: "high".to_string(),
            chorus_energy_delta_min: 0.18,
            drum_density_min: 0.6,
            spectral_brightness_target: 0.5,
            loudness_target_lufs: -8.0,
            metric_weights: BTreeMap::from([
                ("rhythm".to_string(), 1.3),
                ("harmony".to_string(), 1.1),
                ("structure".to_string(), 1.0),
                ("arrangement".to_string(), 0.9),
            ]),
        },
        GenreProfile {
            name: "lo-fi".to_string(),
            bpm_range: (70, 90),
            kick_pattern: "sparse_or_none".to_string(),
            bass_syncopation: "low".to_string(),
            chorus_energy_delta_min: 0.05,
            drum_density_min: 0.2,
            spectral_brightness_target: 0.35,
            loudness_target_lufs: -16.0,
            metric_weights: BTreeMap::from([
                ("harmony".to_string(), 1.2),
                ("rhythm".to_string(), 1.1),
                ("structure".to_string(), 1.0),
                ("arrangement".to_string(), 0.9),
            ]),
        },
    ]
}

/// The calibrated loudness target (LUFS) for a genre, if a profile exists.
///
/// Returns `None` for unknown genres so callers can fall back to
/// [`DEFAULT_LOUDNESS_TARGET_LUFS`]. Allocation-free — reads the const table.
#[must_use]
pub fn loudness_target_for(genre: &str) -> Option<f32> {
    GENRE_LOUDNESS_TARGETS
        .iter()
        .find(|(g, _)| *g == genre)
        .map(|(_, lufs)| *lufs)
}

/// A human preference signal.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PreferenceSignal {
    /// Signal ID.
    pub id: String,
    /// Project ID.
    pub project_id: String,
    /// Signal type.
    pub signal_type: PreferenceType,
    /// What was preferred (e.g. candidate ID, patch ID).
    pub preferred: String,
    /// What was rejected (if A/B comparison).
    pub rejected: Option<String>,
    /// Context (e.g. segment, layer, metric).
    pub context: BTreeMap<String, String>,
    /// Confidence (0.0–1.0).
    pub confidence: f32,
    /// Timestamp.
    pub timestamp: String,
}

/// Type of preference signal.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum PreferenceType {
    /// Human chose candidate A over B.
    CandidateChoice,
    /// Human rejected a patch.
    PatchRejection,
    /// Human edited a result.
    ManualEdit,
    /// Human approved final render.
    FinalApproval,
    /// Human overrode a metric weight.
    WeightOverride,
}

impl PreferenceSignal {
    /// Create a new preference signal.
    #[must_use]
    pub fn new(project_id: &str, signal_type: PreferenceType, preferred: &str) -> Self {
        Self {
            id: format!("pref_{}", PREF_COUNTER.fetch_add(1, Ordering::Relaxed)),
            project_id: project_id.to_string(),
            signal_type,
            preferred: preferred.to_string(),
            rejected: None,
            context: BTreeMap::new(),
            confidence: 0.8,
            timestamp: chrono::Utc::now().to_rfc3339(),
        }
    }
}

/// A lesson learned from experience.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Lesson {
    /// Lesson ID.
    pub id: String,
    /// Scope (e.g. genre, segment type, metric).
    pub scope: String,
    /// Trigger diagnostic code.
    pub trigger: String,
    /// Successful patch operation.
    pub successful_patch: Option<String>,
    /// Failed patch operations.
    pub failed_patches: Vec<String>,
    /// Evidence URIs.
    pub evidence_uris: Vec<String>,
    /// Genre this lesson applies to.
    pub genre: Option<String>,
    /// Confidence (0.0–1.0).
    pub confidence: f32,
    /// Success count.
    pub success_count: u32,
    /// Failure count.
    pub failure_count: u32,
}

impl Lesson {
    /// Create a new lesson.
    #[must_use]
    pub fn new(trigger: &str, scope: &str) -> Self {
        Self {
            id: format!("les_{}", LESSON_COUNTER.fetch_add(1, Ordering::Relaxed)),
            scope: scope.to_string(),
            trigger: trigger.to_string(),
            successful_patch: None,
            failed_patches: vec![],
            evidence_uris: vec![],
            genre: None,
            confidence: 0.5,
            success_count: 0,
            failure_count: 0,
        }
    }

    /// Record a success.
    pub fn record_success(&mut self) {
        self.success_count += 1;
        self.confidence = (self.confidence + 0.1).min(1.0);
    }

    /// Record a failure.
    pub fn record_failure(&mut self) {
        self.failure_count += 1;
        self.confidence = (self.confidence - 0.1).max(0.0);
    }
}

/// Calibration profile — collects signals and maintains calibration state.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct CalibrationProfile {
    /// Genre profiles.
    pub genre_profiles: BTreeMap<String, GenreProfile>,
    /// Preference signals.
    pub preferences: Vec<PreferenceSignal>,
    /// Lessons.
    pub lessons: Vec<Lesson>,
    /// Calibration drift score (0.0 = no drift, 1.0 = high drift).
    pub drift_score: f32,
}

impl CalibrationProfile {
    /// Create a new calibration profile.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Add a genre profile.
    pub fn add_genre_profile(&mut self, profile: GenreProfile) {
        self.genre_profiles.insert(profile.name.clone(), profile);
    }

    /// Get a genre profile.
    #[must_use]
    pub fn get_genre_profile(&self, genre: &str) -> Option<&GenreProfile> {
        self.genre_profiles.get(genre)
    }

    /// Record a preference signal.
    pub fn record_preference(&mut self, signal: PreferenceSignal) {
        self.preferences.push(signal);
    }

    /// Record a lesson.
    pub fn record_lesson(&mut self, lesson: Lesson) {
        self.lessons.push(lesson);
    }

    /// Get lessons for a trigger.
    #[must_use]
    pub fn get_lessons_for_trigger(&self, trigger: &str) -> Vec<&Lesson> {
        self.lessons
            .iter()
            .filter(|l| l.trigger == trigger)
            .collect()
    }

    /// Detect calibration drift based on preference consistency.
    pub fn detect_drift(&mut self) {
        // Need at least 6 preferences to avoid overlap between recent and earlier slices
        if self.preferences.len() < 6 {
            self.drift_score = 0.0;
            return;
        }
        // Split: recent 5 vs everything else
        let split = self.preferences.len() - 5;
        let recent = &self.preferences[split..];
        let earlier = &self.preferences[..split];
        #[expect(clippy::cast_precision_loss, reason = "Preference counts fit in f32")]
        let recent_approvals = recent
            .iter()
            .filter(|p| p.signal_type == PreferenceType::FinalApproval)
            .count() as f32;
        #[expect(clippy::cast_precision_loss, reason = "Preference counts fit in f32")]
        let earlier_approvals = earlier
            .iter()
            .filter(|p| p.signal_type == PreferenceType::FinalApproval)
            .count() as f32;
        if earlier.is_empty() {
            self.drift_score = 0.0;
        } else {
            #[expect(clippy::cast_precision_loss, reason = "Preference counts fit in f32")]
            let recent_rate = recent_approvals / recent.len() as f32;
            #[expect(clippy::cast_precision_loss, reason = "Preference counts fit in f32")]
            let earlier_rate = earlier_approvals / earlier.len() as f32;
            self.drift_score = (recent_rate - earlier_rate).abs();
        }
    }
}

/// Patch template — ranked repair strategy with success tracking.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PatchTemplate {
    /// Template ID.
    pub id: String,
    /// Patch operation name (e.g. `quantize_layer_to_scale`).
    pub operation: String,
    /// Description of what this patch does.
    pub description: String,
    /// Scope where this patch applies (e.g. `harmony`, `transition`, `mix`).
    pub scope: String,
    /// Genres this template works well for.
    pub genres: Vec<String>,
    /// Total times this template was applied.
    pub usage_count: u32,
    /// Times this template succeeded.
    pub success_count: u32,
    /// Times this template failed.
    pub failure_count: u32,
    /// Average score delta when successful.
    pub avg_score_delta: f32,
    /// Average cost (CPU seconds) when applied.
    pub avg_cost_seconds: f32,
}

impl PatchTemplate {
    /// Create a new patch template.
    #[must_use]
    pub fn new(operation: &str, scope: &str) -> Self {
        Self {
            id: format!("tpl_{}", TEMPLATE_COUNTER.fetch_add(1, Ordering::Relaxed)),
            operation: operation.to_string(),
            description: String::new(),
            scope: scope.to_string(),
            genres: vec![],
            usage_count: 0,
            success_count: 0,
            failure_count: 0,
            avg_score_delta: 0.0,
            avg_cost_seconds: 0.0,
        }
    }

    /// Compute success rate (0.0–1.0). Returns 0.5 if no attempts.
    #[must_use]
    pub fn success_rate(&self) -> f32 {
        let total = self.success_count + self.failure_count;
        if total == 0 {
            return 0.5;
        }
        #[expect(clippy::cast_precision_loss, reason = "Counts fit in f32")]
        {
            self.success_count as f32 / total as f32
        }
    }

    /// Record a successful application.
    pub fn record_success(&mut self, score_delta: f32, cost_seconds: f32) {
        self.usage_count += 1;
        self.success_count += 1;
        #[expect(
            clippy::cast_precision_loss,
            reason = "Success count fits in f32 for averaging"
        )]
        let n = self.success_count as f32;
        self.avg_score_delta = self.avg_score_delta.mul_add(n - 1.0, score_delta) / n;
        self.avg_cost_seconds = self.avg_cost_seconds.mul_add(n - 1.0, cost_seconds) / n;
    }

    /// Record a failed application.
    pub const fn record_failure(&mut self) {
        self.usage_count += 1;
        self.failure_count += 1;
    }
}

/// Patch template store — manages ranked repair strategies.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct PatchTemplateStore {
    /// All templates, keyed by ID.
    templates: BTreeMap<String, PatchTemplate>,
    /// Index: operation name → template IDs.
    by_operation: BTreeMap<String, Vec<String>>,
    /// Index: scope → template IDs.
    by_scope: BTreeMap<String, Vec<String>>,
    /// Index: genre → template IDs.
    by_genre: BTreeMap<String, Vec<String>>,
}

impl PatchTemplateStore {
    /// Create a new empty patch template store.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Add a template.
    pub fn add(&mut self, template: PatchTemplate) {
        let id = template.id.clone();
        let op = template.operation.clone();
        let scope = template.scope.clone();
        let genres = template.genres.clone();

        self.by_operation.entry(op).or_default().push(id.clone());
        self.by_scope.entry(scope).or_default().push(id.clone());
        for genre in &genres {
            self.by_genre
                .entry(genre.clone())
                .or_default()
                .push(id.clone());
        }
        self.templates.insert(id, template);
    }

    /// Get a template by ID.
    #[must_use]
    pub fn get(&self, id: &str) -> Option<&PatchTemplate> {
        self.templates.get(id)
    }

    /// Get a mutable reference to a template.
    pub fn get_mut(&mut self, id: &str) -> Option<&mut PatchTemplate> {
        self.templates.get_mut(id)
    }

    /// Get templates for an operation.
    #[must_use]
    pub fn get_by_operation(&self, operation: &str) -> Vec<&PatchTemplate> {
        self.by_operation
            .get(operation)
            .map_or_else(Vec::new, |ids| {
                ids.iter().filter_map(|id| self.templates.get(id)).collect()
            })
    }

    /// Get templates for a scope.
    #[must_use]
    pub fn get_by_scope(&self, scope: &str) -> Vec<&PatchTemplate> {
        self.by_scope.get(scope).map_or_else(Vec::new, |ids| {
            ids.iter().filter_map(|id| self.templates.get(id)).collect()
        })
    }

    /// Get templates for a genre.
    #[must_use]
    pub fn get_by_genre(&self, genre: &str) -> Vec<&PatchTemplate> {
        self.by_genre.get(genre).map_or_else(Vec::new, |ids| {
            ids.iter().filter_map(|id| self.templates.get(id)).collect()
        })
    }

    /// Get top N templates ranked by success rate, filtered by scope and genre.
    #[must_use]
    pub fn get_top_templates(
        &self,
        scope: &str,
        genre: Option<&str>,
        n: usize,
    ) -> Vec<&PatchTemplate> {
        let mut results: Vec<&PatchTemplate> = self
            .templates
            .values()
            .filter(|t| t.scope == scope)
            .filter(|t| genre.is_none_or(|g| t.genres.iter().any(|tg| tg == g)))
            .collect();
        results.sort_by(|a, b| {
            b.success_rate()
                .partial_cmp(&a.success_rate())
                .unwrap_or(std::cmp::Ordering::Equal)
        });
        results.into_iter().take(n).collect()
    }

    /// Get the total number of templates.
    #[must_use]
    pub fn len(&self) -> usize {
        self.templates.len()
    }

    /// Whether the store is empty.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.templates.is_empty()
    }
}

/// Brief compliance — scores how well a project matches its brief.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BriefCompliance {
    /// Overall compliance score (0.0–1.0).
    pub overall_score: f32,
    /// Genre fit score.
    pub genre_score: f32,
    /// Mood fit score.
    pub mood_score: f32,
    /// Duration fit score.
    pub duration_score: f32,
    /// Structure fit score.
    pub structure_score: f32,
    /// Per-metric breakdown.
    pub metric_scores: BTreeMap<String, f32>,
}

impl BriefCompliance {
    /// Compute overall score as weighted average of sub-scores.
    pub fn compute_overall(&mut self) {
        self.overall_score = self.genre_score.mul_add(
            0.3,
            self.mood_score.mul_add(
                0.2,
                self.duration_score.mul_add(0.2, self.structure_score * 0.3),
            ),
        );
    }
}

/// Generic JSON persistence helper — serialize and write to disk.
///
/// # Errors
///
/// Returns [`ScorestrataError::Io`] or [`ScorestrataError::Serialization`].
pub fn save_json<T: Serialize>(path: &Path, value: &T, label: &str) -> StorageResult<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| ScorestrataError::Io {
            path: parent.display().to_string(),
            source: e,
        })?;
    }
    let data =
        serde_json::to_string_pretty(value).map_err(|e| ScorestrataError::Serialization {
            message: format!("Failed to serialize {label}: {e}"),
        })?;
    std::fs::write(path, data).map_err(|e| ScorestrataError::Io {
        path: path.display().to_string(),
        source: e,
    })?;
    Ok(())
}

/// Generic JSON persistence helper — read and deserialize from disk.
/// Returns the provided `default` value if the file does not exist.
///
/// # Errors
///
/// Returns [`ScorestrataError::Io`] or [`ScorestrataError::Serialization`].
pub fn load_json<T: for<'de> DeserializeOwned>(
    path: &Path,
    default: T,
    label: &str,
) -> StorageResult<T> {
    if !path.exists() {
        return Ok(default);
    }
    let content = std::fs::read_to_string(path).map_err(|e| ScorestrataError::Io {
        path: path.display().to_string(),
        source: e,
    })?;
    let value: T = serde_json::from_str(&content).map_err(|e| ScorestrataError::Serialization {
        message: format!("Failed to parse {label}: {e}"),
    })?;
    Ok(value)
}

/// Save calibration profile to disk.
///
/// # Errors
///
/// Returns [`ScorestrataError::Io`] if the file cannot be written.
pub fn save_calibration_profile(path: &Path, profile: &CalibrationProfile) -> StorageResult<()> {
    save_json(path, profile, "calibration profile")
}

/// Load calibration profile from disk.
///
/// Returns empty profile if file does not exist.
///
/// # Errors
///
/// Returns [`ScorestrataError::Io`] or [`ScorestrataError::Serialization`].
pub fn load_calibration_profile(path: &Path) -> StorageResult<CalibrationProfile> {
    load_json(path, CalibrationProfile::new(), "calibration profile")
}

/// Save patch template store to disk.
///
/// # Errors
///
/// Returns [`ScorestrataError::Io`] if the file cannot be written.
pub fn save_patch_template_store(path: &Path, store: &PatchTemplateStore) -> StorageResult<()> {
    save_json(path, store, "patch template store")
}

/// Load patch template store from disk.
///
/// Returns empty store if file does not exist.
///
/// # Errors
///
/// Returns [`ScorestrataError::Io`] or [`ScorestrataError::Serialization`].
pub fn load_patch_template_store(path: &Path) -> StorageResult<PatchTemplateStore> {
    load_json(path, PatchTemplateStore::new(), "patch template store")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn genre_profile_created_with_defaults() {
        let profile = GenreProfile::new("melodic_techno");
        assert_eq!(profile.name, "melodic_techno");
        assert_eq!(profile.bpm_range, (120, 130));
        assert!((profile.loudness_target_lufs - (-14.0)).abs() < 0.01);
    }

    #[test]
    #[expect(clippy::unwrap_used, reason = "Test asserts spec values are present")]
    #[expect(clippy::float_cmp, reason = "Spec values are exact f32 literals")]
    fn default_genre_profiles_match_spec_targets() {
        let profiles = default_genre_profiles();
        assert!(!profiles.is_empty());
        let by_name = |name: &str| profiles.iter().find(|p| p.name == name);
        // §11.6 spec values.
        assert_eq!(
            by_name("melodic_techno").unwrap().loudness_target_lufs,
            -10.0
        );
        assert_eq!(by_name("ambient").unwrap().loudness_target_lufs, -16.0);
        assert_eq!(
            by_name("melodic_techno").unwrap().chorus_energy_delta_min,
            0.15
        );
        assert_eq!(by_name("ambient").unwrap().chorus_energy_delta_min, 0.05);
    }

    #[test]
    fn loudness_target_for_known_and_unknown_genres() {
        assert_eq!(loudness_target_for("melodic_techno"), Some(-10.0));
        assert_eq!(loudness_target_for("ambient"), Some(-16.0));
        assert_eq!(loudness_target_for("house"), Some(-12.0));
        assert_eq!(loudness_target_for("jazz"), None);
        assert_eq!(loudness_target_for(""), None);
    }

    #[test]
    fn lesson_confidence_increases_on_success() {
        let mut lesson = Lesson::new("TRANSITION_ENERGY_LOW", "transition");
        let initial = lesson.confidence;
        lesson.record_success();
        assert!(lesson.confidence > initial);
        assert_eq!(lesson.success_count, 1);
    }

    #[test]
    fn lesson_confidence_decreases_on_failure() {
        let mut lesson = Lesson::new("SCALE_VIOLATION", "harmony");
        lesson.confidence = 0.8;
        lesson.record_failure();
        assert!(lesson.confidence < 0.8);
        assert_eq!(lesson.failure_count, 1);
    }

    #[test]
    fn calibration_profile_manages_genres() {
        let mut profile = CalibrationProfile::new();
        profile.add_genre_profile(GenreProfile::new("ambient"));
        profile.add_genre_profile(GenreProfile::new("techno"));
        assert!(profile.get_genre_profile("ambient").is_some());
        assert!(profile.get_genre_profile("techno").is_some());
        assert!(profile.get_genre_profile("jazz").is_none());
    }

    #[test]
    fn calibration_profile_records_preferences() {
        let mut profile = CalibrationProfile::new();
        let signal = PreferenceSignal::new("proj_1", PreferenceType::FinalApproval, "render_1");
        profile.record_preference(signal);
        assert_eq!(profile.preferences.len(), 1);
    }

    #[test]
    fn patch_template_success_rate_computed() {
        let mut tpl = PatchTemplate::new("quantize_layer_to_scale", "harmony");
        assert!((tpl.success_rate() - 0.5).abs() < 0.01);
        tpl.record_success(0.1, 0.01);
        tpl.record_success(0.15, 0.02);
        tpl.record_failure();
        assert!((tpl.success_rate() - (2.0 / 3.0)).abs() < 0.01);
    }

    #[test]
    fn patch_template_store_manages_templates() {
        let mut store = PatchTemplateStore::new();
        let mut t1 = PatchTemplate::new("quantize", "harmony");
        t1.genres = vec!["techno".to_string()];
        t1.record_success(0.1, 0.01);
        let mut t2 = PatchTemplate::new("add_package", "harmony");
        t2.genres = vec!["ambient".to_string()];
        t2.record_success(0.05, 0.01);
        t2.record_failure();
        store.add(t1);
        store.add(t2);

        assert_eq!(store.len(), 2);
        assert_eq!(store.get_by_scope("harmony").len(), 2);
        assert_eq!(store.get_by_genre("techno").len(), 1);

        let top = store.get_top_templates("harmony", Some("techno"), 5);
        assert_eq!(top.len(), 1);
        assert_eq!(top[0].genres[0], "techno");
    }

    #[test]
    fn brief_compliance_overall_computed() {
        let mut bc = BriefCompliance {
            overall_score: 0.0,
            genre_score: 0.8,
            mood_score: 0.6,
            duration_score: 0.9,
            structure_score: 0.7,
            metric_scores: BTreeMap::new(),
        };
        bc.compute_overall();
        let expected = 0.8f32.mul_add(0.3, 0.6f32.mul_add(0.2, 0.9f32.mul_add(0.2, 0.7 * 0.3)));
        assert!((bc.overall_score - expected).abs() < 0.01);
    }

    #[test]
    #[expect(clippy::unwrap_used, reason = "Test should panic on unexpected error")]
    fn calibration_profile_persistence_roundtrip() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("calibration.json");

        let mut profile = CalibrationProfile::new();
        profile.add_genre_profile(GenreProfile::new("techno"));
        profile.record_preference(PreferenceSignal::new(
            "proj_1",
            PreferenceType::FinalApproval,
            "render_1",
        ));

        save_calibration_profile(&path, &profile).unwrap();
        let loaded = load_calibration_profile(&path).unwrap();
        assert!(loaded.get_genre_profile("techno").is_some());
        assert_eq!(loaded.preferences.len(), 1);
    }

    #[test]
    #[expect(clippy::unwrap_used, reason = "Test should panic on unexpected error")]
    fn patch_template_store_persistence_roundtrip() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("templates.json");

        let mut store = PatchTemplateStore::new();
        let mut t = PatchTemplate::new("add_transition_package", "transition");
        t.record_success(0.15, 0.05);
        store.add(t);

        save_patch_template_store(&path, &store).unwrap();
        let loaded = load_patch_template_store(&path).unwrap();
        assert_eq!(loaded.len(), 1);
    }

    #[test]
    fn default_genre_profiles_carry_calibrated_metric_weights() {
        // §11.8: every curated genre profile must actually weight its metrics
        // — an empty table would make the genre calibration deadweight that
        // the scorecard never sees.
        let profiles = default_genre_profiles();
        assert!(profiles.len() >= 8, "full genre coverage");
        for profile in &profiles {
            assert!(
                !profile.metric_weights.is_empty(),
                "profile '{}' must define metric weights",
                profile.name
            );
            for (category, weight) in &profile.metric_weights {
                assert!(
                    *weight > 0.0,
                    "profile '{}' weight for '{category}' must be positive",
                    profile.name
                );
            }
        }
    }

    #[test]
    fn genre_weight_keys_resolve_against_scorecard_categories() {
        // §11.8: the scorecard looks weights up by the metric-name prefix
        // (`metric_category` splits on '.'). A typo here would silently
        // deaden a weight with no compile error, so every key in every
        // profile must resolve against the category set the validator emits.
        let known_categories = [
            "arrangement",
            "audio",
            "brief",
            "harmony",
            "regression",
            "render",
            "rhythm",
            "structure",
            "timeline",
        ];
        let profiles = default_genre_profiles();
        for profile in &profiles {
            for category in profile.metric_weights.keys() {
                assert!(
                    known_categories.contains(&category.as_str()),
                    "profile '{}' weight key '{category}' is not a scorecard metric category",
                    profile.name
                );
            }
        }
    }
}
