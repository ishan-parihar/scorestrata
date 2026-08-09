//! Lesson store — persistent storage for learned repair strategies.
//!
//! Phase 7 implementation per architecture spec sections 8.12, 13.7, and 15.4.
//!
//! Lessons are the system's accumulated repair knowledge. Each lesson records
//! what diagnostic triggered it, what patch succeeded or failed, and the
//! confidence earned through repeated success.

use std::collections::BTreeMap;
use std::path::Path;

use scorestrata_core::error::ScorestrataError;
use serde::{Deserialize, Serialize};

use crate::StorageResult;

/// A lesson learned from composition experience.
///
/// Lessons accumulate over time as the system discovers which patches fix
/// which diagnostics in which genres. Higher confidence means more verified
/// successes.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StoredLesson {
    /// Unique identifier.
    pub id: String,
    /// Scope where this lesson applies (e.g. `transition`, `harmony`, `mix`).
    pub scope: String,
    /// Diagnostic code that triggers this lesson (e.g. `SCALE_VIOLATION`).
    pub trigger: String,
    /// Successful patch operation name.
    pub successful_patch: Option<String>,
    /// Failed patch operation names (to avoid repeating mistakes).
    pub failed_patches: Vec<String>,
    /// Evidence URIs supporting this lesson.
    pub evidence_uris: Vec<String>,
    /// Genre this lesson applies to (None = genre-agnostic).
    pub genre: Option<String>,
    /// Confidence score (0.0–1.0). Increases with success, decreases with failure.
    pub confidence: f32,
    /// Total times this lesson's patch succeeded.
    pub success_count: u32,
    /// Total times this lesson's patch failed.
    pub failure_count: u32,
    /// Creation timestamp.
    pub created_at: String,
    /// Last time this lesson was updated.
    pub updated_at: String,
}

impl StoredLesson {
    /// Compute success rate (0.0–1.0). Returns 0.5 if no attempts recorded.
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
}

/// Lesson store — manages the collection of learned repair strategies.
///
/// Provides query methods for retrieving lessons by trigger, scope, genre,
/// and confidence threshold. Supports persistence to disk as JSON.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct LessonStore {
    /// All lessons, keyed by lesson ID.
    lessons: BTreeMap<String, StoredLesson>,
    /// Index: trigger code → lesson IDs.
    by_trigger: BTreeMap<String, Vec<String>>,
    /// Index: scope → lesson IDs.
    by_scope: BTreeMap<String, Vec<String>>,
    /// Index: genre → lesson IDs.
    by_genre: BTreeMap<String, Vec<String>>,
}

impl LessonStore {
    /// Create a new empty lesson store.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Record a new lesson.
    pub fn record(&mut self, lesson: StoredLesson) {
        let id = lesson.id.clone();
        let trigger = lesson.trigger.clone();
        let scope = lesson.scope.clone();
        let genre = lesson.genre.clone();

        self.by_trigger.entry(trigger).or_default().push(id.clone());
        self.by_scope.entry(scope).or_default().push(id.clone());
        if let Some(genre) = &genre {
            self.by_genre
                .entry(genre.clone())
                .or_default()
                .push(id.clone());
        }
        self.lessons.insert(id, lesson);
    }

    /// Get a lesson by ID.
    #[must_use]
    pub fn get(&self, id: &str) -> Option<&StoredLesson> {
        self.lessons.get(id)
    }

    /// Get all lessons for a trigger code.
    #[must_use]
    pub fn get_by_trigger(&self, trigger: &str) -> Vec<&StoredLesson> {
        self.by_trigger.get(trigger).map_or_else(Vec::new, |ids| {
            ids.iter().filter_map(|id| self.lessons.get(id)).collect()
        })
    }

    /// Get all lessons for a scope.
    #[must_use]
    pub fn get_by_scope(&self, scope: &str) -> Vec<&StoredLesson> {
        self.by_scope.get(scope).map_or_else(Vec::new, |ids| {
            ids.iter().filter_map(|id| self.lessons.get(id)).collect()
        })
    }

    /// Get all lessons for a genre.
    #[must_use]
    pub fn get_by_genre(&self, genre: &str) -> Vec<&StoredLesson> {
        self.by_genre.get(genre).map_or_else(Vec::new, |ids| {
            ids.iter().filter_map(|id| self.lessons.get(id)).collect()
        })
    }

    /// Get lessons above a confidence threshold, sorted by confidence descending.
    #[must_use]
    pub fn get_confident_lessons(&self, min_confidence: f32) -> Vec<&StoredLesson> {
        let mut results: Vec<&StoredLesson> = self
            .lessons
            .values()
            .filter(|l| l.confidence >= min_confidence)
            .collect();
        results.sort_by(|a, b| {
            b.confidence
                .partial_cmp(&a.confidence)
                .unwrap_or(std::cmp::Ordering::Equal)
        });
        results
    }

    /// Get the best lesson for a trigger + genre combination.
    ///
    /// Returns the lesson with the highest confidence that matches both
    /// the trigger code and genre (or is genre-agnostic).
    #[must_use]
    pub fn get_best_for_trigger(
        &self,
        trigger: &str,
        genre: Option<&str>,
    ) -> Option<&StoredLesson> {
        self.get_by_trigger(trigger)
            .into_iter()
            .filter(|l| genre.is_none() || l.genre.is_none() || l.genre.as_deref() == genre)
            .max_by(|a, b| {
                a.confidence
                    .partial_cmp(&b.confidence)
                    .unwrap_or(std::cmp::Ordering::Equal)
            })
    }

    /// Update a lesson's success/failure counts and recompute confidence.
    pub fn record_outcome(&mut self, lesson_id: &str, success: bool) -> Option<&StoredLesson> {
        if let Some(lesson) = self.lessons.get_mut(lesson_id) {
            if success {
                lesson.success_count += 1;
                lesson.confidence = (lesson.confidence + 0.1).min(1.0);
            } else {
                lesson.failure_count += 1;
                lesson.confidence = (lesson.confidence - 0.1).max(0.0);
            }
            lesson.updated_at = chrono::Utc::now().to_rfc3339();
        }
        self.lessons.get(lesson_id)
    }

    /// Record a lesson or accumulate evidence into an existing matching lesson.
    ///
    /// Matching key: `(trigger, scope, genre)` — genre uses strict equality,
    /// so a genre-agnostic lesson (`None`) and a genre-specific one
    /// (`Some("techno")`) with the same trigger+scope are distinct lessons;
    /// queries such as [`LessonStore::get_best_for_trigger`] are lenient by
    /// contrast and may match either. When a lesson already exists for that
    /// key, this occurrence merges into it instead of creating a duplicate,
    /// per §13.5 (lessons earn confidence through repeated experience):
    /// - `successful_patch: Some(_)` → success occurrence: `success_count`
    ///   increments and confidence rises by 0.1 (capped at 1.0).
    /// - non-empty `failed_patches` → failure occurrence: `failure_count`
    ///   increments, confidence falls by 0.1 (floored at 0.0), and each
    ///   failed patch is appended to the lesson's `failed_patches`
    ///   (failure-pattern memory) so the same patch is not tried again.
    ///
    /// When no lesson matches the key, the incoming lesson is inserted and the
    /// creating occurrence counts as its first outcome (`success_count` or
    /// `failure_count` starts at 1); the declared confidence is kept as-is.
    /// Returns the stored lesson — the existing one after merging, or the
    /// newly inserted one.
    pub fn upsert(&mut self, lesson: StoredLesson) -> Option<&StoredLesson> {
        let id = lesson.id.clone();
        let trigger = lesson.trigger.clone();
        let scope = lesson.scope.clone();
        let genre = lesson.genre.clone();

        let existing_id = self
            .by_trigger
            .get(&trigger)
            .into_iter()
            .flatten()
            .filter_map(|candidate_id| self.lessons.get(candidate_id))
            .find(|existing| existing.scope == scope && existing.genre == genre)
            .map(|existing| existing.id.clone());

        let Some(existing_id) = existing_id else {
            let mut lesson = lesson;
            if lesson.successful_patch.is_some() {
                lesson.success_count = 1;
            } else if !lesson.failed_patches.is_empty() {
                lesson.failure_count = 1;
            }
            self.record(lesson);
            return self.lessons.get(&id);
        };

        let is_success = lesson.successful_patch.is_some();
        let is_failure = !lesson.failed_patches.is_empty();
        if let Some(existing) = self.lessons.get_mut(&existing_id) {
            if is_success {
                existing.success_count += 1;
                existing.confidence = (existing.confidence + 0.1).min(1.0);
                if existing.successful_patch.is_none() {
                    existing.successful_patch = lesson.successful_patch;
                }
            } else if is_failure {
                existing.failure_count += 1;
                existing.confidence = (existing.confidence - 0.1).max(0.0);
                for patch in lesson.failed_patches {
                    if !existing.failed_patches.contains(&patch) {
                        existing.failed_patches.push(patch);
                    }
                }
            }
            existing.updated_at = chrono::Utc::now().to_rfc3339();
        }
        self.lessons.get(&existing_id)
    }

    /// Get the total number of lessons.
    #[must_use]
    pub fn len(&self) -> usize {
        self.lessons.len()
    }

    /// Whether the store is empty.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.lessons.is_empty()
    }
}

/// Save the lesson store to disk as JSON.
///
/// # Errors
///
/// Returns [`ScorestrataError::Io`] if the file cannot be written.
pub fn save_lesson_store(path: &Path, store: &LessonStore) -> StorageResult<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| ScorestrataError::Io {
            path: parent.display().to_string(),
            source: e,
        })?;
    }
    let data =
        serde_json::to_string_pretty(store).map_err(|e| ScorestrataError::Serialization {
            message: format!("Failed to serialize lesson store: {e}"),
        })?;
    std::fs::write(path, data).map_err(|e| ScorestrataError::Io {
        path: path.display().to_string(),
        source: e,
    })?;
    Ok(())
}

/// Load the lesson store from disk.
///
/// Returns an empty store if the file does not exist.
///
/// # Errors
///
/// Returns [`ScorestrataError::Io`] or [`ScorestrataError::Serialization`]
/// if the file cannot be read or parsed.
pub fn load_lesson_store(path: &Path) -> StorageResult<LessonStore> {
    if !path.exists() {
        return Ok(LessonStore::new());
    }
    let content = std::fs::read_to_string(path).map_err(|e| ScorestrataError::Io {
        path: path.display().to_string(),
        source: e,
    })?;
    let store: LessonStore =
        serde_json::from_str(&content).map_err(|e| ScorestrataError::Serialization {
            message: format!("Failed to parse lesson store: {e}"),
        })?;
    Ok(store)
}

#[cfg(test)]
mod tests {
    use std::sync::atomic::{AtomicU64, Ordering};

    use super::*;

    static TEST_COUNTER: AtomicU64 = AtomicU64::new(1);

    fn next_lesson_id() -> String {
        let n = TEST_COUNTER.fetch_add(1, Ordering::Relaxed);
        format!("les_{n}")
    }

    fn make_lesson(trigger: &str, scope: &str, genre: Option<&str>) -> StoredLesson {
        StoredLesson {
            id: next_lesson_id(),
            scope: scope.to_string(),
            trigger: trigger.to_string(),
            successful_patch: Some("add_transition_package".to_string()),
            failed_patches: vec![],
            evidence_uris: vec![],
            genre: genre.map(str::to_string),
            confidence: 0.5,
            success_count: 0,
            failure_count: 0,
            created_at: chrono::Utc::now().to_rfc3339(),
            updated_at: chrono::Utc::now().to_rfc3339(),
        }
    }

    #[test]
    fn lesson_store_records_and_retrieves() {
        let mut store = LessonStore::new();
        let lesson = make_lesson("SCALE_VIOLATION", "harmony", Some("melodic_techno"));
        let id = lesson.id.clone();
        store.record(lesson);

        assert_eq!(store.len(), 1);
        assert!(store.get(&id).is_some());
        assert_eq!(store.get_by_trigger("SCALE_VIOLATION").len(), 1);
        assert_eq!(store.get_by_scope("harmony").len(), 1);
        assert_eq!(store.get_by_genre("melodic_techno").len(), 1);
    }

    #[test]
    #[expect(clippy::unwrap_used, reason = "Test should panic on unexpected error")]
    fn lesson_confidence_updates_on_outcome() {
        let mut store = LessonStore::new();
        let lesson = make_lesson("TRANSITION_ENERGY_LOW", "transition", None);
        let id = lesson.id.clone();
        store.record(lesson);

        store.record_outcome(&id, true);
        store.record_outcome(&id, true);
        let lesson = store.get(&id).unwrap();
        assert_eq!(lesson.success_count, 2);
        assert!(lesson.confidence > 0.5);

        store.record_outcome(&id, false);
        let lesson = store.get(&id).unwrap();
        assert_eq!(lesson.failure_count, 1);
    }
    #[test]
    #[expect(clippy::unwrap_used, reason = "Test should panic on unexpected error")]
    fn upsert_creates_new_lesson_when_no_match() {
        let mut store = LessonStore::new();
        let lesson = make_lesson("SCALE_VIOLATION", "harmony", Some("melodic_techno"));
        store.upsert(lesson);

        assert_eq!(store.len(), 1);
        // The creating occurrence counts as the first outcome.
        let stored = store
            .get_best_for_trigger("SCALE_VIOLATION", Some("melodic_techno"))
            .unwrap();
        assert_eq!(stored.success_count, 1);
        assert_eq!(stored.failure_count, 0);
    }

    #[test]
    #[expect(clippy::unwrap_used, reason = "Test should panic on unexpected error")]
    fn upsert_accumulates_confidence_on_repeated_success() {
        let mut store = LessonStore::new();
        store.upsert(make_lesson("TRANSITION_ENERGY_LOW", "transition", None));
        store.upsert(make_lesson("TRANSITION_ENERGY_LOW", "transition", None));

        // Repeated success on the same key merges instead of duplicating.
        assert_eq!(store.len(), 1);
        let stored = store
            .get_best_for_trigger("TRANSITION_ENERGY_LOW", None)
            .unwrap();
        assert_eq!(stored.success_count, 2);
        assert!((stored.confidence - 0.6).abs() < 0.01);
    }

    #[test]
    #[expect(clippy::unwrap_used, reason = "Test should panic on unexpected error")]
    fn upsert_records_failure_patterns_and_lowers_confidence() {
        let mut store = LessonStore::new();
        store.upsert(make_lesson("TRANSITION_ENERGY_LOW", "transition", None));

        // A failed attempt of the same trigger/scope/genre with a different
        // patch records the failure pattern and lowers confidence.
        let mut failure = make_lesson("TRANSITION_ENERGY_LOW", "transition", None);
        failure.successful_patch = None;
        failure.failed_patches = vec!["boost_transition_energy".to_string()];
        store.upsert(failure);

        assert_eq!(store.len(), 1);
        let stored = store
            .get_best_for_trigger("TRANSITION_ENERGY_LOW", None)
            .unwrap();
        assert_eq!(stored.success_count, 1);
        assert_eq!(stored.failure_count, 1);
        assert!(
            stored
                .failed_patches
                .contains(&"boost_transition_energy".to_string())
        );
        assert!((stored.confidence - 0.4).abs() < 0.01);
    }

    #[test]
    fn upsert_distinguishes_by_genre() {
        let mut store = LessonStore::new();
        store.upsert(make_lesson("SCALE_VIOLATION", "harmony", Some("ambient")));
        store.upsert(make_lesson(
            "SCALE_VIOLATION",
            "harmony",
            Some("melodic_techno"),
        ));

        // Same trigger + scope but different genre is a different lesson.
        assert_eq!(store.len(), 2);
        assert_eq!(store.get_by_genre("ambient").len(), 1);
        assert_eq!(store.get_by_genre("melodic_techno").len(), 1);
    }

    #[test]
    #[expect(clippy::unwrap_used, reason = "Test should panic on unexpected error")]
    fn best_lesson_for_trigger_respects_genre() {
        let mut store = LessonStore::new();
        let mut l1 = make_lesson("SCALE_VIOLATION", "harmony", Some("ambient"));
        l1.confidence = 0.9;
        let mut l2 = make_lesson("SCALE_VIOLATION", "harmony", None);
        l2.confidence = 0.6;
        store.record(l1);
        store.record(l2);

        // Should prefer the genre-specific lesson
        let best = store.get_best_for_trigger("SCALE_VIOLATION", Some("ambient"));
        assert!(best.is_some());
        assert!((best.unwrap().confidence - 0.9).abs() < 0.01);
    }

    #[test]
    fn confident_lessons_filtered_correctly() {
        let mut store = LessonStore::new();
        let mut l1 = make_lesson("A", "s1", None);
        l1.confidence = 0.9;
        let mut l2 = make_lesson("B", "s1", None);
        l2.confidence = 0.3;
        store.record(l1);
        store.record(l2);

        let confident = store.get_confident_lessons(0.5);
        assert_eq!(confident.len(), 1);
        assert!((confident[0].confidence - 0.9).abs() < 0.01);
    }

    #[test]
    #[expect(clippy::unwrap_used, reason = "Test should panic on unexpected error")]
    fn lesson_store_roundtrip() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("lessons.json");

        let mut store = LessonStore::new();
        store.record(make_lesson("SCALE_VIOLATION", "harmony", Some("techno")));
        store.record(make_lesson("TRANSITION_LOW", "transition", None));

        save_lesson_store(&path, &store).unwrap();
        let loaded = load_lesson_store(&path).unwrap();
        assert_eq!(loaded.len(), 2);
    }

    #[test]
    #[expect(clippy::unwrap_used, reason = "Test should panic on unexpected error")]
    fn load_nonexistent_returns_empty() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("nonexistent.json");
        let store = load_lesson_store(&path).unwrap();
        assert!(store.is_empty());
    }
}
