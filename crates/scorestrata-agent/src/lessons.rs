//! Learning-loop bridge (§13.5).
//!
//! Closes the loop between the agent kernel's in-memory
//! [`RecordedLesson`](crate::kernel::RecordedLesson) events and the
//! persistent [`LessonStore`](scorestrata_storage::lesson_store::LessonStore):
//! replays kernel lessons through [`LessonStore::upsert`] so repeated
//! experience with the same diagnostic accumulates confidence and failed
//! patches are remembered as failure patterns.

use scorestrata_storage::lesson_store::{LessonStore, StoredLesson};

use crate::kernel::RecordedLesson;

/// Export kernel lessons into a lesson store.
///
/// Each [`RecordedLesson`] is mapped to a [`StoredLesson`] and merged via
/// [`LessonStore::upsert`]: successful patches raise confidence, failed
/// patches lower it and are recorded as failure-pattern memory. Lessons
/// matching an existing `(trigger, scope, genre)` key accumulate into it
/// rather than duplicating. Returns the number of lessons exported.
///
/// `genre` scopes the lesson to a genre (or `None` for genre-agnostic);
/// `project_id` is used for evidence URIs and lesson IDs.
///
/// Lesson IDs are derived from the `(trigger, scope, genre)` merge key so
/// that re-exporting the same lesson across sessions never collides with a
/// different lesson (a counter/iteration-based id would reset per session and
/// corrupt the store's trigger/scope indexes).
pub fn export_lessons_to_store(
    lessons: &[RecordedLesson],
    store: &mut LessonStore,
    genre: Option<&str>,
    project_id: &str,
) -> usize {
    let now = chrono::Utc::now().to_rfc3339();
    let evidence_uri = format!("project://{project_id}/memory/lessons");
    let genre_key = genre.unwrap_or("any");

    for lesson in lessons {
        let (successful_patch, failed_patches) = if lesson.successful {
            (Some(lesson.applied_patch.clone()), Vec::new())
        } else {
            (None, vec![lesson.applied_patch.clone()])
        };

        let stored = StoredLesson {
            id: format!(
                "les_{project_id}_{}_{}_{}",
                lesson.diagnosis_metric, lesson.scope, genre_key
            ),
            scope: lesson.scope.clone(),
            trigger: lesson.diagnosis_metric.clone(),
            successful_patch,
            failed_patches,
            evidence_uris: vec![evidence_uri.clone()],
            genre: genre.map(str::to_string),
            confidence: 0.5,
            success_count: 0,
            failure_count: 0,
            created_at: now.clone(),
            updated_at: now.clone(),
        };
        store.upsert(stored);
    }

    lessons.len()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::kernel::{AgentKernel, Candidate, Diagnosis};
    use scorestrata_core::ids::ProjectId;

    fn sample_diagnosis(metric: &str, scope: &str) -> Diagnosis {
        Diagnosis {
            metric: metric.to_string(),
            scope: scope.to_string(),
            current_value: 0.3,
            target_value: 0.8,
            severity: 0.7,
            is_hard_failure: false,
            evidence_uri: None,
        }
    }

    fn sample_candidate(operation: &str, scope: &str) -> Candidate {
        Candidate {
            id: "c1".to_string(),
            operation: operation.to_string(),
            description: "test".to_string(),
            scope: scope.to_string(),
            diagnosis_metric: "scale_violation".to_string(),
            estimated_delta: 0.05,
            is_repair: false,
            template_id: None,
        }
    }

    #[test]
    #[expect(clippy::unwrap_used, reason = "Test should panic on unexpected error")]
    fn export_should_populate_store_from_kernel_lessons() {
        let mut kernel = AgentKernel::new(ProjectId::new());
        let diagnosis = sample_diagnosis("scale_violation", "harmony");
        let candidate = sample_candidate("quantize_to_scale", "harmony");

        // Two successes + one failure on the same key, using a distinct patch
        // for the failure so failure-pattern memory is unambiguous.
        kernel.l4_learn(&diagnosis, &candidate, 0.1);
        kernel.l4_learn(&diagnosis, &candidate, 0.2);
        let failure_candidate = Candidate {
            id: "c2".to_string(),
            operation: "force_scale_notes".to_string(),
            ..candidate.clone()
        };
        kernel.l4_learn(&diagnosis, &failure_candidate, -0.1);

        let mut store = LessonStore::new();
        let exported = export_lessons_to_store(
            &kernel.lessons_learned,
            &mut store,
            Some("melodic_techno"),
            "proj_1",
        );

        assert_eq!(exported, 3);
        // All three merge into one keyed lesson.
        assert_eq!(store.len(), 1);
        let lesson = store
            .get_best_for_trigger("scale_violation", Some("melodic_techno"))
            .unwrap();
        assert_eq!(lesson.success_count, 2);
        assert_eq!(lesson.failure_count, 1);
        assert!(
            lesson
                .failed_patches
                .contains(&"force_scale_notes".to_string())
        );
        assert!(
            !lesson
                .failed_patches
                .contains(&"quantize_to_scale".to_string()),
            "the successful patch must not be recorded as a failure pattern"
        );
        // Insert at 0.5, +0.1 for the second success, −0.1 for the failure.
        assert!((lesson.confidence - 0.5).abs() < 0.01);
    }

    #[test]
    fn export_should_distinguish_scope_keys() {
        let mut kernel = AgentKernel::new(ProjectId::new());
        kernel.l4_learn(
            &sample_diagnosis("scale_violation", "harmony"),
            &sample_candidate("quantize_to_scale", "harmony"),
            0.1,
        );
        kernel.l4_learn(
            &sample_diagnosis("loudness_flat", "mix"),
            &sample_candidate("boost_highs", "mix"),
            0.1,
        );

        let mut store = LessonStore::new();
        export_lessons_to_store(&kernel.lessons_learned, &mut store, None, "proj_1");

        assert_eq!(store.len(), 2);
        assert_eq!(store.get_by_scope("harmony").len(), 1);
        assert_eq!(store.get_by_scope("mix").len(), 1);
    }

    #[test]
    fn export_across_sessions_does_not_collide_ids() {
        // Session 1: a lesson for one trigger at iteration 0.
        let mut store = LessonStore::new();
        let mut kernel_one = AgentKernel::new(ProjectId::new());
        kernel_one.l4_learn(
            &sample_diagnosis("scale_violation", "harmony"),
            &sample_candidate("quantize_to_scale", "harmony"),
            0.1,
        );
        export_lessons_to_store(&kernel_one.lessons_learned, &mut store, None, "proj_1");

        // Session 2: a DIFFERENT trigger also exported at iteration 0 — its
        // id must not collide with session 1's (which would corrupt the
        // trigger index via `record`'s map overwrite).
        let mut kernel_two = AgentKernel::new(ProjectId::new());
        kernel_two.l4_learn(
            &sample_diagnosis("loudness_flat", "mix"),
            &sample_candidate("boost_highs", "mix"),
            0.1,
        );
        export_lessons_to_store(&kernel_two.lessons_learned, &mut store, None, "proj_1");

        assert_eq!(store.len(), 2);
        // Each trigger's index must still resolve to the correct lesson.
        assert_eq!(store.get_by_trigger("scale_violation").len(), 1);
        assert_eq!(store.get_by_trigger("loudness_flat").len(), 1);
        let first = &store.get_by_trigger("scale_violation")[0];
        assert_eq!(first.successful_patch.as_deref(), Some("quantize_to_scale"));
    }

    #[test]
    fn export_empty_lessons_is_noop() {
        let mut store = LessonStore::new();
        let exported = export_lessons_to_store(&[], &mut store, None, "proj_1");
        assert_eq!(exported, 0);
        assert!(store.is_empty());
    }
}
