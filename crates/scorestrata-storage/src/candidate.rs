//! Candidate branches — isolation, evaluation, and tournament selection.
//!
//! Phase 7 implementation per architecture spec sections 13.2–13.7.

use scorestrata_core::diagnostic::Scorecard;
use serde::{Deserialize, Serialize};

/// Status of a candidate branch.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum CandidateStatus {
    /// Candidate is being drafted.
    Drafting,
    /// Candidate is ready for evaluation.
    Ready,
    /// Candidate is being evaluated.
    Evaluating,
    /// Candidate won the tournament.
    Winner,
    /// Candidate lost the tournament.
    Loser,
    /// Candidate was rejected by policy.
    Rejected,
}

/// A candidate branch — an experimental mutation isolated from main state.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Candidate {
    /// Unique identifier.
    pub id: String,
    /// Branch this candidate belongs to.
    pub branch_id: String,
    /// Parent snapshot ID.
    pub parent_snapshot_id: String,
    /// Patches applied in this candidate.
    pub patches: Vec<String>,
    /// Scorecard for this candidate.
    pub scorecard: Option<Scorecard>,
    /// Status.
    pub status: CandidateStatus,
    /// Creation timestamp.
    pub created_at: String,
    /// Evaluation timestamp.
    pub evaluated_at: Option<String>,
}

/// A patch template — reusable repair strategy.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PatchTemplate {
    /// Unique identifier.
    pub id: String,
    /// Template name.
    pub name: String,
    /// Description.
    pub description: String,
    /// Patch operation.
    pub operation: String,
    /// Parameters.
    pub params: std::collections::BTreeMap<String, String>,
    /// Genre this template works well for.
    pub genres: Vec<String>,
    /// Success count.
    pub success_count: u32,
    /// Failure count.
    pub failure_count: u32,
    /// Confidence (0.0–1.0).
    pub confidence: f32,
}

impl PatchTemplate {
    /// Create a new patch template.
    #[must_use]
    pub fn new(name: &str, operation: &str) -> Self {
        Self {
            id: format!("tpl_{}", chrono::Utc::now().timestamp_millis()),
            name: name.to_string(),
            description: String::new(),
            operation: operation.to_string(),
            params: std::collections::BTreeMap::new(),
            genres: vec![],
            success_count: 0,
            failure_count: 0,
            confidence: 0.5,
        }
    }
}

/// Candidate tournament — selects the best candidate from a set.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct CandidateTournament {
    /// Candidates in the tournament.
    pub candidates: Vec<Candidate>,
    /// Winner (if any).
    pub winner_id: Option<String>,
    /// Tournament timestamp.
    pub timestamp: String,
}

impl CandidateTournament {
    /// Create a new empty tournament.
    #[must_use]
    pub fn new() -> Self {
        Self {
            timestamp: chrono::Utc::now().to_rfc3339(),
            ..Self::default()
        }
    }

    /// Add a candidate to the tournament.
    pub fn add_candidate(&mut self, candidate: Candidate) {
        self.candidates.push(candidate);
    }

    /// Select the best candidate using tournament logic.
    ///
    /// Selection criteria:
    /// 1. Filter out non-ready candidates
    /// 2. Sort by overall score descending
    /// 3. Select the highest-scoring candidate
    #[must_use]
    pub fn select_winner(&mut self) -> Option<String> {
        // Find the candidate with the highest overall score
        let best_id = self
            .candidates
            .iter()
            .filter(|c| {
                c.status == CandidateStatus::Ready || c.status == CandidateStatus::Evaluating
            })
            .max_by(|a, b| {
                let score_a = a.scorecard.as_ref().map_or(0.0, |s| s.overall_score);
                let score_b = b.scorecard.as_ref().map_or(0.0, |s| s.overall_score);
                score_a
                    .partial_cmp(&score_b)
                    .unwrap_or(std::cmp::Ordering::Equal)
            })
            .map(|c| c.id.clone());

        if let Some(ref winner_id) = best_id {
            self.winner_id = Some(winner_id.clone());
            // Mark winner and losers
            for candidate in &mut self.candidates {
                if candidate.id == *winner_id {
                    candidate.status = CandidateStatus::Winner;
                } else if candidate.status != CandidateStatus::Rejected {
                    candidate.status = CandidateStatus::Loser;
                }
            }
        }
        best_id
    }
}

/// No-improvement detector — tracks stagnation.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct StagnationDetector {
    /// Number of rounds without improvement.
    pub no_improvement_count: u32,
    /// Threshold before escalation.
    pub threshold: u32,
    /// Best score seen so far.
    pub best_score: f32,
    /// Whether stagnation was detected.
    pub is_stagnant: bool,
}

impl StagnationDetector {
    /// Create a new stagnation detector.
    #[must_use]
    pub const fn new(threshold: u32) -> Self {
        Self {
            no_improvement_count: 0,
            threshold,
            best_score: 0.0,
            is_stagnant: false,
        }
    }

    /// Record a new score and detect stagnation.
    pub fn record_score(&mut self, score: f32) {
        if self.best_score == 0.0 && self.no_improvement_count == 0 {
            // First score — initialize
            self.best_score = score;
        } else if score > self.best_score + 0.001 {
            // Improvement found
            self.best_score = score;
            self.no_improvement_count = 0;
            self.is_stagnant = false;
        } else {
            // No improvement
            self.no_improvement_count += 1;
            self.is_stagnant = self.no_improvement_count >= self.threshold;
        }
    }

    /// Reset the detector.
    pub const fn reset(&mut self) {
        self.no_improvement_count = 0;
        self.is_stagnant = false;
    }
}

/// Human escalation packet — when agent is stuck.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HumanEscalationPacket {
    /// Unique identifier.
    pub id: String,
    /// Project ID.
    pub project_id: String,
    /// Reason for escalation.
    pub reason: String,
    /// Current scorecard.
    pub scorecard: Option<Scorecard>,
    /// Failed patches.
    pub failed_patches: Vec<String>,
    /// Suggested next steps.
    pub suggested_steps: Vec<String>,
    /// Creation timestamp.
    pub created_at: String,
}

impl HumanEscalationPacket {
    /// Create a new escalation packet.
    #[must_use]
    pub fn new(project_id: &str, reason: &str) -> Self {
        Self {
            id: format!("esc_{}", chrono::Utc::now().timestamp_millis()),
            project_id: project_id.to_string(),
            reason: reason.to_string(),
            scorecard: None,
            failed_patches: vec![],
            suggested_steps: vec![],
            created_at: chrono::Utc::now().to_rfc3339(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tournament_selects_highest_score() {
        let mut tournament = CandidateTournament::new();
        tournament.add_candidate(Candidate {
            id: "c1".to_string(),
            branch_id: "main".to_string(),
            parent_snapshot_id: "snap_0".to_string(),
            patches: vec![],
            scorecard: Some(Scorecard {
                id: scorestrata_core::ids::ScorecardId::new(),
                metrics: vec![],
                overall_score: 0.7,
                hard_failures: 0,
                warnings: 0,
            }),
            status: CandidateStatus::Ready,
            created_at: chrono::Utc::now().to_rfc3339(),
            evaluated_at: None,
        });
        tournament.add_candidate(Candidate {
            id: "c2".to_string(),
            branch_id: "main".to_string(),
            parent_snapshot_id: "snap_0".to_string(),
            patches: vec![],
            scorecard: Some(Scorecard {
                id: scorestrata_core::ids::ScorecardId::new(),
                metrics: vec![],
                overall_score: 0.9,
                hard_failures: 0,
                warnings: 0,
            }),
            status: CandidateStatus::Ready,
            created_at: chrono::Utc::now().to_rfc3339(),
            evaluated_at: None,
        });
        let winner = tournament.select_winner();
        assert_eq!(winner, Some("c2".to_string()));
    }

    #[test]
    fn stagnation_detector_detects_no_improvement() {
        let mut detector = StagnationDetector::new(3);
        detector.record_score(0.5); // Initializes best_score
        detector.record_score(0.5); // no_improvement_count = 1
        detector.record_score(0.5); // no_improvement_count = 2
        detector.record_score(0.5); // no_improvement_count = 3, stagnant
        assert!(detector.is_stagnant);
        assert_eq!(detector.no_improvement_count, 3);
    }

    #[test]
    fn stagnation_detector_resets_on_improvement() {
        let mut detector = StagnationDetector::new(3);
        detector.record_score(0.5);
        detector.record_score(0.5);
        detector.record_score(0.6); // Improvement
        assert!(!detector.is_stagnant);
        assert_eq!(detector.no_improvement_count, 0);
    }

    #[test]
    fn escalation_packet_created_correctly() {
        let packet = HumanEscalationPacket::new("proj_123", "Stagnation detected");
        assert_eq!(packet.project_id, "proj_123");
        assert_eq!(packet.reason, "Stagnation detected");
    }
}
