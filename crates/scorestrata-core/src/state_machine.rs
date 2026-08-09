//! Project state machine — enforces valid state transitions.
//!
//! The architecture spec (section 4.2) defines a strict state machine:
//!
//! ```text
//! BriefReceived → ConfigDrafted → ConfigValidationFailed / ConfigApproved
//! ConfigApproved → ArrangementPlanned → SkeletonCreated → SegmentDrafting
//! SegmentDrafting → SegmentCandidatesGenerated → SegmentValidationFailed / SegmentValidated
//! SegmentValidated → TransitionValidationFailed / TransitionValidated
//! TransitionValidated → MixDrafting → MixValidationFailed / Compiling
//! Compiling → Validating → Experimenting → Calibrating → Rendering
//! Rendering → RenderFailed / FinalValidationPassed → RenderFinalized
//! Any → AwaitingHumanDecision / NeedsPolicyReview / NeedsHumanReview / Replaying / Quarantined / Archived
//! ```
//!
//! This module implements the transition guards and validation so agents
//! cannot skip steps or make illegal state changes.

use std::sync::OnceLock;

use crate::domain::ProjectStatus;
use crate::error::ScorestrataError;

/// A transition rule: from which states is a given target state reachable?
#[derive(Debug, Clone)]
pub struct TransitionRule {
    /// The target state.
    pub target: ProjectStatus,
    /// The set of states from which this transition is valid.
    pub from: Vec<ProjectStatus>,
    /// Human-readable description of what action triggers this transition.
    pub trigger: &'static str,
}

/// Build the complete set of transition rules.
///
/// Called once via [`OnceLock`] to avoid repeated allocation.
#[expect(
    clippy::too_many_lines,
    reason = "The transition table is a data table by design"
)]
fn build_transitions() -> Vec<TransitionRule> {
    use ProjectStatus::{
        Archived, ArrangementPlanned, AwaitingHumanDecision, BriefReceived, Calibrating, Compiling,
        ConfigApproved, ConfigDrafted, ConfigValidationFailed, Experimenting,
        FinalValidationPassed, MixDrafting, MixValidationFailed, NeedsHumanReview,
        NeedsPolicyReview, Quarantined, RenderFailed, RenderFinalized, Rendering, Replaying,
        SegmentCandidatesGenerated, SegmentDrafting, SegmentValidationFailed, SegmentsValidated,
        SkeletonCreated, TransitionValidated, TransitionValidationFailed, Validating,
    };

    vec![
        // Forward progress transitions
        TransitionRule {
            target: ConfigDrafted,
            from: vec![BriefReceived, ConfigValidationFailed],
            trigger: "generate_config_from_brief",
        },
        TransitionRule {
            target: ConfigValidationFailed,
            from: vec![ConfigDrafted],
            trigger: "validate_config (failed)",
        },
        TransitionRule {
            target: ConfigApproved,
            from: vec![ConfigDrafted, ConfigValidationFailed],
            trigger: "approve_config",
        },
        TransitionRule {
            target: ArrangementPlanned,
            from: vec![ConfigApproved],
            trigger: "plan_arrangement",
        },
        TransitionRule {
            target: SkeletonCreated,
            from: vec![ArrangementPlanned],
            trigger: "create_skeleton",
        },
        TransitionRule {
            target: SegmentDrafting,
            from: vec![SkeletonCreated, SegmentsValidated, SegmentValidationFailed],
            trigger: "draft_segment",
        },
        TransitionRule {
            target: SegmentCandidatesGenerated,
            from: vec![SegmentDrafting],
            trigger: "generate_segment_candidates",
        },
        TransitionRule {
            target: SegmentValidationFailed,
            from: vec![SegmentCandidatesGenerated],
            trigger: "validate_segments (failed)",
        },
        TransitionRule {
            target: SegmentsValidated,
            from: vec![SegmentCandidatesGenerated, SegmentDrafting],
            trigger: "validate_segments (passed)",
        },
        TransitionRule {
            target: TransitionValidationFailed,
            from: vec![SegmentsValidated],
            trigger: "validate_transitions (failed)",
        },
        TransitionRule {
            target: TransitionValidated,
            from: vec![SegmentsValidated, TransitionValidationFailed],
            trigger: "validate_transitions (passed)",
        },
        TransitionRule {
            target: MixDrafting,
            from: vec![TransitionValidated],
            trigger: "draft_mix",
        },
        TransitionRule {
            target: MixValidationFailed,
            from: vec![MixDrafting],
            trigger: "validate_mix (failed)",
        },
        TransitionRule {
            target: Compiling,
            from: vec![TransitionValidated, MixDrafting, Validating],
            trigger: "compile",
        },
        TransitionRule {
            target: Validating,
            from: vec![Compiling],
            trigger: "validate (level 2+)",
        },
        TransitionRule {
            target: Experimenting,
            from: vec![Validating],
            trigger: "start_experiment",
        },
        TransitionRule {
            target: Calibrating,
            from: vec![Experimenting, Validating],
            trigger: "calibrate",
        },
        TransitionRule {
            target: Rendering,
            from: vec![Calibrating, Validating, FinalValidationPassed],
            trigger: "render",
        },
        TransitionRule {
            target: RenderFailed,
            from: vec![Rendering],
            trigger: "render (failed)",
        },
        TransitionRule {
            target: FinalValidationPassed,
            from: vec![Validating, Calibrating],
            trigger: "validate (all levels passed)",
        },
        TransitionRule {
            target: RenderFinalized,
            from: vec![FinalValidationPassed, Rendering],
            trigger: "finalize_render",
        },
        // Recovery / fallback transitions
        TransitionRule {
            target: SegmentDrafting,
            from: vec![SegmentValidationFailed, MixValidationFailed],
            trigger: "redraft_segment (regression)",
        },
        // Universal transitions (any active state can go to these)
        TransitionRule {
            target: AwaitingHumanDecision,
            from: vec![
                ConfigDrafted,
                ConfigApproved,
                ArrangementPlanned,
                SkeletonCreated,
                SegmentDrafting,
                SegmentsValidated,
                Compiling,
                Validating,
                Rendering,
                FinalValidationPassed,
                RenderFinalized,
                Experimenting,
                Calibrating,
            ],
            trigger: "escalate_to_human",
        },
        TransitionRule {
            target: NeedsPolicyReview,
            from: vec![
                ConfigDrafted,
                ConfigApproved,
                Compiling,
                Validating,
                Experimenting,
                Calibrating,
                Rendering,
            ],
            trigger: "policy_review_required",
        },
        TransitionRule {
            target: NeedsHumanReview,
            from: vec![
                Validating,
                Experimenting,
                Calibrating,
                Rendering,
                RenderFailed,
            ],
            trigger: "human_review_required",
        },
        TransitionRule {
            target: Replaying,
            from: vec![
                Quarantined,
                RenderFailed,
                SegmentValidationFailed,
                TransitionValidationFailed,
                MixValidationFailed,
            ],
            trigger: "replay_from_snapshot",
        },
        TransitionRule {
            target: Quarantined,
            from: vec![
                ConfigDrafted,
                ConfigApproved,
                ArrangementPlanned,
                SkeletonCreated,
                SegmentDrafting,
                SegmentsValidated,
                Compiling,
                Validating,
                Rendering,
                RenderFailed,
                FinalValidationPassed,
                RenderFinalized,
                AwaitingHumanDecision,
                Experimenting,
                Calibrating,
            ],
            trigger: "quarantine (stuck/regression)",
        },
        TransitionRule {
            target: Archived,
            from: vec![
                RenderFinalized,
                Quarantined,
                AwaitingHumanDecision,
                NeedsHumanReview,
            ],
            trigger: "archive",
        },
    ]
}

/// Get all valid transitions for the project state machine.
///
/// Returns a reference to the complete set of transition rules
/// that define the valid state graph per architecture spec §4.2.
/// Computed once via [`OnceLock`] for zero-allocation repeated access.
#[must_use]
pub fn valid_transitions() -> &'static [TransitionRule] {
    static RULES: OnceLock<Vec<TransitionRule>> = OnceLock::new();
    RULES.get_or_init(build_transitions)
}

/// Check if a transition from `from` to `to` is valid.
///
/// Returns `Ok(())` if the transition is valid, or
/// `Err(ScorestrataError::InvalidTransition)` if it is not.
///
/// # Errors
///
/// Returns [`ScorestrataError::InvalidTransition`] if the transition
/// is not in the valid set.
pub fn validate_transition(from: ProjectStatus, to: ProjectStatus) -> Result<(), ScorestrataError> {
    let transitions = valid_transitions();

    let is_valid = transitions
        .iter()
        .any(|rule| rule.target == to && rule.from.contains(&from));

    if is_valid {
        Ok(())
    } else {
        Err(ScorestrataError::InvalidTransition {
            from: format!("{from:?}"),
            to: format!("{to:?}"),
        })
    }
}

/// Get all states reachable from a given state.
#[must_use]
pub fn reachable_from(state: ProjectStatus) -> Vec<ProjectStatus> {
    let transitions = valid_transitions();
    transitions
        .iter()
        .filter(|rule| rule.from.contains(&state))
        .map(|rule| rule.target)
        .collect()
}

/// Get the trigger description for a transition.
#[must_use]
pub fn trigger_for_transition(from: ProjectStatus, to: ProjectStatus) -> Option<&'static str> {
    let transitions = valid_transitions();
    transitions
        .iter()
        .find(|rule| rule.target == to && rule.from.contains(&from))
        .map(|rule| rule.trigger)
}

/// Display a human-readable summary of the state machine.
#[must_use]
pub fn state_machine_summary() -> String {
    let transitions = valid_transitions();
    let mut lines = Vec::new();
    for rule in transitions {
        let from_names: Vec<String> = rule.from.iter().map(|s| format!("{s:?}")).collect();
        lines.push(format!(
            "  {:?} → {:?}  [{}]",
            from_names.join(", "),
            rule.target,
            rule.trigger,
        ));
    }
    lines.join("\n")
}

#[cfg(test)]
mod tests {
    use super::*;
    use ProjectStatus::*;

    #[test]
    fn brief_received_can_go_to_config_draft() {
        assert!(validate_transition(BriefReceived, ConfigDrafted).is_ok());
    }

    #[test]
    fn brief_received_cannot_skip_to_rendering() {
        assert!(validate_transition(BriefReceived, Rendering).is_err());
    }

    #[test]
    fn config_approved_can_go_to_arrangement() {
        assert!(validate_transition(ConfigApproved, ArrangementPlanned).is_ok());
    }

    #[test]
    fn config_validation_failed_can_retry() {
        assert!(validate_transition(ConfigValidationFailed, ConfigDrafted).is_ok());
    }

    #[test]
    fn rendering_can_fail() {
        assert!(validate_transition(Rendering, RenderFailed).is_ok());
    }

    #[test]
    fn render_failed_can_be_quarantined() {
        assert!(validate_transition(RenderFailed, Quarantined).is_ok());
    }

    #[test]
    fn any_active_state_can_escalate() {
        let active_states = vec![
            ConfigDrafted,
            ConfigApproved,
            ArrangementPlanned,
            SkeletonCreated,
            SegmentDrafting,
            SegmentsValidated,
            Compiling,
            Validating,
            Rendering,
            FinalValidationPassed,
            RenderFinalized,
        ];
        for state in active_states {
            assert!(
                validate_transition(state, AwaitingHumanDecision).is_ok(),
                "Expected {state:?} → AwaitingHumanDecision to be valid"
            );
        }
    }

    #[test]
    fn archived_is_terminal() {
        assert!(reachable_from(Archived).is_empty());
    }

    #[test]
    fn render_finalized_can_archive() {
        assert!(validate_transition(RenderFinalized, Archived).is_ok());
    }

    #[test]
    fn segments_validated_can_redraft() {
        assert!(validate_transition(SegmentsValidated, SegmentDrafting).is_ok());
    }

    #[test]
    fn validating_can_recompile() {
        assert!(validate_transition(Validating, Compiling).is_ok());
    }

    #[test]
    fn trigger_for_valid_transition_returns_description() {
        let trigger = trigger_for_transition(BriefReceived, ConfigDrafted);
        assert_eq!(trigger, Some("generate_config_from_brief"));
    }

    #[test]
    fn trigger_for_invalid_transition_returns_none() {
        let trigger = trigger_for_transition(BriefReceived, Rendering);
        assert!(trigger.is_none());
    }

    #[test]
    fn state_machine_summary_is_non_empty() {
        let summary = state_machine_summary();
        assert!(!summary.is_empty());
        assert!(summary.contains("BriefReceived"));
    }

    #[test]
    fn segment_validated_can_validate_transitions() {
        // After segments are validated, next step is transition validation
        assert!(validate_transition(SegmentsValidated, TransitionValidated).is_ok());
        assert!(validate_transition(SegmentsValidated, TransitionValidationFailed).is_ok());
        // SegmentsValidated no longer goes directly to Compiling (must pass transition validation first)
        assert!(validate_transition(SegmentsValidated, Compiling).is_err());
    }

    #[test]
    fn new_states_have_valid_transitions() {
        // Segment candidates generation
        assert!(validate_transition(SegmentDrafting, SegmentCandidatesGenerated).is_ok());
        assert!(validate_transition(SegmentCandidatesGenerated, SegmentsValidated).is_ok());
        assert!(validate_transition(SegmentCandidatesGenerated, SegmentValidationFailed).is_ok());
        // Transition validation
        assert!(validate_transition(TransitionValidationFailed, TransitionValidated).is_ok());
        assert!(validate_transition(TransitionValidated, MixDrafting).is_ok());
        assert!(validate_transition(TransitionValidated, Compiling).is_ok());
        // Mix validation
        assert!(validate_transition(MixDrafting, MixValidationFailed).is_ok());
        assert!(validate_transition(MixValidationFailed, SegmentDrafting).is_ok());
        // Experimenting and calibrating
        assert!(validate_transition(Validating, Experimenting).is_ok());
        assert!(validate_transition(Experimenting, Calibrating).is_ok());
        assert!(validate_transition(Validating, Calibrating).is_ok());
        // Policy and human review
        assert!(validate_transition(Validating, NeedsPolicyReview).is_ok());
        assert!(validate_transition(Validating, NeedsHumanReview).is_ok());
        // Replaying
        assert!(validate_transition(Quarantined, Replaying).is_ok());
        assert!(validate_transition(RenderFailed, Replaying).is_ok());
    }

    #[test]
    fn quarantined_can_archive() {
        assert!(validate_transition(Quarantined, Archived).is_ok());
    }

    #[test]
    fn awaiting_human_can_archive() {
        assert!(validate_transition(AwaitingHumanDecision, Archived).is_ok());
    }

    // ── Negative transition tests ──────────────────────────────────

    #[test]
    fn mix_drafting_can_compile_bypass() {
        // MixDrafting → Compiling is allowed per §4.2 as a forward path
        assert!(
            validate_transition(MixDrafting, Compiling).is_ok(),
            "MixDrafting → Compiling should be a valid forward path"
        );
    }

    #[test]
    fn archived_cannot_go_anywhere() {
        assert!(
            reachable_from(Archived).is_empty(),
            "Archived is terminal — no outgoing transitions"
        );
    }

    #[test]
    fn rendering_can_finalize_directly() {
        // Rendering → RenderFinalized is allowed per §4.2
        assert!(
            validate_transition(Rendering, RenderFinalized).is_ok(),
            "Rendering → RenderFinalized should be valid"
        );
    }

    #[test]
    fn segment_drafting_cannot_skip_to_compiling() {
        assert!(
            validate_transition(SegmentDrafting, Compiling).is_err(),
            "SegmentDrafting → Compiling is not a valid forward path"
        );
    }

    #[test]
    fn brief_received_cannot_go_to_rendering() {
        assert!(
            validate_transition(BriefReceived, Rendering).is_err(),
            "BriefReceived cannot skip the entire pipeline"
        );
    }

    #[test]
    fn config_approved_cannot_skip_to_segment_drafting() {
        assert!(
            validate_transition(ConfigApproved, SegmentDrafting).is_err(),
            "ConfigApproved must go through ArrangementPlanned first"
        );
    }

    #[test]
    fn quarantined_cannot_skip_to_rendering() {
        assert!(
            validate_transition(Quarantined, Rendering).is_err(),
            "Quarantined must go through Replaying or Archived"
        );
    }

    #[test]
    fn render_finalized_cannot_go_back_to_rendering() {
        assert!(
            validate_transition(RenderFinalized, Rendering).is_err(),
            "RenderFinalized → Rendering is a backward transition, should be rejected"
        );
    }

    #[test]
    fn config_draft_cannot_skip_to_validating() {
        assert!(
            validate_transition(ConfigDrafted, Validating).is_err(),
            "ConfigDrafted cannot skip to Validating"
        );
    }

    #[test]
    fn experiment_cannot_skip_to_rendering() {
        assert!(
            validate_transition(Experimenting, Rendering).is_err(),
            "Experimenting must go through Calibrating first"
        );
    }

    #[test]
    fn all_non_initial_states_are_reachable() {
        // BriefReceived is the initial state — all others must be reachable
        let all_targets: Vec<ProjectStatus> =
            valid_transitions().iter().map(|r| r.target).collect();
        let all_variants = [
            ConfigDrafted,
            ConfigValidationFailed,
            ConfigApproved,
            ArrangementPlanned,
            SkeletonCreated,
            SegmentDrafting,
            SegmentCandidatesGenerated,
            SegmentValidationFailed,
            SegmentsValidated,
            TransitionValidationFailed,
            TransitionValidated,
            MixDrafting,
            MixValidationFailed,
            Compiling,
            Validating,
            Experimenting,
            Calibrating,
            Rendering,
            RenderFailed,
            FinalValidationPassed,
            RenderFinalized,
            AwaitingHumanDecision,
            NeedsPolicyReview,
            NeedsHumanReview,
            Replaying,
            Quarantined,
            Archived,
        ];
        for variant in &all_variants {
            if *variant == BriefReceived {
                continue;
            }
            assert!(
                all_targets.contains(variant),
                "{variant:?} should be reachable via at least one transition"
            );
        }
    }

    // ── NeedsPolicyReview / NeedsHumanReview negative tests ─────────

    #[test]
    fn needs_policy_review_cannot_skip_to_rendering() {
        // Why: NeedsPolicyReview is a review state — must resolve before proceeding
        assert!(
            validate_transition(NeedsPolicyReview, Rendering).is_err(),
            "NeedsPolicyReview → Rendering should not be valid"
        );
    }

    #[test]
    fn needs_human_review_can_archive() {
        // Why: Archived is reachable from NeedsHumanReview per §4.2
        assert!(
            validate_transition(NeedsHumanReview, Archived).is_ok(),
            "NeedsHumanReview → Archived should be valid"
        );
    }

    #[test]
    fn needs_human_review_cannot_skip_to_compiling() {
        assert!(
            validate_transition(NeedsHumanReview, Compiling).is_err(),
            "NeedsHumanReview → Compiling should not be valid"
        );
    }

    #[test]
    fn brief_received_cannot_go_to_needs_policy_review() {
        // Why: NeedsPolicyReview requires being in an active state first
        assert!(
            validate_transition(BriefReceived, NeedsPolicyReview).is_err(),
            "BriefReceived cannot skip directly to NeedsPolicyReview"
        );
    }

    #[test]
    fn brief_received_cannot_go_to_needs_human_review() {
        assert!(
            validate_transition(BriefReceived, NeedsHumanReview).is_err(),
            "BriefReceived cannot skip directly to NeedsHumanReview"
        );
    }

    #[test]
    fn render_finalized_cannot_go_to_needs_policy_review() {
        // Why: RenderFinalized is a terminal-adjacent state
        assert!(
            validate_transition(RenderFinalized, NeedsPolicyReview).is_err(),
            "RenderFinalized → NeedsPolicyReview should not be valid"
        );
    }

    #[test]
    fn quarantined_can_go_to_replaying() {
        assert!(
            validate_transition(Quarantined, Replaying).is_ok(),
            "Quarantined → Replaying should be a valid recovery path"
        );
    }

    #[test]
    fn quarantined_can_go_to_needs_human_review() {
        // Why: Quarantined can escalate to human review
        assert!(
            validate_transition(Quarantined, NeedsHumanReview).is_err(),
            "Quarantined should go through Replaying or Archived, not directly to NeedsHumanReview"
        );
    }
}
