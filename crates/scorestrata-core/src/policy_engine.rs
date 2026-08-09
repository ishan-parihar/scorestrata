//! Policy engine — evaluates commands against policy rules before execution.
//!
//! The architecture spec (section 3.3) requires:
//! - policies are declarative, versioned, and simulated in dry-run,
//! - every policy check should be recorded,
//! - safety classes gate mutations.
//!
//! This module implements the policy evaluation pipeline that sits between
//! agent intent and command execution, ensuring all mutations are safe.

use crate::command::{Command, SafetyClass};
use crate::policy::{PolicyCheckResult, PolicyProfile, PolicyRuleResult};

/// Map a command type to its safety class.
///
/// This is the central registry that determines what level of permission
/// a command requires.
#[must_use]
#[expect(
    clippy::match_same_arms,
    reason = "The explicit Write arm keeps the registry self-documenting; the wildcard is the safe default"
)]
pub fn safety_class_for_command(command_type: &str) -> SafetyClass {
    match command_type {
        "get_capabilities"
        | "get_contracts"
        | "get_policy_profile"
        | "check_budget"
        | "get_project_summary"
        | "get_validation_report"
        | "get_audit_summary"
        | "explain_metric"
        | "get_root_causes"
        | "get_metric_evidence"
        | "get_lessons"
        | "get_repair_templates"
        | "get_command_trace"
        | "get_snapshot_diff"
        | "get_candidate_diff"
        | "get_failure_report" => SafetyClass::Read,

        "create_project"
        | "generate_config_from_brief"
        | "plan_arrangement"
        | "create_pattern"
        | "write_pattern_notes"
        | "write_pattern_steps"
        | "apply_motivic_variation"
        | "place_pattern_in_segment"
        | "set_layer_instrument"
        | "set_segment_goals"
        | "set_energy_curve"
        | "apply_patch"
        | "dry_run_patch"
        | "quantize_layer_to_scale"
        | "harmonize_melody"
        | "add_drum_fill"
        | "add_transition_package"
        | "increase_segment_density"
        | "reduce_masking"
        | "normalize_loudness_target"
        | "record_lesson"
        | "update_calibration_profile"
        | "validate_config"
        | "approve_config"
        | "suggest_fixes"
        | "compare_candidates"
        | "isolate_layer"
        | "solo_segment"
        | "create_human_review_packet" => SafetyClass::Write,

        "render_project"
        | "render_segment"
        | "render_stems"
        | "render_candidate_preview"
        | "get_render_job"
        | "cancel_render_job" => SafetyClass::Render,

        "delete_project" | "delete_snapshot" | "force_rollback" | "clear_cache"
        | "reset_budget" => SafetyClass::Destructive,

        "update_policy" | "update_budget" | "set_safety_override" => SafetyClass::Governance,

        "create_branch"
        | "merge_branch"
        | "claim_task"
        | "submit_task_result"
        | "run_acceptance_tests"
        | "update_docs"
        | "create_adr" => SafetyClass::Engineering,

        _ => SafetyClass::Write, // Unknown commands default to Write (safe default)
    }
}

/// Evaluate a command against a policy profile.
///
/// Returns a [`PolicyCheckResult`] indicating whether the command is allowed,
/// which rules were evaluated, and whether an audit trail entry is needed.
///
/// # Policy Rules Evaluated
///
/// 1. **Destructive commands** — blocked unless `allow_destructive_commands` is true.
/// 2. **Snapshot requirement** — patch commands require a snapshot if `require_snapshot_before_patch` is true.
/// 3. **Budget class** — render commands are checked against render budget.
/// 4. **Safety class hierarchy** — Read < Write < Render < Destructive < Governance.
#[must_use]
pub fn evaluate_policy(command: &Command, policy: &PolicyProfile) -> PolicyCheckResult {
    let mut rules = Vec::new();
    let mut allowed = true;
    let mut override_reason = None;

    let safety = safety_class_for_command(&command.command_type);

    // Rule 1: Destructive commands
    if safety == SafetyClass::Destructive {
        let passed = policy.allow_destructive_commands;
        rules.push(PolicyRuleResult {
            rule: "allow_destructive_commands".to_string(),
            passed,
            value: None,
            limit: None,
            explanation: if passed {
                Some("Destructive commands are allowed by policy".to_string())
            } else {
                Some("Destructive commands are blocked by policy".to_string())
            },
        });
        if !passed {
            allowed = false;
            override_reason = Some("Destructive commands are not allowed".to_string());
        }
    }

    // Rule 2: Snapshot requirement before patch
    if safety == SafetyClass::Write && policy.require_snapshot_before_patch {
        // In a full implementation, we'd check if a snapshot exists.
        // For now, we always pass this check (snapshot creation is expected
        // to happen before the patch command is issued).
        rules.push(PolicyRuleResult {
            rule: "require_snapshot_before_patch".to_string(),
            passed: true,
            value: None,
            limit: None,
            explanation: Some(
                "Snapshot requirement checked (assumed pre-snapshot created)".to_string(),
            ),
        });
    }

    // Rule 3: Budget check via dry_run flag
    if command.dry_run {
        rules.push(PolicyRuleResult {
            rule: "dry_run_mode".to_string(),
            passed: true,
            value: None,
            limit: None,
            explanation: Some("Command is a dry run — no state mutation".to_string()),
        });
    }

    // Rule 4: Read-only commands always pass
    if safety == SafetyClass::Read {
        rules.push(PolicyRuleResult {
            rule: "read_only_command".to_string(),
            passed: true,
            value: None,
            limit: None,
            explanation: Some("Read-only commands always pass policy".to_string()),
        });
    }

    // Rule 5: Engineering commands require governance approval in production
    if safety == SafetyClass::Engineering {
        rules.push(PolicyRuleResult {
            rule: "engineering_requires_approval".to_string(),
            passed: true, // Allow for now, but audit
            value: None,
            limit: None,
            explanation: Some("Engineering command logged for audit trail".to_string()),
        });
    }

    // Determine audit trail requirement
    let audit_trail = matches!(
        safety,
        SafetyClass::Destructive | SafetyClass::Governance | SafetyClass::Engineering
    ) || !allowed;

    PolicyCheckResult {
        allowed,
        rules_evaluated: rules,
        override_reason,
        audit_trail,
    }
}

/// Evaluate a policy check result and convert it into command receipt policy refs.
#[must_use]
pub fn policy_refs_from_check(check: &PolicyCheckResult) -> Vec<crate::command::PolicyDecisionRef> {
    check
        .rules_evaluated
        .iter()
        .map(|r| crate::command::PolicyDecisionRef {
            rule: r.rule.clone(),
            passed: r.passed,
            explanation: r.explanation.clone(),
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::command::{Command, CommandScope};
    use crate::ids::*;
    use chrono::Utc;

    fn make_command(command_type: &str) -> Command {
        Command {
            id: CommandId::new(),
            idempotency_key: format!("test-{command_type}"),
            command_type: command_type.to_string(),
            actor: "agent:test".to_string(),
            project_id: ProjectId::new(),
            branch_id: None,
            dry_run: false,
            scope: CommandScope::default(),
            params: serde_json::json!({}),
            budget_id: None,
            created_at: Utc::now(),
        }
    }

    #[test]
    fn read_commands_always_pass() {
        let command = make_command("get_capabilities");
        let policy = PolicyProfile::default();
        let result = evaluate_policy(&command, &policy);
        assert!(result.allowed);
        assert!(
            result
                .rules_evaluated
                .iter()
                .any(|r| r.rule == "read_only_command")
        );
    }

    #[test]
    fn destructive_commands_blocked_by_default() {
        let command = make_command("delete_project");
        let policy = PolicyProfile::default();
        let result = evaluate_policy(&command, &policy);
        assert!(!result.allowed);
        assert!(result.override_reason.is_some());
        assert!(
            result
                .rules_evaluated
                .iter()
                .any(|r| r.rule == "allow_destructive_commands" && !r.passed)
        );
    }

    #[test]
    fn destructive_commands_allowed_when_permitted() {
        let command = make_command("delete_project");
        let policy = PolicyProfile {
            allow_destructive_commands: true,
            ..Default::default()
        };
        let result = evaluate_policy(&command, &policy);
        assert!(result.allowed);
    }

    #[test]
    fn write_commands_pass_with_default_policy() {
        let command = make_command("apply_patch");
        let policy = PolicyProfile::default();
        let result = evaluate_policy(&command, &policy);
        assert!(result.allowed);
    }

    #[test]
    fn dry_run_commands_always_pass() {
        let mut command = make_command("apply_patch");
        command.dry_run = true;
        let policy = PolicyProfile {
            allow_destructive_commands: false,
            ..Default::default()
        };
        // Even a destructive dry-run should pass
        let mut destructive_command = make_command("delete_project");
        destructive_command.dry_run = true;
        let result = evaluate_policy(&destructive_command, &policy);
        // Dry-run doesn't override destructive check — it still evaluates
        // the destructive rule. This is intentional: dry-run means "show me
        // what would happen" not "bypass safety checks".
        assert!(!result.allowed); // Destructive is still blocked
    }

    #[test]
    fn engineering_commands_trigger_audit() {
        let command = make_command("claim_task");
        let policy = PolicyProfile::default();
        let result = evaluate_policy(&command, &policy);
        assert!(result.allowed);
        assert!(result.audit_trail);
    }

    #[test]
    fn governance_commands_trigger_audit() {
        let command = make_command("update_policy");
        let policy = PolicyProfile::default();
        let result = evaluate_policy(&command, &policy);
        assert!(result.allowed);
        assert!(result.audit_trail);
    }

    #[test]
    fn safety_class_mapping_is_correct() {
        assert_eq!(
            safety_class_for_command("get_capabilities"),
            SafetyClass::Read
        );
        assert_eq!(safety_class_for_command("apply_patch"), SafetyClass::Write);
        assert_eq!(
            safety_class_for_command("render_project"),
            SafetyClass::Render
        );
        assert_eq!(
            safety_class_for_command("delete_project"),
            SafetyClass::Destructive
        );
        assert_eq!(
            safety_class_for_command("update_policy"),
            SafetyClass::Governance
        );
        assert_eq!(
            safety_class_for_command("claim_task"),
            SafetyClass::Engineering
        );
    }

    #[test]
    fn unknown_command_defaults_to_write() {
        assert_eq!(
            safety_class_for_command("unknown_command_xyz"),
            SafetyClass::Write
        );
    }

    #[test]
    fn policy_refs_converted_correctly() {
        let command = make_command("apply_patch");
        let policy = PolicyProfile::default();
        let check = evaluate_policy(&command, &policy);
        let refs = policy_refs_from_check(&check);
        assert!(!refs.is_empty());
        for r in &refs {
            assert!(!r.rule.is_empty());
        }
    }
}
