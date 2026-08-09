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

/// Severity at or above which a diagnosis still warrants segment-level
/// development work (§13.3). Below this, the issue is a localized micro-issue.
const SEGMENT_DEV_SEVERITY_THRESHOLD: f32 = 0.5;

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
    /// Genre for calibration lookups (§11.6). When set, the loudness gate
    /// in audio diagnosis uses the genre's calibrated target instead of the
    /// generic `loudness_lufs` threshold.
    pub genre: Option<String>,
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
            genre: None,
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

    /// Build a mix/audio [`Diagnosis`] from a render's Level-4 metrics.
    ///
    /// Wires the audio engine's loudness analysis (§11.4) into the agent
    /// loop: the L1 micro-repair loop can then generate a
    /// `normalize_loudness_target` candidate for a clipping or under-loud
    /// master. Returns `None` when the master is already within tolerance.
    #[must_use]
    pub fn diagnosis_from_audio_metrics(
        &self,
        integrated_lufs: f32,
        true_peak_dbtp: f32,
    ) -> Option<Diagnosis> {
        // Genre-calibrated target per §11.6 wins over the generic gate so a
        // melodic_techno project (target −10 LUFS) is diagnosed correctly.
        let loudness_target = self
            .config
            .genre
            .as_deref()
            .and_then(scorestrata_storage::calibration::loudness_target_for)
            .or_else(|| {
                self.config
                    .hard_gate_thresholds
                    .iter()
                    .find(|(m, _)| m == "loudness_lufs")
                    .map(|(_, t)| *t)
            })
            .unwrap_or(scorestrata_storage::calibration::DEFAULT_LOUDNESS_TARGET_LUFS);
        let peak_limit = self
            .config
            .hard_gate_thresholds
            .iter()
            .find(|(m, _)| m == "true_peak_dbtp")
            .map_or(-1.0, |(_, t)| *t);

        // A mix is problematic when the true peak exceeds the limit (clipping)
        // or the loudness is far below the target (inaudible master).
        let peak_issue = true_peak_dbtp > peak_limit;
        let quiet_issue = integrated_lufs < loudness_target - 6.0;

        if peak_issue || quiet_issue {
            // Severity scales with how far the peak exceeds the limit.
            let severity = if peak_issue {
                ((true_peak_dbtp - peak_limit) / 6.0).clamp(0.3, 1.0)
            } else {
                0.4
            };
            Some(Diagnosis {
                metric: "audio.loudness".to_string(),
                scope: "mix".to_string(),
                current_value: integrated_lufs,
                target_value: loudness_target,
                severity,
                is_hard_failure: peak_issue,
                evidence_uri: Some("project://audio/lufs".to_string()),
            })
        } else {
            None
        }
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

    /// Whether the current loop level's task is complete per the exit
    /// criteria in §13.1–§13.4.
    ///
    /// Level advancement is trigger-based (task completion), not
    /// iteration-count based: a level is left only when its actual work is
    /// done, so the kernel cannot drift past unfinished work on a lucky
    /// iteration count, nor stall at a level whose task is already finished.
    ///
    /// - **L0 Orientation**: complete after the first pass (the agent has
    ///   seen the project state).
    /// - **L1 Micro-repair**: complete when no hard failures remain.
    /// - **L2 Segment development**: complete when no high-severity issue
    ///   remains.
    /// - **L3 Project refinement**: complete when the target score is reached
    ///   with hard gates passing, or there is nothing left to refine
    ///   (mirrors the §13.7 `overall_score >= target && hard_failures == 0`
    ///   break).
    /// - **L4 Learning**: terminal — the loop caps there.
    #[must_use]
    pub fn level_task_complete(&self, diagnoses: &[Diagnosis]) -> bool {
        match self.level {
            LoopLevel::Orientation => self.iteration >= 1,
            LoopLevel::MicroRepair => diagnoses.iter().all(|d| !d.is_hard_failure),
            LoopLevel::SegmentDevelopment => diagnoses
                .iter()
                .all(|d| d.severity < SEGMENT_DEV_SEVERITY_THRESHOLD),
            LoopLevel::ProjectRefinement => {
                (self.current_score >= self.config.target_score && self.hard_gates_passing)
                    || diagnoses.is_empty()
            }
            LoopLevel::Learning => true,
        }
    }

    /// Whether all configured hard gates pass against the given metrics.
    ///
    /// Extracted from [`Self::dry_run`] so the whole kernel shares one gate
    /// evaluation — the loop uses it to keep `hard_gates_passing` truthful
    /// for L3 completion.
    fn gates_pass(&self, current_metrics: &[(String, f32)]) -> bool {
        self.config.hard_gate_metrics.iter().all(|metric| {
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
        })
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

        // Mix issues (clipping / under-loud master) repair via loudness
        // normalization — the real patch op wired in the MCP layer (§10.5).
        if diagnosis.scope == "mix" || diagnosis.metric.starts_with("audio.") {
            candidates.push(Candidate {
                id: format!("cand_mix_{}_{}", self.iteration, candidates.len()),
                operation: "normalize_loudness_target".to_string(),
                description: format!(
                    "Normalize master loudness toward {} LUFS",
                    diagnosis.target_value
                ),
                scope: diagnosis.scope.clone(),
                diagnosis_metric: diagnosis.metric.clone(),
                estimated_delta: diagnosis.severity * 0.6,
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
    ///
    /// Generates segment-building candidates from the diagnoses, not just
    /// micro-repairs. A soft segment-scope diagnosis (e.g. `SEGMENT_EMPTY`,
    /// `SEGMENT_BAR_COUNT_INVALID`) previously produced **zero** candidates
    /// because `l1_micro_repair` was called with empty templates — starving
    /// the L2 loop. Structural segment metrics now map to real patch ops:
    /// `add_segment_layer` for empty segments, `set_segment_bars` for
    /// out-of-range bar counts, then fall back to micro-repair for the rest.
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
            match diagnosis.metric.as_str() {
                // An empty segment needs a layer — structural build, not a repair.
                "structure.segment_layers" => candidates.push(Candidate {
                    id: format!("cand_l2_layer_{}_{}", self.iteration, candidates.len()),
                    operation: "add_segment_layer".to_string(),
                    description: format!(
                        "Add a default layer to segment '{segment_name}' ({})",
                        diagnosis.metric
                    ),
                    scope: diagnosis.scope.clone(),
                    diagnosis_metric: diagnosis.metric.clone(),
                    estimated_delta: 0.25,
                    is_repair: true,
                    template_id: None,
                }),
                // An out-of-range bar count needs a resize.
                "structure.segment_bars" => candidates.push(Candidate {
                    id: format!("cand_l2_bars_{}_{}", self.iteration, candidates.len()),
                    operation: "set_segment_bars".to_string(),
                    description: format!(
                        "Resize segment '{segment_name}' into the valid bar range ({})",
                        diagnosis.metric
                    ),
                    scope: diagnosis.scope.clone(),
                    diagnosis_metric: diagnosis.metric.clone(),
                    estimated_delta: 0.10,
                    is_repair: true,
                    template_id: None,
                }),
                // Everything else falls back to the micro-repair generator.
                _ => candidates.extend(self.l1_micro_repair(diagnosis, &[])),
            }
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
        let hard_gates_pass = self.gates_pass(current_metrics);

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

        // Refresh the gate flag from the *current* metrics so every path
        // (main loop, early-return escalation, no-diagnoses) sees a truthful
        // value rather than a stale one from a previous iteration.
        self.hard_gates_passing = self.gates_pass(current_metrics);

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

        // Level advancement is task-completion triggered (§13.1–§13.4 exit
        // criteria), not iteration-count based: leave a level only when its
        // actual work is done.
        if self.level_task_complete(diagnoses) {
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
#[expect(
    clippy::expect_used,
    reason = "Kernel loop tests assert diagnosis/metric presence with expect"
)]
mod tests {
    use super::*;

    #[test]
    #[expect(clippy::expect_used, reason = "Test asserts diagnosis presence")]
    fn audio_diagnosis_detects_clipping_master() {
        let kernel = AgentKernel::new(ProjectId::new());
        // True peak above the -1 dBTP limit is a hard failure.
        let diag = kernel
            .diagnosis_from_audio_metrics(-12.0, 1.8)
            .expect("clipping master produces a diagnosis");
        assert_eq!(diag.scope, "mix");
        assert!(diag.is_hard_failure);
        assert!(diag.severity > 0.3);
    }

    #[test]
    #[expect(clippy::expect_used, reason = "Test asserts diagnosis presence")]
    #[expect(clippy::float_cmp, reason = "Target is an exact f32 literal")]
    fn audio_diagnosis_detects_under_loud_master() {
        let kernel = AgentKernel::new(ProjectId::new());
        // -30 LUFS is far below the -14 target.
        let diag = kernel
            .diagnosis_from_audio_metrics(-30.0, -10.0)
            .expect("under-loud master produces a diagnosis");
        assert_eq!(diag.scope, "mix");
        assert!(!diag.is_hard_failure);
        assert_eq!(diag.target_value, -14.0);
    }

    #[test]
    #[expect(clippy::expect_used, reason = "Test asserts diagnosis presence")]
    #[expect(clippy::float_cmp, reason = "Target is an exact f32 literal")]
    fn audio_diagnosis_uses_genre_calibrated_target() {
        let config = AgentConfig {
            genre: Some("melodic_techno".to_string()),
            ..AgentConfig::default()
        };
        let kernel = AgentKernel::with_config(ProjectId::new(), config);
        // A melodic_techno project targets -10 LUFS (§11.6), so a -18 LUFS
        // master is below target - 6 tolerance while still above the
        // generic -14 gate - 6 tolerance. Only the genre target flags it.
        let diag = kernel
            .diagnosis_from_audio_metrics(-18.0, -3.0)
            .expect("genre-calibrated under-loud master produces a diagnosis");
        assert_eq!(diag.scope, "mix");
        assert_eq!(diag.target_value, -10.0);
        // The generic config (-14 target) does NOT flag a -18 LUFS master
        // (above -14 - 6 = -20), proving the genre calibration matters.
        let generic = AgentKernel::new(ProjectId::new());
        assert!(generic.diagnosis_from_audio_metrics(-18.0, -3.0).is_none());
    }

    #[test]
    fn audio_diagnosis_none_when_within_tolerance() {
        let kernel = AgentKernel::new(ProjectId::new());
        assert!(kernel.diagnosis_from_audio_metrics(-14.0, -3.0).is_none());
    }

    #[test]
    #[expect(clippy::expect_used, reason = "Test asserts diagnosis presence")]
    fn l1_micro_repair_suggests_normalize_for_mix_issue() {
        let mut kernel = AgentKernel::new(ProjectId::new());
        let diag = kernel
            .diagnosis_from_audio_metrics(-12.0, 1.8)
            .expect("diagnosis exists");
        let candidates = kernel.l1_micro_repair(&diag, &[]);
        assert!(
            candidates
                .iter()
                .any(|c| c.operation == "normalize_loudness_target"),
            "mix diagnosis yields a normalize_loudness_target candidate"
        );
    }

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
            ..AgentConfig::default()
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
    fn level_task_complete_orientation_requires_first_pass() {
        let kernel = AgentKernel::new(ProjectId::new());
        assert_eq!(kernel.level, LoopLevel::Orientation);
        assert!(
            !kernel.level_task_complete(&[]),
            "orientation is not complete before any iteration"
        );
        let mut kernel = kernel;
        kernel.iteration = 1;
        assert!(
            kernel.level_task_complete(&[]),
            "orientation completes after the first pass"
        );
    }

    #[test]
    fn level_task_complete_micro_repair_requires_no_hard_failures() {
        let mut kernel = AgentKernel::new(ProjectId::new());
        kernel.level = LoopLevel::MicroRepair;
        let diag = |is_hard: bool| Diagnosis {
            metric: "m".to_string(),
            scope: "s".to_string(),
            current_value: 0.0,
            target_value: 1.0,
            severity: 0.8,
            is_hard_failure: is_hard,
            evidence_uri: None,
        };
        assert!(
            !kernel.level_task_complete(&[diag(true)]),
            "a hard failure keeps L1 active"
        );
        assert!(
            kernel.level_task_complete(&[diag(false)]),
            "no hard failures completes L1"
        );
    }

    #[test]
    fn level_task_complete_segment_dev_requires_low_severity() {
        let mut kernel = AgentKernel::new(ProjectId::new());
        kernel.level = LoopLevel::SegmentDevelopment;
        let diag = |sev: f32| Diagnosis {
            metric: "m".to_string(),
            scope: "s".to_string(),
            current_value: 0.0,
            target_value: 1.0,
            severity: sev,
            is_hard_failure: false,
            evidence_uri: None,
        };
        assert!(
            !kernel.level_task_complete(&[diag(0.8)]),
            "a high-severity issue keeps L2 active"
        );
        assert!(
            kernel.level_task_complete(&[diag(0.3)]),
            "only low-severity issues complete L2"
        );
    }

    #[test]
    fn level_task_complete_project_refinement_requires_target_and_gates() {
        let config = AgentConfig {
            target_score: 0.8,
            ..AgentConfig::default()
        };
        let mut kernel = AgentKernel::with_config(ProjectId::new(), config);
        kernel.level = LoopLevel::ProjectRefinement;
        kernel.current_score = 0.85;
        kernel.hard_gates_passing = true;
        let diag = Diagnosis {
            metric: "m".to_string(),
            scope: "s".to_string(),
            current_value: 0.0,
            target_value: 1.0,
            severity: 0.3,
            is_hard_failure: false,
            evidence_uri: None,
        };
        assert!(
            kernel.level_task_complete(std::slice::from_ref(&diag)),
            "target + gates completes L3"
        );
        kernel.hard_gates_passing = false;
        assert!(
            !kernel.level_task_complete(&[diag]),
            "failing gates keep L3 active"
        );
        assert!(
            kernel.level_task_complete(&[]),
            "nothing left to refine completes L3"
        );
    }

    #[test]
    fn run_iteration_advances_by_task_completion_not_iteration_count() {
        let config = AgentConfig {
            max_iterations: 10,
            max_no_improvement: 4,
            target_score: 5.0, // unreachable so budget, not target, bounds the run
            improvement_threshold: 0.01,
            max_candidates_per_iteration: 3,
            ..AgentConfig::default()
        };
        let mut kernel = AgentKernel::with_config(ProjectId::new(), config);

        // A persistent hard failure in the mix: the kernel must stay at L1
        // micro-repair no matter how many successful iterations pass.
        let hard = Diagnosis {
            metric: "audio.loudness".to_string(),
            scope: "mix".to_string(),
            current_value: -4.0,
            target_value: -14.0,
            severity: 0.8,
            is_hard_failure: true,
            evidence_uri: None,
        };
        let templates = vec!["normalize_loudness_target".to_string()];
        let metrics = vec![
            ("loudness_lufs".to_string(), -14.0),
            ("true_peak_dbtp".to_string(), -1.0),
        ];

        // Iteration 1: L0 orientation completes after the first pass.
        let result = kernel
            .run_iteration(std::slice::from_ref(&hard), &templates, &metrics)
            .expect("iteration runs");
        assert!(result.improved);
        assert_eq!(
            kernel.level,
            LoopLevel::MicroRepair,
            "L0 completes after the first pass → L1"
        );

        // Iterations 2 and 3: the hard failure persists, so the kernel stays
        // at L1 — even at iteration 2, where the old count-based rule
        // (`iteration.is_multiple_of(2)`) would have advanced to L2.
        for _ in 0..2 {
            let result = kernel
                .run_iteration(std::slice::from_ref(&hard), &templates, &metrics)
                .expect("iteration runs");
            assert!(result.improved);
        }
        assert_eq!(kernel.iteration, 3);
        assert_eq!(
            kernel.level,
            LoopLevel::MicroRepair,
            "a hard failure pins the kernel at L1 regardless of iteration count"
        );

        // Hard failure resolved: the next iteration completes L1 and advances
        // to L2 segment development.
        let soft = Diagnosis {
            metric: "arrangement.transition_strength".to_string(),
            scope: "transition".to_string(),
            current_value: 0.4,
            target_value: 0.8,
            severity: 0.3,
            is_hard_failure: false,
            evidence_uri: None,
        };
        let result = kernel
            .run_iteration(&[soft], &templates, &metrics)
            .expect("iteration runs");
        assert!(result.improved);
        assert_eq!(
            kernel.level,
            LoopLevel::SegmentDevelopment,
            "L1 task done → L2"
        );
    }

    #[test]
    fn l2_segment_development_generates_candidates_without_templates() {
        let mut kernel = AgentKernel::new(ProjectId::new());
        // Empty segment diagnosis (structure.segment_layers) is a soft
        // segment-scope issue that previously produced ZERO candidates at L2
        // because l1_micro_repair received empty templates.
        let empty_segment = Diagnosis {
            metric: "structure.segment_layers".to_string(),
            scope: "segment".to_string(),
            current_value: 0.0,
            target_value: 1.0,
            severity: 0.6,
            is_hard_failure: false,
            evidence_uri: None,
        };
        let candidates = kernel.l2_segment_development("intro", &[empty_segment]);
        assert!(
            candidates
                .iter()
                .any(|c| c.operation == "add_segment_layer"),
            "empty segment yields an add_segment_layer candidate"
        );

        let bad_bars = Diagnosis {
            metric: "structure.segment_bars".to_string(),
            scope: "segment".to_string(),
            current_value: 128.0,
            target_value: 8.0,
            severity: 0.4,
            is_hard_failure: false,
            evidence_uri: None,
        };
        let candidates = kernel.l2_segment_development("intro", &[bad_bars]);
        assert!(
            candidates.iter().any(|c| c.operation == "set_segment_bars"),
            "bar-count violation yields a set_segment_bars candidate"
        );

        // Non-structural diagnoses still fall back to micro-repair.
        let mix = Diagnosis {
            metric: "audio.loudness".to_string(),
            scope: "mix".to_string(),
            current_value: -4.0,
            target_value: -14.0,
            severity: 0.8,
            is_hard_failure: true,
            evidence_uri: None,
        };
        let candidates = kernel.l2_segment_development("intro", &[mix]);
        assert!(
            candidates
                .iter()
                .any(|c| c.operation == "normalize_loudness_target"),
            "mix diagnosis still falls back to micro-repair at L2"
        );
    }

    #[test]
    fn run_iteration_refreshes_hard_gates_passing_from_metrics() {
        let config = AgentConfig {
            max_iterations: 4,
            max_no_improvement: 2,
            target_score: 0.95,
            improvement_threshold: 0.01,
            hard_gate_metrics: vec!["loudness_lufs".to_string(), "true_peak_dbtp".to_string()],
            hard_gate_thresholds: vec![
                ("loudness_lufs".to_string(), -14.0),
                ("true_peak_dbtp".to_string(), -1.0),
            ],
            ..AgentConfig::default()
        };
        let mut kernel = AgentKernel::with_config(ProjectId::new(), config);
        let diag = Diagnosis {
            metric: "arrangement.transition_strength".to_string(),
            scope: "transition".to_string(),
            current_value: 0.4,
            target_value: 0.8,
            severity: 0.3,
            is_hard_failure: false,
            evidence_uri: None,
        };
        let templates = vec!["add_transition_package".to_string()];

        // True peak below the -1 dBTP gate → gates not passing.
        let failing = vec![
            ("loudness_lufs".to_string(), -13.0),
            ("true_peak_dbtp".to_string(), -3.0),
        ];
        let result = kernel
            .run_iteration(std::slice::from_ref(&diag), &templates, &failing)
            .expect("iteration runs");
        assert!(result.improved);
        assert!(
            !kernel.hard_gates_passing,
            "over-gate metrics keep gates failing"
        );

        // Healthy metrics → gates pass and the flag is refreshed.
        // True peak must be ABOVE -1 dBTP to pass (a more negative value
        // would still violate the gate).
        let passing = vec![
            ("loudness_lufs".to_string(), -13.0),
            ("true_peak_dbtp".to_string(), -0.5),
        ];
        let result = kernel
            .run_iteration(&[diag], &templates, &passing)
            .expect("iteration runs");
        assert!(result.improved);
        assert!(
            kernel.hard_gates_passing,
            "healthy metrics refresh hard_gates_passing to true"
        );
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
            ..AgentConfig::default()
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
