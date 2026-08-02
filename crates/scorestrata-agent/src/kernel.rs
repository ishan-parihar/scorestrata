//! Agent kernel — the autonomous orchestrator that runs the improvement loop.
//!
//! Implements the L0–L4 loops per architecture spec sections 13.1–13.7.
//! The kernel coordinates diagnosis, candidate generation, evaluation,
//! patching, and escalation.

use serde::{Deserialize, Serialize};

use scorestrata_core::ids::ProjectId;

// ── Loop Levels ─────────────────────────────────────────────────────

/// The level of the agent loop.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum LoopLevel {
    /// L0: Orientation — discover capabilities, inspect state, select playbook.
    Orientation,
    /// L1: Micro-repair — fix localized issues quickly.
    MicroRepair,
    /// L2: Segment development — build or improve a segment.
    SegmentDevelopment,
    /// L3: Project refinement — improve the whole track.
    ProjectRefinement,
    /// L4: Learning — convert experience into competence.
    Learning,
}

impl std::fmt::Display for LoopLevel {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Orientation => write!(f, "L0-orientation"),
            Self::MicroRepair => write!(f, "L1-micro-repair"),
            Self::SegmentDevelopment => write!(f, "L2-segment-dev"),
            Self::ProjectRefinement => write!(f, "L3-project-refinement"),
            Self::Learning => write!(f, "L4-learning"),
        }
    }
}

// ── Kernel State ────────────────────────────────────────────────────

/// The current state of the agent kernel.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum KernelState {
    /// Agent is initializing (L0 orientation).
    Initializing,
    /// Agent is actively working on improvement.
    Running,
    /// Agent is generating candidates.
    GeneratingCandidates,
    /// Agent is evaluating candidates.
    EvaluatingCandidates,
    /// Agent is applying a patch.
    ApplyingPatch,
    /// Agent is recording a lesson.
    RecordingLesson,
    /// Agent is awaiting human decision.
    AwaitingHuman,
    /// Agent has completed successfully.
    Completed,
    /// Agent has been terminated.
    Terminated,
}

impl std::fmt::Display for KernelState {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Initializing => write!(f, "initializing"),
            Self::Running => write!(f, "running"),
            Self::GeneratingCandidates => write!(f, "generating-candidates"),
            Self::EvaluatingCandidates => write!(f, "evaluating-candidates"),
            Self::ApplyingPatch => write!(f, "applying-patch"),
            Self::RecordingLesson => write!(f, "recording-lesson"),
            Self::AwaitingHuman => write!(f, "awaiting-human"),
            Self::Completed => write!(f, "completed"),
            Self::Terminated => write!(f, "terminated"),
        }
    }
}

// ── Iteration Result ────────────────────────────────────────────────

/// The outcome of a single iteration of the kernel loop.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IterationResult {
    /// Whether improvement was found.
    pub improved: bool,
    /// Number of candidates evaluated.
    pub candidates_evaluated: u32,
    /// Score delta if improvement was applied.
    pub score_delta: Option<f32>,
    /// Message describing what happened.
    pub message: String,
    /// The patch operation that was applied (if any).
    pub applied_patch: Option<String>,
    /// Time taken for this iteration in milliseconds.
    pub duration_ms: u64,
}

// ── Escalation ──────────────────────────────────────────────────────

/// Human escalation reason.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum EscalationReason {
    /// No improvement for N rounds.
    Stagnation {
        /// Number of rounds without improvement.
        rounds: u32,
    },
    /// Budget exhausted.
    BudgetExhausted,
    /// Multiple candidates are equally good (need human choice).
    TieBreaking,
    /// Critical error that cannot be auto-resolved.
    CriticalError {
        /// Description of the error.
        message: String,
    },
    /// Target score reached, needs final approval.
    TargetReached,
}

impl std::fmt::Display for EscalationReason {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Stagnation { rounds } => write!(f, "No improvement for {rounds} rounds"),
            Self::BudgetExhausted => write!(f, "Budget exhausted"),
            Self::TieBreaking => write!(f, "Tie-breaking required"),
            Self::CriticalError { message } => write!(f, "Critical error: {message}"),
            Self::TargetReached => write!(f, "Target score reached"),
        }
    }
}

// ── Human Review Packet ─────────────────────────────────────────────

/// A human review packet containing all context needed for a decision.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HumanReviewPacket {
    /// Project ID.
    pub project_id: ProjectId,
    /// Reason for escalation.
    pub reason: EscalationReason,
    /// Current score summary.
    pub current_score: f32,
    /// Top issues remaining.
    pub top_issues: Vec<String>,
    /// What the agent tried.
    pub attempted_actions: Vec<String>,
    /// Suggested next steps for the human.
    pub suggested_actions: Vec<String>,
}

// ── Diagnosis ───────────────────────────────────────────────────────

/// A diagnostic finding about the project state.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Diagnosis {
    /// The metric that has an issue.
    pub metric: String,
    /// Scope of the issue (e.g. `harmony`, `transition`, `mix`).
    pub scope: String,
    /// Current value of the metric.
    pub current_value: f32,
    /// Target value for the metric.
    pub target_value: f32,
    /// Severity: 0.0 (minor) to 1.0 (critical).
    pub severity: f32,
    /// Whether this is a hard failure (gate violation).
    pub is_hard_failure: bool,
    /// Evidence URI supporting this diagnosis.
    pub evidence_uri: Option<String>,
}

// ── Candidate ───────────────────────────────────────────────────────

/// A candidate patch to apply.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Candidate {
    /// Unique ID for this candidate.
    pub id: String,
    /// Patch operation name.
    pub operation: String,
    /// Description of what this patch does.
    pub description: String,
    /// Target scope.
    pub scope: String,
    /// The diagnosis this candidate addresses.
    pub diagnosis_metric: String,
    /// Estimated score delta (positive = improvement).
    pub estimated_delta: f32,
    /// Whether this is a structural repair vs improvement.
    pub is_repair: bool,
    /// Template ID if this came from a patch template.
    pub template_id: Option<String>,
}

// ── Scored Candidate ────────────────────────────────────────────────

/// A candidate with its evaluated score.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScoredCandidate {
    /// The candidate.
    pub candidate: Candidate,
    /// Actual score delta after dry-run evaluation.
    pub score_delta: f32,
    /// Whether the hard gates passed.
    pub hard_gates_pass: bool,
    /// Policy compliance score.
    pub policy_score: f32,
    /// Cost in CPU seconds.
    pub cost_seconds: f32,
}

// ── Agent Configuration ─────────────────────────────────────────────

/// Configuration for the agent kernel.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentConfig {
    /// Maximum iterations before escalation.
    pub max_iterations: u32,
    /// Maximum consecutive rounds without improvement.
    pub max_no_improvement: u32,
    /// Target overall score (0.0–1.0).
    pub target_score: f32,
    /// Minimum score delta to consider an improvement.
    pub improvement_threshold: f32,
    /// Maximum candidates to evaluate per iteration.
    pub max_candidates_per_iteration: u32,
    /// Hard gate metrics that must pass.
    pub hard_gate_metrics: Vec<String>,
    /// Hard gate thresholds (metric → minimum value).
    pub hard_gate_thresholds: Vec<(String, f32)>,
}

impl Default for AgentConfig {
    fn default() -> Self {
        Self {
            max_iterations: 12,
            max_no_improvement: 3,
            target_score: 0.85,
            improvement_threshold: 0.01,
            max_candidates_per_iteration: 5,
            hard_gate_metrics: vec!["loudness_lufs".to_string(), "true_peak_dbtp".to_string()],
            hard_gate_thresholds: vec![
                ("loudness_lufs".to_string(), -14.0),
                ("true_peak_dbtp".to_string(), -1.0),
            ],
        }
    }
}

// ── Agent Kernel ────────────────────────────────────────────────────

/// The agent kernel orchestrating the improvement loop.
///
/// Implements the main loop from architecture spec section 13.7:
///
/// ```text
/// while not done and budget.remaining:
///     summary = get_project_summary()
///     diagnosis = get_top_diagnosis(summary)
///     candidates = generate_candidates(diagnosis)
///     for candidate in candidates:
///         dry = dry_run_patch(candidate)
///         branch = create_candidate_branch(candidate)
///         compile(branch)
///         validate(branch)
///         score = score_candidate(branch)
///     best = tournament_select(evaluated)
///     if best.improves_threshold:
///         apply_patch(best)
///         create_snapshot()
///         record_lesson(diagnosis, best)
///     else:
///         increase_no_improvement_counter()
///     if no_improvement_counter > limit:
///         create_human_review_packet()
///         break
/// ```
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentKernel {
    /// Project being worked on.
    pub project_id: ProjectId,
    /// Current loop level.
    pub level: LoopLevel,
    /// Current kernel state.
    pub state: KernelState,
    /// Current iteration number.
    pub iteration: u32,
    /// Agent configuration.
    pub config: AgentConfig,
    /// Consecutive rounds without improvement.
    pub no_improvement_streak: u32,
    /// Overall score achieved.
    pub current_score: f32,
    /// Whether hard gates are passing.
    pub hard_gates_passing: bool,
    /// Total candidates evaluated across all iterations.
    pub total_candidates_evaluated: u32,
    /// Total patches applied.
    pub total_patches_applied: u32,
    /// History of iteration results.
    pub history: Vec<IterationResult>,
    /// Lessons learned during this session.
    pub lessons_learned: Vec<RecordedLesson>,
}

/// A lesson recorded from an iteration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RecordedLesson {
    /// The diagnosis that triggered this lesson.
    pub diagnosis_metric: String,
    /// The scope.
    pub scope: String,
    /// The patch that was applied.
    pub applied_patch: String,
    /// Whether it was successful.
    pub successful: bool,
    /// The score delta achieved.
    pub score_delta: f32,
    /// Iteration number when this lesson was recorded.
    pub iteration: u32,
}

impl AgentKernel {
    /// Create a new agent kernel for a project.
    #[must_use]
    pub fn new(project_id: ProjectId) -> Self {
        Self::with_config(project_id, AgentConfig::default())
    }

    /// Create a new agent kernel with custom configuration.
    #[must_use]
    pub const fn with_config(project_id: ProjectId, config: AgentConfig) -> Self {
        Self {
            project_id,
            level: LoopLevel::Orientation,
            state: KernelState::Initializing,
            iteration: 0,
            config,
            no_improvement_streak: 0,
            current_score: 0.0,
            hard_gates_passing: false,
            total_candidates_evaluated: 0,
            total_patches_applied: 0,
            history: Vec::new(),
            lessons_learned: Vec::new(),
        }
    }

    /// Check if the kernel should terminate.
    #[must_use]
    pub const fn should_terminate(&self) -> bool {
        self.iteration >= self.config.max_iterations
            || self.no_improvement_streak >= self.config.max_no_improvement
            || (self.current_score >= self.config.target_score && self.hard_gates_passing)
    }

    /// Check if human escalation is needed.
    #[must_use]
    pub const fn needs_escalation(&self) -> Option<EscalationReason> {
        if self.no_improvement_streak >= self.config.max_no_improvement {
            return Some(EscalationReason::Stagnation {
                rounds: self.no_improvement_streak,
            });
        }
        if self.iteration >= self.config.max_iterations {
            return Some(EscalationReason::BudgetExhausted);
        }
        None
    }

    /// Record an iteration result — single point of truth for iteration counting.
    pub fn record_iteration(&mut self, result: IterationResult) {
        self.iteration += 1;
        self.total_candidates_evaluated += result.candidates_evaluated;

        if result.improved {
            self.no_improvement_streak = 0;
            self.total_patches_applied += 1;
            if let Some(delta) = result.score_delta {
                self.current_score += delta;
            }
        } else {
            self.no_improvement_streak += 1;
        }

        self.history.push(result);
    }

    /// Transition to a new state.
    pub const fn transition(&mut self, new_state: KernelState) {
        self.state = new_state;
    }

    /// Advance the loop level.
    pub const fn advance_level(&mut self) {
        self.level = match self.level {
            LoopLevel::Orientation => LoopLevel::MicroRepair,
            LoopLevel::MicroRepair => LoopLevel::SegmentDevelopment,
            LoopLevel::SegmentDevelopment => LoopLevel::ProjectRefinement,
            LoopLevel::ProjectRefinement | LoopLevel::Learning => LoopLevel::Learning,
        };
    }

    /// Build a human review packet for escalation.
    #[must_use]
    pub fn build_escalation_packet(
        &self,
        reason: EscalationReason,
        top_issues: Vec<String>,
    ) -> HumanReviewPacket {
        let attempted: Vec<String> = self.history.iter().map(|r| r.message.clone()).collect();

        HumanReviewPacket {
            project_id: self.project_id,
            reason,
            current_score: self.current_score,
            top_issues,
            attempted_actions: attempted,
            suggested_actions: vec![
                "Review top issues and apply manual patches".to_string(),
                "Adjust target score or budget".to_string(),
                "Approve current state as final".to_string(),
            ],
        }
    }

    // ── L0: Orientation Loop ────────────────────────────────────────

    /// L0: Orientation — discover capabilities and select initial playbook.
    ///
    /// Returns the top diagnoses to focus on in subsequent loops.
    #[must_use]
    pub fn l0_orient(&mut self, available_metrics: &[String]) -> Vec<Diagnosis> {
        self.transition(KernelState::Running);
        eprintln!(
            "[{}] L0 orientation: discovered {} metrics",
            self.project_id,
            available_metrics.len()
        );
        self.level = LoopLevel::Orientation;
        Vec::new()
    }

    // ── L1: Micro-Repair Loop ───────────────────────────────────────

    /// L1: Micro-repair — fix localized issues quickly.
    #[must_use]
    pub fn l1_micro_repair(
        &mut self,
        diagnosis: &Diagnosis,
        available_templates: &[String],
    ) -> Vec<Candidate> {
        self.transition(KernelState::GeneratingCandidates);
        eprintln!(
            "[{}] L1 micro-repair: generating candidates for {} (severity={:.2})",
            self.project_id, diagnosis.metric, diagnosis.severity
        );

        let mut candidates = Vec::new();

        if diagnosis.is_hard_failure {
            candidates.push(Candidate {
                id: format!("cand_repair_{}_{}", self.iteration, candidates.len()),
                operation: format!("fix_{}", diagnosis.metric),
                description: format!("Structural repair for {}", diagnosis.metric),
                scope: diagnosis.scope.clone(),
                diagnosis_metric: diagnosis.metric.clone(),
                estimated_delta: diagnosis.severity * 0.5,
                is_repair: true,
                template_id: None,
            });
        }

        for (i, template) in available_templates
            .iter()
            .enumerate()
            .take(self.config.max_candidates_per_iteration as usize - candidates.len())
        {
            candidates.push(Candidate {
                id: format!("cand_improve_{}_{}", self.iteration, i),
                operation: template.clone(),
                description: format!("Improvement patch: {template}"),
                scope: diagnosis.scope.clone(),
                diagnosis_metric: diagnosis.metric.clone(),
                estimated_delta: self.config.improvement_threshold,
                is_repair: false,
                template_id: Some(template.clone()),
            });
        }

        candidates
    }

    // ── L2: Segment Development Loop ────────────────────────────────

    /// L2: Segment development — build or improve a segment.
    #[must_use]
    pub fn l2_segment_development(
        &mut self,
        segment_name: &str,
        diagnoses: &[Diagnosis],
    ) -> Vec<Candidate> {
        self.transition(KernelState::GeneratingCandidates);
        eprintln!(
            "[{}] L2 segment development: working on {} with {} diagnoses",
            self.project_id,
            segment_name,
            diagnoses.len()
        );

        let mut candidates = Vec::new();
        for diagnosis in diagnoses {
            candidates.extend(self.l1_micro_repair(diagnosis, &[]));
        }
        candidates
    }

    // ── L3: Project Refinement Loop ─────────────────────────────────

    /// L3: Project refinement — improve the whole track.
    #[must_use]
    pub fn l3_project_refinement(&mut self, diagnoses: &[Diagnosis]) -> Vec<Candidate> {
        self.transition(KernelState::Running);
        eprintln!(
            "[{}] L3 project refinement: {} diagnoses to address",
            self.project_id,
            diagnoses.len()
        );

        let mut sorted = diagnoses.to_vec();
        sorted.sort_by(|a, b| {
            b.severity
                .partial_cmp(&a.severity)
                .unwrap_or(std::cmp::Ordering::Equal)
        });

        let mut candidates = Vec::new();
        for diagnosis in &sorted {
            candidates.extend(self.l1_micro_repair(diagnosis, &[]));
            if candidates.len() >= self.config.max_candidates_per_iteration as usize {
                break;
            }
        }
        candidates
    }

    // ── L4: Learning Loop ───────────────────────────────────────────

    /// L4: Learning — convert experience into competence.
    pub fn l4_learn(&mut self, diagnosis: &Diagnosis, candidate: &Candidate, score_delta: f32) {
        self.transition(KernelState::RecordingLesson);
        eprintln!(
            "[{}] L4 learning: recording lesson for {} (delta={:.3})",
            self.project_id, diagnosis.metric, score_delta
        );

        let successful = score_delta > self.config.improvement_threshold;

        let lesson = RecordedLesson {
            diagnosis_metric: diagnosis.metric.clone(),
            scope: diagnosis.scope.clone(),
            applied_patch: candidate.operation.clone(),
            successful,
            score_delta,
            iteration: self.iteration,
        };

        self.lessons_learned.push(lesson);
    }

    // ── Tournament Selection ────────────────────────────────────────

    /// Select the best candidate from a scored list using multi-objective
    /// tournament selection.
    #[must_use]
    pub fn tournament_select(scored: &[ScoredCandidate]) -> Option<&ScoredCandidate> {
        if scored.is_empty() {
            return None;
        }

        // Filter to hard-gate-passing candidates first
        let valid: Vec<&ScoredCandidate> = scored.iter().filter(|c| c.hard_gates_pass).collect();

        let pool = if valid.is_empty() {
            scored.iter().collect::<Vec<_>>()
        } else {
            valid
        };

        // Sort by composite score: score_delta * policy_score
        let mut sorted = pool;
        sorted.sort_by(|a, b| {
            let score_a = a.score_delta * a.policy_score;
            let score_b = b.score_delta * b.policy_score;
            score_b
                .partial_cmp(&score_a)
                .unwrap_or(std::cmp::Ordering::Equal)
        });

        sorted.first().copied()
    }

    // ── Dry Run ─────────────────────────────────────────────────────

    /// Dry-run a candidate without applying it.
    #[must_use]
    pub fn dry_run(
        &self,
        candidate: &Candidate,
        current_metrics: &[(String, f32)],
    ) -> ScoredCandidate {
        let policy_score = if candidate.is_repair { 1.0 } else { 0.9 };

        // Check hard gates against current metrics
        let hard_gates_pass = self.config.hard_gate_metrics.iter().all(|metric| {
            let value = current_metrics
                .iter()
                .find(|(m, _)| m == metric)
                .map_or(f32::MIN, |(_, v)| *v);
            let threshold = self
                .config
                .hard_gate_thresholds
                .iter()
                .find(|(m, _)| m == metric)
                .map_or(f32::MIN, |(_, t)| *t);
            value >= threshold
        });

        ScoredCandidate {
            candidate: candidate.clone(),
            score_delta: candidate.estimated_delta,
            hard_gates_pass,
            policy_score,
            cost_seconds: 0.1,
        }
    }

    // ── Main Loop (Section 13.7) ────────────────────────────────────

    /// Execute one full iteration of the main loop from section 13.7.
    #[allow(
        clippy::too_many_lines,
        reason = "Main orchestration loop — intentional single-method design"
    )]
    pub fn run_iteration(
        &mut self,
        diagnoses: &[Diagnosis],
        available_templates: &[String],
        current_metrics: &[(String, f32)],
    ) -> Option<IterationResult> {
        let start = std::time::Instant::now();

        if self.should_terminate() {
            self.transition(KernelState::Completed);
            return None;
        }

        // L0: First iteration — orientation
        if self.iteration == 0 {
            let diag_metrics: Vec<String> = diagnoses.iter().map(|d| d.metric.clone()).collect();
            let _ = self.l0_orient(&diag_metrics);
        }

        if let Some(reason) = self.needs_escalation() {
            self.transition(KernelState::AwaitingHuman);
            let result = IterationResult {
                improved: false,
                candidates_evaluated: 0,
                score_delta: None,
                message: format!("Escalation: {reason}"),
                applied_patch: None,
                duration_ms: Self::elapsed_ms(&start),
            };
            self.record_iteration(result);
            return self.history.last().cloned();
        }

        let Some(top_issue) = diagnoses.iter().max_by(|a, b| {
            a.severity
                .partial_cmp(&b.severity)
                .unwrap_or(std::cmp::Ordering::Equal)
        }) else {
            let result = IterationResult {
                improved: false,
                candidates_evaluated: 0,
                score_delta: None,
                message: "No diagnoses to address".to_string(),
                applied_patch: None,
                duration_ms: Self::elapsed_ms(&start),
            };
            self.record_iteration(result);
            return self.history.last().cloned();
        };

        let candidates = match self.level {
            LoopLevel::Orientation | LoopLevel::MicroRepair => {
                self.l1_micro_repair(top_issue, available_templates)
            }
            LoopLevel::SegmentDevelopment => {
                self.l2_segment_development(&top_issue.scope, diagnoses)
            }
            LoopLevel::ProjectRefinement | LoopLevel::Learning => {
                self.l3_project_refinement(diagnoses)
            }
        };

        self.transition(KernelState::EvaluatingCandidates);

        let scored: Vec<ScoredCandidate> = candidates
            .iter()
            .map(|c| self.dry_run(c, current_metrics))
            .collect();

        let result = Self::select_and_apply(&scored, self.config.improvement_threshold);
        let (improved, applied_patch, score_delta, message) = match result {
            Some((best_op, delta)) => {
                self.transition(KernelState::ApplyingPatch);
                self.record_lesson(&top_issue.metric, &top_issue.scope, &best_op, delta);

                (
                    true,
                    Some(best_op),
                    Some(delta),
                    format!("Applied patch: delta={delta:.3}"),
                )
            }
            None => (
                false,
                None,
                None,
                "No valid candidates above threshold".to_string(),
            ),
        };

        let iter_result = IterationResult {
            improved,
            candidates_evaluated: u32::try_from(scored.len()).unwrap_or(u32::MAX),
            score_delta,
            message,
            applied_patch,
            duration_ms: Self::elapsed_ms(&start),
        };
        self.record_iteration(iter_result);

        // Advance level on every other successful iteration
        // TODO(#1): Use task-completion triggers instead of iteration count
        if self.no_improvement_streak == 0 && self.iteration.is_multiple_of(2) {
            self.advance_level();
        }

        self.history.last().cloned()
    }

    /// Core tournament selection + threshold check, extracted for testability.
    fn select_and_apply(scored: &[ScoredCandidate], threshold: f32) -> Option<(String, f32)> {
        let best = Self::tournament_select(scored)?;
        if best.score_delta >= threshold {
            Some((best.candidate.operation.clone(), best.score_delta))
        } else {
            None
        }
    }

    /// Record a lesson from an iteration.
    fn record_lesson(&mut self, metric: &str, scope: &str, operation: &str, delta: f32) {
        self.transition(KernelState::RecordingLesson);
        let successful = delta > self.config.improvement_threshold;
        self.lessons_learned.push(RecordedLesson {
            diagnosis_metric: metric.to_string(),
            scope: scope.to_string(),
            applied_patch: operation.to_string(),
            successful,
            score_delta: delta,
            iteration: self.iteration,
        });
    }

    /// Safely convert elapsed duration to milliseconds.
    #[expect(
        clippy::cast_possible_truncation,
        reason = "Duration fits in u64 for practical use"
    )]
    fn elapsed_ms(start: &std::time::Instant) -> u64 {
        start.elapsed().as_millis() as u64
    }

    /// Run the full improvement loop until termination.
    pub fn run<F1, F2>(
        &mut self,
        mut get_diagnoses: F1,
        mut get_templates: F2,
    ) -> (Vec<IterationResult>, KernelState)
    where
        F1: FnMut(&ProjectId) -> Vec<Diagnosis>,
        F2: FnMut(&ProjectId) -> Vec<String>,
    {
        let mut all_results = Vec::new();

        loop {
            let diagnoses = get_diagnoses(&self.project_id);
            let templates = get_templates(&self.project_id);

            match self.run_iteration(&diagnoses, &templates, &[]) {
                Some(result) => {
                    all_results.push(result);
                }
                None => break,
            }
        }

        (all_results, self.state)
    }
}

// ── Tests ───────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn kernel_should_create_with_defaults() {
        let kernel = AgentKernel::new(ProjectId::new());
        assert_eq!(kernel.state, KernelState::Initializing);
        assert_eq!(kernel.level, LoopLevel::Orientation);
        assert_eq!(kernel.iteration, 0);
        assert!(!kernel.should_terminate());
    }

    #[test]
    fn kernel_should_create_with_custom_config() {
        let config = AgentConfig {
            max_iterations: 5,
            max_no_improvement: 2,
            target_score: 0.9,
            improvement_threshold: 0.02,
            max_candidates_per_iteration: 3,
            hard_gate_metrics: vec!["loudness_lufs".to_string()],
            hard_gate_thresholds: vec![("loudness_lufs".to_string(), -14.0)],
        };
        let kernel = AgentKernel::with_config(ProjectId::new(), config);
        assert_eq!(kernel.config.max_iterations, 5);
        assert!((kernel.config.target_score - 0.9).abs() < f32::EPSILON);
    }

    #[test]
    fn kernel_should_terminate_on_max_iterations() {
        let config = AgentConfig {
            max_iterations: 2,
            ..AgentConfig::default()
        };
        let mut kernel = AgentKernel::with_config(ProjectId::new(), config);
        for _ in 0..2 {
            kernel.record_iteration(IterationResult {
                improved: false,
                candidates_evaluated: 1,
                score_delta: None,
                message: "no improvement".to_string(),
                applied_patch: None,
                duration_ms: 0,
            });
        }
        assert!(kernel.should_terminate());
    }

    #[test]
    fn kernel_should_escalate_on_stagnation() {
        let config = AgentConfig {
            max_no_improvement: 2,
            ..AgentConfig::default()
        };
        let mut kernel = AgentKernel::with_config(ProjectId::new(), config);
        kernel.record_iteration(IterationResult {
            improved: false,
            candidates_evaluated: 1,
            score_delta: None,
            message: "no improvement".to_string(),
            applied_patch: None,
            duration_ms: 0,
        });
        kernel.record_iteration(IterationResult {
            improved: false,
            candidates_evaluated: 1,
            score_delta: None,
            message: "no improvement".to_string(),
            applied_patch: None,
            duration_ms: 0,
        });
        assert!(kernel.needs_escalation().is_some());
    }

    #[test]
    fn kernel_should_not_escalate_with_improvement() {
        let config = AgentConfig {
            max_no_improvement: 2,
            ..AgentConfig::default()
        };
        let mut kernel = AgentKernel::with_config(ProjectId::new(), config);
        kernel.record_iteration(IterationResult {
            improved: false,
            candidates_evaluated: 1,
            score_delta: None,
            message: "no improvement".to_string(),
            applied_patch: None,
            duration_ms: 0,
        });
        kernel.record_iteration(IterationResult {
            improved: true,
            candidates_evaluated: 3,
            score_delta: Some(0.1),
            message: "improved".to_string(),
            applied_patch: Some("fix_scale".to_string()),
            duration_ms: 0,
        });
        assert_eq!(kernel.no_improvement_streak, 0);
        assert!(kernel.needs_escalation().is_none());
    }

    #[test]
    fn kernel_should_track_score() {
        let mut kernel = AgentKernel::new(ProjectId::new());
        kernel.record_iteration(IterationResult {
            improved: true,
            candidates_evaluated: 3,
            score_delta: Some(0.15),
            message: "improved".to_string(),
            applied_patch: Some("fix_scale".to_string()),
            duration_ms: 0,
        });
        kernel.record_iteration(IterationResult {
            improved: true,
            candidates_evaluated: 5,
            score_delta: Some(0.10),
            message: "improved more".to_string(),
            applied_patch: Some("add_package".to_string()),
            duration_ms: 0,
        });
        assert!((kernel.current_score - 0.25).abs() < f32::EPSILON);
        assert_eq!(kernel.total_patches_applied, 2);
        assert_eq!(kernel.total_candidates_evaluated, 8);
    }

    #[test]
    fn kernel_should_build_escalation_packet() {
        let mut kernel = AgentKernel::new(ProjectId::new());
        kernel.current_score = 0.65;
        let packet = kernel.build_escalation_packet(
            EscalationReason::Stagnation { rounds: 3 },
            vec!["low density".to_string()],
        );
        assert!((packet.current_score - 0.65).abs() < f32::EPSILON);
        assert!(matches!(
            packet.reason,
            EscalationReason::Stagnation { rounds: 3 }
        ));
        assert_eq!(packet.top_issues.len(), 1);
    }

    #[test]
    fn kernel_should_terminate_on_target() {
        let config = AgentConfig {
            target_score: 0.8,
            ..AgentConfig::default()
        };
        let mut kernel = AgentKernel::with_config(ProjectId::new(), config);
        kernel.current_score = 0.85;
        kernel.hard_gates_passing = true;
        assert!(kernel.should_terminate());
    }

    #[test]
    fn kernel_should_advance_level() {
        let mut kernel = AgentKernel::new(ProjectId::new());
        assert_eq!(kernel.level, LoopLevel::Orientation);
        kernel.advance_level();
        assert_eq!(kernel.level, LoopLevel::MicroRepair);
        kernel.advance_level();
        assert_eq!(kernel.level, LoopLevel::SegmentDevelopment);
        kernel.advance_level();
        assert_eq!(kernel.level, LoopLevel::ProjectRefinement);
        kernel.advance_level();
        assert_eq!(kernel.level, LoopLevel::Learning);
        kernel.advance_level();
        assert_eq!(kernel.level, LoopLevel::Learning);
    }

    #[test]
    fn tournament_should_select_best_passing_candidate() {
        let candidates = vec![
            ScoredCandidate {
                candidate: Candidate {
                    id: "c1".to_string(),
                    operation: "fix_scale".to_string(),
                    description: "Fix scale".to_string(),
                    scope: "harmony".to_string(),
                    diagnosis_metric: "scale_violation".to_string(),
                    estimated_delta: 0.05,
                    is_repair: true,
                    template_id: None,
                },
                score_delta: 0.1,
                hard_gates_pass: true,
                policy_score: 1.0,
                cost_seconds: 0.01,
            },
            ScoredCandidate {
                candidate: Candidate {
                    id: "c2".to_string(),
                    operation: "add_package".to_string(),
                    description: "Add package".to_string(),
                    scope: "transition".to_string(),
                    diagnosis_metric: "weak_transition".to_string(),
                    estimated_delta: 0.03,
                    is_repair: false,
                    template_id: None,
                },
                score_delta: 0.2,
                hard_gates_pass: true,
                policy_score: 0.9,
                cost_seconds: 0.02,
            },
            ScoredCandidate {
                candidate: Candidate {
                    id: "c3".to_string(),
                    operation: "break_loudness".to_string(),
                    description: "Break loudness".to_string(),
                    scope: "mix".to_string(),
                    diagnosis_metric: "loudness".to_string(),
                    estimated_delta: 0.5,
                    is_repair: false,
                    template_id: None,
                },
                score_delta: 0.5,
                hard_gates_pass: false,
                policy_score: 0.5,
                cost_seconds: 0.01,
            },
        ];

        #[expect(clippy::unwrap_used, reason = "Test asserts presence")]
        let best = AgentKernel::tournament_select(&candidates).unwrap();
        // c2 wins: 0.2 * 0.9 = 0.18 > c1: 0.1 * 1.0 = 0.1
        assert_eq!(best.candidate.id, "c2");
    }

    #[test]
    fn tournament_should_return_none_for_empty() {
        assert!(AgentKernel::tournament_select(&[]).is_none());
    }

    #[test]
    fn dry_run_should_check_hard_gates() {
        let config = AgentConfig {
            hard_gate_metrics: vec!["loudness_lufs".to_string()],
            hard_gate_thresholds: vec![("loudness_lufs".to_string(), -14.0)],
            ..AgentConfig::default()
        };
        let kernel = AgentKernel::with_config(ProjectId::new(), config);
        let candidate = Candidate {
            id: "c1".to_string(),
            operation: "fix_scale".to_string(),
            description: "Fix scale".to_string(),
            scope: "harmony".to_string(),
            diagnosis_metric: "scale_violation".to_string(),
            estimated_delta: 0.05,
            is_repair: true,
            template_id: None,
        };

        // Passing gate
        let metrics = vec![("loudness_lufs".to_string(), -13.0)];
        let scored = kernel.dry_run(&candidate, &metrics);
        assert!(scored.hard_gates_pass);

        // Failing gate
        let metrics = vec![("loudness_lufs".to_string(), -16.0)];
        let scored = kernel.dry_run(&candidate, &metrics);
        assert!(!scored.hard_gates_pass);
    }

    #[test]
    fn l4_learn_should_record_lesson() {
        let mut kernel = AgentKernel::new(ProjectId::new());
        let diagnosis = Diagnosis {
            metric: "scale_violation".to_string(),
            scope: "harmony".to_string(),
            current_value: 0.3,
            target_value: 0.8,
            severity: 0.7,
            is_hard_failure: false,
            evidence_uri: None,
        };
        let candidate = Candidate {
            id: "c1".to_string(),
            operation: "quantize_to_scale".to_string(),
            description: "Quantize notes".to_string(),
            scope: "harmony".to_string(),
            diagnosis_metric: "scale_violation".to_string(),
            estimated_delta: 0.05,
            is_repair: false,
            template_id: None,
        };

        kernel.l4_learn(&diagnosis, &candidate, 0.1);
        assert_eq!(kernel.lessons_learned.len(), 1);
        assert!(kernel.lessons_learned[0].successful);
    }

    #[test]
    fn full_run_loop_should_converge() {
        let config = AgentConfig {
            max_iterations: 5,
            max_no_improvement: 2,
            target_score: 0.15,
            improvement_threshold: 0.01,
            max_candidates_per_iteration: 2,
            hard_gate_metrics: vec![],
            hard_gate_thresholds: vec![],
        };
        let mut kernel = AgentKernel::with_config(ProjectId::new(), config);

        let diagnoses = vec![Diagnosis {
            metric: "test_metric".to_string(),
            scope: "test".to_string(),
            current_value: 0.0,
            target_value: 1.0,
            severity: 0.5,
            is_hard_failure: false,
            evidence_uri: None,
        }];

        let templates = vec!["fix_test".to_string()];

        let (results, state) = kernel.run(|_| diagnoses.clone(), |_| templates.clone());

        assert!(!results.is_empty());
        assert!(
            state == KernelState::Completed || state == KernelState::AwaitingHuman,
            "Expected Completed or AwaitingHuman, got {state:?}"
        );
    }

    #[test]
    fn kernel_should_escalate_when_no_diagnoses() {
        let config = AgentConfig {
            max_iterations: 3,
            max_no_improvement: 1,
            ..AgentConfig::default()
        };
        let mut kernel = AgentKernel::with_config(ProjectId::new(), config.clone());

        let empty: Vec<Diagnosis> = Vec::new();
        let (results, _) = kernel.run(|_| empty.clone(), |_| vec![]);

        assert!(!results.is_empty());
        assert!(kernel.no_improvement_streak >= config.max_no_improvement);
    }

    #[test]
    fn escalation_packet_has_all_fields() {
        let config = AgentConfig {
            max_iterations: 5,
            max_no_improvement: 3,
            ..AgentConfig::default()
        };
        let mut kernel = AgentKernel::with_config(ProjectId::new(), config);
        kernel.current_score = 0.72;

        let packet = kernel.build_escalation_packet(
            EscalationReason::BudgetExhausted,
            vec!["metric_a".to_string(), "metric_b".to_string()],
        );

        assert_eq!(packet.top_issues.len(), 2);
        assert!(packet.attempted_actions.is_empty());
        assert_eq!(packet.suggested_actions.len(), 3);
        assert!((packet.current_score - 0.72).abs() < f32::EPSILON);
    }
}
