//! Debug plane — first-class debugging capabilities for autonomous agents.
//!
//! Implements architecture spec §12:
//! - Command tracing through the event ledger
//! - Snapshot and candidate diffs
//! - Layer isolation and segment solo
//! - Metric explanation with evidence
//! - Human escalation packet with full context

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

use crate::error::Severity;

// ── Command Trace (§12.2) ───────────────────────────────────────────

/// A command trace entry — tracks a command through the event ledger.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[must_use]
pub struct CommandTrace {
    /// The command ID being traced.
    pub command_id: String,
    /// Actor who issued the command.
    pub actor: String,
    /// Command type (e.g. `apply_patch`, `run_validation`).
    pub command_type: String,
    /// Project ID.
    pub project_id: String,
    /// Branch ID where the command was applied.
    pub branch_id: Option<String>,
    /// Snapshot ID before the command.
    pub before_snapshot_id: Option<String>,
    /// Snapshot ID after the command.
    pub after_snapshot_id: Option<String>,
    /// Events produced by this command.
    pub events: Vec<CommandEvent>,
    /// Policy decisions made during this command.
    pub policy_decisions: Vec<PolicyDecisionSummary>,
    /// Score delta produced.
    pub score_delta: BTreeMap<String, String>,
    /// Whether this was a dry run.
    pub dry_run: bool,
    /// Duration in milliseconds.
    pub duration_ms: u64,
    /// Whether the command succeeded.
    pub succeeded: bool,
    /// Error message if the command failed.
    pub error: Option<String>,
}

/// A summary of a command event.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CommandEvent {
    /// Event type (e.g. `pattern_updated`, `snapshot_created`).
    pub event_type: String,
    /// Event payload summary.
    pub payload_summary: String,
    /// Timestamp.
    pub timestamp: String,
}

/// A summary of a policy decision.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PolicyDecisionSummary {
    /// Rule that was evaluated.
    pub rule: String,
    /// Whether the rule passed or failed.
    pub result: String,
    /// Actual value if numeric.
    pub value: Option<f32>,
    /// Limit if numeric.
    pub limit: Option<f32>,
}

// ── Snapshot Diff (§12.2) ───────────────────────────────────────────

/// A diff between two snapshots — what changed.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[must_use]
pub struct SnapshotDiff {
    /// First snapshot ID.
    pub snapshot_a: String,
    /// Second snapshot ID.
    pub snapshot_b: String,
    /// Project ID.
    pub project_id: String,
    /// High-level summary of changes.
    pub summary: String,
    /// Changed segments.
    pub changed_segments: Vec<SegmentDiff>,
    /// Changed patterns.
    pub changed_patterns: Vec<PatternDiff>,
    /// Changed mixer state.
    pub mixer_changed: bool,
    /// Score delta between snapshots.
    pub score_delta: BTreeMap<String, String>,
}

/// Diff for a single segment.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SegmentDiff {
    /// Segment ID.
    pub segment_id: String,
    /// What changed (e.g. `layers_added`, `bar_count_changed`).
    pub changes: Vec<String>,
    /// Evidence URI for this diff.
    pub evidence_uri: String,
}

/// Diff for a single pattern.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PatternDiff {
    /// Pattern ID.
    pub pattern_id: String,
    /// What changed (e.g. `notes_quantized`, `rhythm_modified`).
    pub changes: Vec<String>,
    /// Evidence URI for this diff.
    pub evidence_uri: String,
}

// ── Candidate Diff (§12.2) ─────────────────────────────────────────

/// Which candidate won a comparison.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CandidateWinner {
    /// Candidate A won.
    A,
    /// Candidate B won.
    B,
    /// Neither candidate won (tie).
    Tie,
}

impl std::fmt::Display for CandidateWinner {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::A => write!(f, "a"),
            Self::B => write!(f, "b"),
            Self::Tie => write!(f, "tie"),
        }
    }
}

/// A diff between two candidates — for A/B comparison.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[must_use]
pub struct CandidateDiff {
    /// Candidate A ID.
    pub candidate_a: String,
    /// Candidate B ID.
    pub candidate_b: String,
    /// Project ID.
    pub project_id: String,
    /// Score comparison.
    pub score_comparison: BTreeMap<String, CandidateScore>,
    /// Winner based on multi-objective comparison.
    pub winner: Option<CandidateWinner>,
    /// Reason for the winner selection.
    pub reason: Option<String>,
}

/// A single metric comparison between two candidates.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CandidateScore {
    /// Metric name.
    pub metric: String,
    /// Candidate A score.
    pub score_a: f32,
    /// Candidate B score.
    pub score_b: f32,
    /// Winner for this metric.
    pub winner: CandidateWinner,
}

// ── Layer Isolation (§12.2) ─────────────────────────────────────────

/// Layer isolation request — solo a specific layer for inspection.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[must_use]
pub struct LayerIsolation {
    /// Project ID.
    pub project_id: String,
    /// Layer ID to isolate.
    pub layer_id: String,
    /// Segment ID if isolating within a segment.
    pub segment_id: Option<String>,
    /// Whether to mute all other layers.
    pub mute_others: bool,
    /// Preview URI for the isolated audio.
    pub preview_uri: Option<String>,
}

// ── Segment Solo (§12.2) ───────────────────────────────────────────

/// Segment solo request — listen to a specific segment.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[must_use]
pub struct SegmentSolo {
    /// Project ID.
    pub project_id: String,
    /// Segment ID to solo.
    pub segment_id: String,
    /// Whether to include transitions in/out.
    pub include_transitions: bool,
    /// Preview URI for the soloed audio.
    pub preview_uri: Option<String>,
}

// ── Metric Explanation (§12.2) ──────────────────────────────────────

/// Full metric explanation — why a metric has its current value.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[must_use]
pub struct MetricExplanation {
    /// Project ID.
    pub project_id: String,
    /// Metric name.
    pub metric_name: String,
    /// Current value.
    pub current_value: f32,
    /// Target value (if known).
    pub target_value: Option<f32>,
    /// Root causes.
    pub root_causes: Vec<RootCauseExplanation>,
    /// Evidence URIs supporting this explanation.
    pub evidence_uris: Vec<String>,
    /// Suggested actions.
    pub suggested_actions: Vec<String>,
    /// Confidence in this explanation (0.0–1.0).
    pub confidence: f32,
}

/// A root cause explanation for a metric.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RootCauseExplanation {
    /// Cause identifier.
    pub cause: String,
    /// Severity of this cause.
    pub severity: Severity,
    /// Scope of this cause.
    pub scope: String,
    /// Evidence URI for this cause.
    pub evidence_uri: Option<String>,
    /// Suggested patch to fix this cause.
    pub suggested_patch: Option<String>,
}

// ── Human Escalation Packet (§12.3) ────────────────────────────────

/// Full human escalation packet — all context needed for a human decision.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[must_use]
pub struct HumanEscalationPacket {
    /// Project ID.
    pub project_id: String,
    /// Summary of the situation.
    pub summary: String,
    /// Latest validation report.
    pub latest_validation: Option<String>,
    /// Candidate comparison results.
    pub candidate_comparison: Option<CandidateDiff>,
    /// Patches attempted and their outcomes.
    pub attempted_patches: Vec<AttemptedPatch>,
    /// Failed metrics with evidence.
    pub failed_metrics: Vec<MetricExplanation>,
    /// Recommended decisions for the human.
    pub recommended_decisions: Vec<RecommendedDecision>,
    /// Agent's confidence that the current state is acceptable.
    pub agent_confidence: f32,
    /// Timestamp of escalation.
    pub timestamp: chrono::DateTime<chrono::Utc>,
}

/// A summary of an attempted patch.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AttemptedPatch {
    /// Patch operation name.
    pub operation: String,
    /// Scope of the patch.
    pub scope: String,
    /// Whether it succeeded.
    pub succeeded: bool,
    /// Score delta achieved.
    pub score_delta: f32,
    /// Iteration number.
    pub iteration: u32,
}

/// A recommended decision for the human.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RecommendedDecision {
    /// Decision type (e.g. `approve_current`, `adjust_target`, `manual_patch`).
    pub decision_type: String,
    /// Description of what this decision would do.
    pub description: String,
    /// Confidence that this is the right decision (0.0–1.0).
    pub confidence: f32,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn command_trace_serializes() {
        let trace = CommandTrace {
            command_id: "cmd_001".to_string(),
            actor: "agent:composer".to_string(),
            command_type: "apply_patch".to_string(),
            project_id: "proj_123".to_string(),
            branch_id: Some("main".to_string()),
            before_snapshot_id: Some("snap_001".to_string()),
            after_snapshot_id: Some("snap_002".to_string()),
            events: vec![],
            policy_decisions: vec![],
            score_delta: BTreeMap::new(),
            dry_run: false,
            duration_ms: 42,
            succeeded: true,
            error: None,
        };
        #[expect(clippy::unwrap_used, reason = "Test assertion")]
        let json = serde_json::to_string_pretty(&trace).unwrap();
        assert!(json.contains("cmd_001"));
        assert!(json.contains("apply_patch"));
    }

    #[test]
    fn snapshot_diff_serializes() {
        let diff = SnapshotDiff {
            snapshot_a: "snap_001".to_string(),
            snapshot_b: "snap_002".to_string(),
            project_id: "proj_123".to_string(),
            summary: "3 segments changed".to_string(),
            changed_segments: vec![],
            changed_patterns: vec![],
            mixer_changed: false,
            score_delta: BTreeMap::new(),
        };
        #[expect(clippy::unwrap_used, reason = "Test assertion")]
        let json = serde_json::to_string_pretty(&diff).unwrap();
        assert!(json.contains("snap_001"));
    }

    #[test]
    fn candidate_diff_selects_winner() {
        let diff = CandidateDiff {
            candidate_a: "cand_a".to_string(),
            candidate_b: "cand_b".to_string(),
            project_id: "proj_123".to_string(),
            score_comparison: BTreeMap::from([(
                "scale_compliance".to_string(),
                CandidateScore {
                    metric: "scale_compliance".to_string(),
                    score_a: 0.9,
                    score_b: 0.7,
                    winner: CandidateWinner::A,
                },
            )]),
            winner: Some(CandidateWinner::A),
            reason: Some("Higher scale compliance".to_string()),
        };
        #[expect(clippy::unwrap_used, reason = "Test assertion")]
        let json = serde_json::to_string_pretty(&diff).unwrap();
        assert!(json.contains("cand_a"));
        assert!(json.contains("Higher scale compliance"));
    }

    #[test]
    fn metric_explanation_has_all_fields() {
        let explanation = MetricExplanation {
            project_id: "proj_123".to_string(),
            metric_name: "harmony.scale_compliance".to_string(),
            current_value: 0.68,
            target_value: Some(0.85),
            root_causes: vec![RootCauseExplanation {
                cause: "non_scale_notes_on_strong_beats".to_string(),
                severity: Severity::Error,
                scope: "segment:chorus_1,layer:lead".to_string(),
                evidence_uri: Some(
                    "project://proj_123/segments/chorus_1/layers/lead/notes".to_string(),
                ),
                suggested_patch: Some("quantize_layer_to_scale".to_string()),
            }],
            evidence_uris: vec!["project://proj_123/harmony".to_string()],
            suggested_actions: vec!["Quantize lead to C minor".to_string()],
            confidence: 0.85,
        };
        assert_eq!(explanation.root_causes.len(), 1);
        assert!(explanation.confidence > 0.5);
    }

    #[test]
    fn human_escalation_packet_has_all_sections() {
        let packet = HumanEscalationPacket {
            project_id: "proj_123".to_string(),
            summary: "Stagnation detected after 5 rounds".to_string(),
            latest_validation: None,
            candidate_comparison: None,
            attempted_patches: vec![AttemptedPatch {
                operation: "quantize_to_scale".to_string(),
                scope: "harmony".to_string(),
                succeeded: true,
                score_delta: 0.05,
                iteration: 3,
            }],
            failed_metrics: vec![],
            recommended_decisions: vec![RecommendedDecision {
                decision_type: "approve_current".to_string(),
                description: "Current score is acceptable for this genre".to_string(),
                confidence: 0.7,
            }],
            agent_confidence: 0.6,
            timestamp: chrono::Utc::now(),
        };
        assert_eq!(packet.attempted_patches.len(), 1);
        assert_eq!(packet.recommended_decisions.len(), 1);
        assert!(packet.agent_confidence > 0.0);
    }

    #[test]
    fn layer_isolation_serializes() {
        let isolation = LayerIsolation {
            project_id: "proj_123".to_string(),
            layer_id: "lead".to_string(),
            segment_id: Some("chorus_1".to_string()),
            mute_others: true,
            preview_uri: None,
        };
        #[expect(clippy::unwrap_used, reason = "Test assertion")]
        let json = serde_json::to_string_pretty(&isolation).unwrap();
        assert!(json.contains("lead"));
        assert!(json.contains("chorus_1"));
    }
}
