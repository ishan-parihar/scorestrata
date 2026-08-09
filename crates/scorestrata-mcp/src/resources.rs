//! MCP resources — inspectable state that agents can query.
//!
//! Resources follow the "tools mutate, resources inspect" principle.
//! Returns summary-level data by default, with drill-down via scoped lenses.
//!
//! Implements architecture spec §10.3 (system resources) and §10.4
//! (project resources and engineering resources).
//!
//! Resource renderers are long formatting functions converting domain types
//! to inspectable text, so the pedantic size/doc/cast lints are expected.
#![expect(
    clippy::cast_possible_truncation,
    clippy::cast_possible_wrap,
    clippy::too_long_first_doc_paragraph,
    clippy::too_many_lines,
    reason = "Resource renderers are long formatting functions with bounded wire-type casts"
)]

use scorestrata_core::error::Severity;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

// ── System Resources (§10.3) ────────────────────────────────────────

/// System contracts — versioned schemas for tools, receipts, diagnostics.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SystemContracts {
    /// Schema version.
    pub schema_version: String,
    /// Tool contracts.
    pub tools: Vec<ToolContract>,
    /// Receipt contracts.
    pub receipts: Vec<ReceiptContract>,
    /// Diagnostic contracts.
    pub diagnostics: Vec<DiagnosticContract>,
}

/// A tool contract — metadata about a tool's schema and constraints.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolContract {
    /// Tool name.
    pub name: String,
    /// Version.
    pub version: String,
    /// Stability level.
    pub stability: String,
    /// Safety class.
    pub safety_class: String,
    /// Whether dry-run is supported.
    pub supports_dry_run: bool,
    /// Required parameters.
    pub required_params: Vec<String>,
    /// Optional parameters.
    pub optional_params: Vec<String>,
}

/// A receipt contract — what a tool returns.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReceiptContract {
    /// Receipt type name.
    pub name: String,
    /// Fields in the receipt.
    pub fields: Vec<String>,
}

/// A diagnostic contract — diagnostic packet schema.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DiagnosticContract {
    /// Diagnostic code.
    pub code: String,
    /// Severity range.
    pub severity_range: String,
    /// Fields.
    pub fields: Vec<String>,
}

/// System onboarding packet — orientation data for new agents.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OnboardingPacket {
    /// System map (high-level architecture overview).
    pub system_map: String,
    /// Capability manifest.
    pub capabilities: String,
    /// Schema catalog.
    pub schemas: String,
    /// Tool catalog.
    pub tools: String,
    /// Playbooks.
    pub playbooks: Vec<String>,
    /// Failure taxonomy.
    pub failure_taxonomy: Vec<FailureTaxonomyEntry>,
    /// Budget templates.
    pub budget_templates: Vec<BudgetTemplate>,
    /// Success criteria.
    pub success_criteria: Vec<String>,
    /// Golden examples.
    pub golden_examples: Vec<String>,
    /// Do-not-do list.
    pub do_not_do: Vec<String>,
}

/// A failure taxonomy entry — categorized failure type with remediation.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FailureTaxonomyEntry {
    /// Failure code.
    pub code: String,
    /// Category (e.g. `harmony`, `structure`, `mix`, `render`).
    pub category: String,
    /// Description.
    pub description: String,
    /// Suggested remediation.
    pub remediation: Vec<String>,
    /// Severity.
    pub severity: Severity,
}

/// A budget template — pre-configured budget envelope.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BudgetTemplate {
    /// Template name.
    pub name: String,
    /// Description.
    pub description: String,
    /// Max iterations.
    pub max_iterations: u32,
    /// Max tool calls.
    pub max_tool_calls: u32,
    /// Max render seconds.
    pub max_render_seconds: f64,
    /// Max CPU seconds.
    pub max_cpu_seconds: f64,
}

// ── Project Resources (§10.4) ───────────────────────────────────────

/// Policy profile view — current policy state for a project.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PolicyProfileView {
    /// Project ID.
    pub project_id: String,
    /// Whether destructive commands are allowed.
    pub allow_destructive_commands: bool,
    /// Whether snapshot is required before patch.
    pub require_snapshot_before_patch: bool,
    /// Max out-of-scale ratio.
    pub max_out_of_scale_ratio: f32,
    /// Min chorus energy delta.
    pub min_chorus_energy_delta: f32,
    /// Max true peak dBTP.
    pub max_true_peak_dbtp: f32,
    /// Max candidate renders per segment.
    pub max_candidate_renders_per_segment: u32,
}

impl PolicyProfileView {
    /// Create from a core `PolicyProfile`.
    #[must_use]
    pub fn from_policy(project_id: &str, policy: &scorestrata_core::policy::PolicyProfile) -> Self {
        Self {
            project_id: project_id.to_string(),
            allow_destructive_commands: policy.allow_destructive_commands,
            require_snapshot_before_patch: policy.require_snapshot_before_patch,
            max_out_of_scale_ratio: policy.max_out_of_scale_ratio,
            min_chorus_energy_delta: policy.min_chorus_energy_delta,
            max_true_peak_dbtp: policy.max_true_peak_dbtp,
            max_candidate_renders_per_segment: policy.max_candidate_renders_per_segment,
        }
    }
}

/// Provenance manifest view — reproducibility metadata for a project.
///
/// Derived from [`scorestrata_storage::provenance::ProvenanceManifest`]
/// (architecture §15.2): every artifact must be reproducible from brief,
/// config, policy, seed, asset hashes, engine versions, and render settings.
/// The [`ProvenanceView::provenance_hash`] is the deterministic composite
/// hash used as the content-addressed cache key (§9.3).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProvenanceView {
    /// Project ID.
    pub project_id: String,
    /// Compiler version.
    pub compiler_version: String,
    /// Validator version.
    pub validator_version: String,
    /// Audio engine version.
    pub audio_engine_version: String,
    /// DSP graph version.
    pub dsp_graph_version: String,
    /// Seed used for deterministic generation.
    pub seed: Option<u64>,
    /// Asset hashes.
    pub asset_hashes: BTreeMap<String, String>,
    /// Render settings.
    pub render_settings: BTreeMap<String, String>,
    /// Policy profile hash.
    pub policy_hash: String,
    /// Brief hash.
    pub brief_hash: String,
    /// Config hash.
    pub config_hash: String,
    /// Base snapshot hash.
    pub snapshot_hash: String,
    /// Deterministic composite provenance hash (cache key, §9.3/§15.2).
    pub provenance_hash: String,
    /// Whether all required provenance inputs are present.
    pub complete: bool,
}

impl ProvenanceView {
    /// Build the view from a real [`ProvenanceManifest`].
    #[must_use]
    pub fn from_manifest(manifest: &scorestrata_storage::provenance::ProvenanceManifest) -> Self {
        let provenance_hash = manifest.provenance_hash();
        Self {
            project_id: manifest.project_id.clone(),
            compiler_version: manifest.compiler_version.clone(),
            validator_version: manifest.validator_version.clone(),
            audio_engine_version: manifest.audio_engine_version.clone(),
            dsp_graph_version: manifest.dsp_graph_version.clone(),
            seed: manifest.seed,
            asset_hashes: manifest.asset_hashes.clone(),
            render_settings: manifest.render_settings.clone(),
            policy_hash: manifest.policy_hash.clone(),
            brief_hash: manifest.brief_hash.clone(),
            config_hash: manifest.config_hash.clone(),
            snapshot_hash: manifest.snapshot_hash.clone(),
            complete: manifest.is_complete(),
            provenance_hash,
        }
    }
}

/// Render manifest view — reproducibility receipt for a rendered artifact.
///
/// Derived from [`scorestrata_storage::render_manifest::RenderManifest`]
/// (architecture §17.2): every render produces a manifest tying the artifact's
/// SHA-256 to its reproducibility inputs (source snapshot, command, engine
/// versions, seed, asset hashes). The [`RenderManifestView::manifest_hash`]
/// is the deterministic reproducibility fingerprint used as the content-
/// addressed cache key (§9.3).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RenderManifestView {
    /// Artifact ID.
    pub artifact_id: String,
    /// Artifact type (`master_wav`, `stem`, `preview`).
    pub artifact_type: String,
    /// Output path.
    pub path: String,
    /// SHA-256 of the artifact bytes.
    pub sha256: String,
    /// Sample rate.
    pub sample_rate: u32,
    /// Channel count.
    pub channels: u16,
    /// Duration in seconds.
    pub duration_seconds: f64,
    /// Snapshot the render was produced from.
    pub source_snapshot_id: String,
    /// Command that triggered the render.
    pub command_id: String,
    /// Compiler crate version.
    pub compiler_version: String,
    /// Audio engine crate version.
    pub audio_engine_version: String,
    /// Seed.
    pub seed: Option<u64>,
    /// Asset hashes.
    pub asset_hashes: BTreeMap<String, String>,
    /// Deterministic reproducibility fingerprint (cache key, §9.3/§17.2).
    pub manifest_hash: String,
    /// Whether all required manifest fields are present.
    pub complete: bool,
}

impl RenderManifestView {
    /// Build the view from a real [`RenderManifest`].
    #[must_use]
    pub fn from_manifest(manifest: &scorestrata_storage::render_manifest::RenderManifest) -> Self {
        Self {
            artifact_id: manifest.artifact_id.clone(),
            artifact_type: manifest.artifact_type.clone(),
            path: manifest.path.clone(),
            sha256: manifest.sha256.clone(),
            sample_rate: manifest.sample_rate,
            channels: manifest.channels,
            duration_seconds: manifest.duration_seconds,
            source_snapshot_id: manifest.source_snapshot_id.clone(),
            command_id: manifest.command_id.clone(),
            compiler_version: manifest.compiler_version.clone(),
            audio_engine_version: manifest.audio_engine_version.clone(),
            seed: manifest.seed,
            asset_hashes: manifest.asset_hashes.clone(),
            complete: manifest.is_complete(),
            manifest_hash: manifest.manifest_hash(),
        }
    }
}

/// Ledger summary — event counts and recent activity.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LedgerSummary {
    /// Project ID.
    pub project_id: String,
    /// Total event count.
    pub total_events: u32,
    /// Event counts by type.
    pub events_by_type: BTreeMap<String, u32>,
    /// Most recent event timestamp.
    pub last_event_timestamp: Option<String>,
    /// Event rate (events per minute).
    pub events_per_minute: f32,
}

/// Branches summary — active and archived branches.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BranchesSummary {
    /// Project ID.
    pub project_id: String,
    /// Active branches.
    pub active_branches: Vec<BranchSummary>,
    /// Total archived branches.
    pub archived_count: u32,
}

/// Summary of a single branch.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BranchSummary {
    /// Branch ID.
    pub id: String,
    /// Branch name.
    pub name: String,
    /// Whether this is the main branch.
    pub is_main: bool,
    /// Status.
    pub status: String,
    /// Number of candidates on this branch.
    pub candidate_count: u32,
}

/// Candidates summary — active candidates for tournament selection.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CandidatesSummary {
    /// Project ID.
    pub project_id: String,
    /// Total candidate count.
    pub total_count: u32,
    /// Candidates by status.
    pub by_status: BTreeMap<String, u32>,
    /// Top candidates (highest score).
    pub top_candidates: Vec<CandidateBrief>,
}

/// Brief info about a candidate.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CandidateBrief {
    /// Candidate ID.
    pub id: String,
    /// Description.
    pub description: String,
    /// Overall score.
    pub overall_score: Option<f32>,
    /// Status.
    pub status: String,
}

/// Memory lessons view — learned repair strategies.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MemoryLessonsView {
    /// Project ID.
    pub project_id: String,
    /// Total lesson count.
    pub total_lessons: u32,
    /// Lessons by trigger.
    pub by_trigger: BTreeMap<String, u32>,
    /// Top lessons by confidence.
    pub top_lessons: Vec<LessonBrief>,
}

/// Brief info about a lesson.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LessonBrief {
    /// Lesson ID.
    pub id: String,
    /// Trigger diagnostic.
    pub trigger: String,
    /// Successful patch.
    pub successful_patch: Option<String>,
    /// Confidence.
    pub confidence: f32,
    /// Genre.
    pub genre: Option<String>,
}

/// Memory repair templates view — ranked patch templates.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RepairTemplatesView {
    /// Project ID.
    pub project_id: String,
    /// Total template count.
    pub total_templates: u32,
    /// Top templates by success rate.
    pub top_templates: Vec<TemplateBrief>,
}

/// Brief info about a repair template.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TemplateBrief {
    /// Template ID.
    pub id: String,
    /// Operation name.
    pub operation: String,
    /// Scope.
    pub scope: String,
    /// Success rate.
    pub success_rate: f32,
    /// Usage count.
    pub usage_count: u32,
}

// ── Engineering Resources (§10.4) ───────────────────────────────────

/// Engineering specs list.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EngineeringSpecs {
    /// All specs.
    pub specs: Vec<EngineeringSpecBrief>,
}

/// Brief info about an engineering spec.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EngineeringSpecBrief {
    /// Spec ID.
    pub id: String,
    /// Title.
    pub title: String,
    /// Component.
    pub component: String,
    /// Priority.
    pub priority: String,
    /// Status.
    pub status: String,
}

/// Next engineering task.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NextTask {
    /// Task ID.
    pub task_id: String,
    /// Title.
    pub title: String,
    /// Status.
    pub status: String,
    /// Whether dependencies are met.
    pub dependencies_met: bool,
    /// Acceptance criteria.
    pub acceptance_criteria: Vec<String>,
    /// Test commands.
    pub test_commands: Vec<String>,
}

/// Engineering eval summary.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EngineeringEvalSummary {
    /// Total test count.
    pub total_tests: u32,
    /// Passing tests.
    pub passing_tests: u32,
    /// Failing tests.
    pub failing_tests: u32,
    /// Golden test pass rate.
    pub golden_pass_rate: f32,
    /// Last run timestamp.
    pub last_run: Option<String>,
}

/// Traceability entry — links spec → task → code → tests → docs.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TraceabilityEntry {
    /// Feature ID.
    pub feature_id: String,
    /// Spec ID.
    pub spec_id: Option<String>,
    /// Task ID.
    pub task_id: Option<String>,
    /// Code files.
    pub code_files: Vec<String>,
    /// Test files.
    pub test_files: Vec<String>,
    /// Golden tests.
    pub golden_tests: Vec<String>,
    /// Doc files.
    pub doc_files: Vec<String>,
    /// MCP tool names.
    pub mcp_tools: Vec<String>,
}

// ── Timeline View (G1) ──────────────────────────────────────────────

/// A section row in the layered timeline view (arrangement header).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TimelineSection {
    /// Section name (segment name).
    pub name: String,
    /// Section role (intro, build, drop, …).
    pub role: String,
    /// Start bar (0-indexed).
    pub start_bar: u32,
    /// Duration in bars.
    pub bars: u32,
    /// Target energy for this section (0..1), if configured.
    pub energy: Option<f32>,
    /// Layer names active in this section.
    pub layers: Vec<String>,
}

/// A note row in a layer lane.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TimelineNote {
    /// Absolute start tick.
    pub start_tick: u64,
    /// Bar number the note starts in (0-indexed).
    pub bar: u32,
    /// MIDI pitch.
    pub pitch: u8,
    /// Pitch name (e.g. `C6`).
    pub name: String,
    /// Duration in ticks.
    pub duration_ticks: u64,
    /// Velocity (0–127).
    pub velocity: u8,
}

/// An automation lane shown in the timeline view (G4) — the parameter curve
/// a layer rides, exactly like the automation clip rows in a DAW arrange view.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AutomationLaneView {
    /// Parameter name (`cutoff`, `volume`, `pan`, `send`).
    pub param: String,
    /// MIDI CC controller used in the timeline.
    pub controller: u8,
    /// Curve points: `(tick, value 0–1)`.
    pub points: Vec<(u64, f32)>,
}

/// A layer lane in the timeline view — one instrument's notes over time.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TimelineLayer {
    /// Layer ID.
    pub layer_id: String,
    /// Layer name.
    pub name: String,
    /// Instrument.
    pub instrument: String,
    /// Segment this layer belongs to.
    pub segment: String,
    /// Lowest MIDI octave occupied by this layer's notes (derived).
    pub min_octave: i8,
    /// Highest MIDI octave occupied by this layer's notes (derived).
    pub max_octave: i8,
    /// Total note count.
    pub note_count: u32,
    /// Note rows (capped for payload size).
    pub notes: Vec<TimelineNote>,
    /// Automation lanes (G4) — per-param curves riding on this layer.
    #[serde(default)]
    pub automation: Vec<AutomationLaneView>,
    /// Insert FX chain summary (G5) — HPF / EQ bands / comp / sidechain / sends.
    #[serde(default)]
    pub fx: Option<String>,
}

/// The layered timeline view — the agent's "arrange view".
///
/// Sections (arrangement rows) × layers (piano-roll lanes) with note events,
/// derived purely from the project + compiled timeline. This is the G1
/// deliverable that makes the composition *visible* to agents.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TimelineView {
    /// Project ID.
    pub project_id: String,
    /// Project name.
    pub name: String,
    /// Genre.
    pub genre: String,
    /// BPM.
    pub bpm: u32,
    /// Key.
    pub key: String,
    /// Total bars.
    pub total_bars: u32,
    /// Arrangement sections in order.
    pub sections: Vec<TimelineSection>,
    /// Layer lanes (one per segment-layer, in arrangement order).
    pub layers: Vec<TimelineLayer>,
}

/// A section archetype in an arrangement template (G6) — the DAW convention
/// of composing in section shapes (intro → build → drop → breakdown → outro)
/// with production-grade bar counts and an energy arc.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SectionTemplate {
    /// Section role name (`intro`, `build`, `drop`, `breakdown`, `outro`).
    pub role: String,
    /// Bar count for this section.
    pub bars: u32,
    /// Energy target (0.0–1.0) — rises through the build, peaks at the drop,
    /// drops in the breakdown, and lands on the outro.
    pub energy: f32,
    /// Layers expected active in this section (instrument hints the planner
    /// uses to fill it).
    pub layers: Vec<String>,
}

/// One guided step in a production playbook: what to do, which tools to
/// call, and what "done" looks like.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PlaybookStep {
    /// Step name (e.g. "Compose the foundational loop").
    pub name: String,
    /// Why this step matters (the producer's intent).
    pub purpose: String,
    /// The exact tool(s) / CLI command(s) to call for this step.
    pub tools: Vec<String>,
    /// What must be true before moving on.
    pub exit_criteria: String,
}

/// A genre production playbook — the "skills" layer.
///
/// Served to agents via `system://playbooks/<genre>` so a producer is
/// guided through the direct workflow (configure → compose → arrange →
/// register-discipline → automate → mix → master → validate) with the exact
/// tools to call at every step, instead of being handed a bare tool list.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProductionPlaybook {
    /// Genre this playbook targets.
    pub genre: String,
    /// One-paragraph production strategy for the genre.
    pub strategy: String,
    /// The arrangement shape to aim for (bars per section).
    pub arrangement_shape: String,
    /// Per-instrument octave registers (§1.3 discipline).
    pub octave_map: Vec<String>,
    /// The ordered guided workflow.
    pub steps: Vec<PlaybookStep>,
    /// Mixing/mastering guidance specific to the genre.
    pub mix_master: String,
    /// Common mistakes to avoid (the "do not do" list).
    pub common_mistakes: Vec<String>,
}

/// The guided production playbook for a genre (falls back to the general
/// workflow for any genre without a specialized table).
#[must_use]
pub fn production_playbook_for(genre: &str) -> ProductionPlaybook {
    let g = genre.to_lowercase();
    if g.contains("rock") || g.contains("metal") || g.contains("nu_metal") || g.contains("punk") {
        rock_playbook(genre)
    } else if g.contains("house") {
        house_playbook(genre)
    } else if g.contains("ambient")
        || g.contains("drone")
        || g.contains("lo-fi")
        || g.contains("lofi")
    {
        ambient_playbook(genre)
    } else if g.contains("techno") || g.contains("trance") {
        techno_playbook(genre)
    } else if g.contains("jazz") || g.contains("swing") || g.contains("bossa") {
        jazz_playbook(genre)
    } else if g.contains("hip") || g.contains("trap") || g.contains("rap") || g.contains("phonk") {
        hiphop_playbook(genre)
    } else if g.contains("pop") {
        pop_playbook(genre)
    } else if g.contains("funk") || g.contains("soul") || g.contains("disco") {
        funk_playbook(genre)
    } else if g.contains("blues") {
        blues_playbook(genre)
    } else if g.contains("synthwave") || g.contains("retrowave") || g.contains("vaporwave") {
        synthwave_playbook(genre)
    } else if g.contains("drum and bass")
        || g.contains("drum_and_bass")
        || g.contains("dnb")
        || g.contains("jungle")
        || g.contains("breakbeat")
    {
        dnb_playbook(genre)
    } else if g.contains("cinematic") || g.contains("orchestral") || g.contains("classical") {
        cinematic_playbook(genre)
    } else {
        general_playbook(genre)
    }
}

/// The rock/nu-metal playbook (Linkin Park stress-test workflow).
fn rock_playbook(genre: &str) -> ProductionPlaybook {
    ProductionPlaybook {
        genre: genre.to_string(),
        strategy: "Rock/nu-metal lives on the verse→chorus energy ramp: a "
            .to_string()
            + "backbeat drum kit, a driving root-fifth bass, palm-muted "
            + "power-chord guitars that open up into the chorus, and a "
            + "vocal-range lead. Keep the guitar octaves 2–4 so the riff "
            + "never collides with the bass (1–2) or the vocal (4–6).",
        arrangement_shape: "intro 8 · verse 8 · chorus 32 · breakdown 8 · outro 8".to_string(),
        octave_map: vec![
            "Sub bass / kick: octaves 1–2, dead center".to_string(),
            "Bass guitar: octaves 1–2, sidechain optional (rock bass is driven, not ducked)".to_string(),
            "Rhythm/lead guitar (power chords): octaves 2–4".to_string(),
            "Lead vocal / melody: octaves 4–6".to_string(),
            "Hats / cymbals: octaves 6–8, wide".to_string(),
        ],
        steps: vec![
            PlaybookStep {
                name: "Set the scene".to_string(),
                purpose: "A rock brief must land on the rock family with a rock tempo/key.".to_string(),
                tools: vec![
                    "generate_config_from_brief".to_string(),
                    "validate_config".to_string(),
                ],
                exit_criteria: "Config genre is rock/nu_metal, BPM 90–130, minor key.".to_string(),
            },
            PlaybookStep {
                name: "Compose the foundational loops".to_string(),
                purpose: "Create the 1-bar atoms: backbeat drums, driving bass, power-chord guitar chug, vocal lead.".to_string(),
                tools: vec![
                    "create_pattern".to_string(),
                    "write_pattern_notes".to_string(),
                    "write_pattern_steps".to_string(),
                ],
                exit_criteria: "Drums have a snare backbeat on 2&4; guitar is root+fifth dyads in octaves 2–4.".to_string(),
            },
            PlaybookStep {
                name: "Arrange the band".to_string(),
                purpose: "Lay out verse → chorus with the full band peaking in the chorus.".to_string(),
                tools: vec![
                    "apply_arrangement_template".to_string(),
                    "extend_arrangement".to_string(),
                    "place_pattern_in_segment".to_string(),
                ],
                exit_criteria: "Arrangement covers the target bar count with a Chorus carrying guitar + lead.".to_string(),
            },
            PlaybookStep {
                name: "Enforce octave discipline".to_string(),
                purpose: "Assign registers so layers never fight (guitar 2–4, bass 1–2, lead 4–6).".to_string(),
                tools: vec![
                    "set_layer_register".to_string(),
                    "run_validation".to_string(),
                ],
                exit_criteria: "No register-violation diagnostics; timeline shows per-layer octave bands.".to_string(),
            },
            PlaybookStep {
                name: "Automate the dynamics".to_string(),
                purpose: "Filter-cutoff / volume sweeps on guitars and lead give the rock the push-and-pull.".to_string(),
                tools: vec![
                    "write_automation".to_string(),
                    "repeat_pattern_in_segment".to_string(),
                ],
                exit_criteria: "Timeline has control-change lanes; the build visibly opens up.".to_string(),
            },
            PlaybookStep {
                name: "Mix the band".to_string(),
                purpose: "Saturated guitars, punchy drum compression, driven bass.".to_string(),
                tools: vec![
                    "set_track_fx".to_string(),
                    "set_track_mix".to_string(),
                    "apply_genre_mix_preset".to_string(),
                ],
                exit_criteria: "Every layer has an FX chain; the rock preset is applied.".to_string(),
            },
            PlaybookStep {
                name: "Master".to_string(),
                purpose: "Loudness target with a limiter ceiling.".to_string(),
                tools: vec!["ss mix master".to_string(), "ss render".to_string()],
                exit_criteria: "True peak ≤ −1 dBTP; LUFS near −10.".to_string(),
            },
            PlaybookStep {
                name: "Validate against the reference".to_string(),
                purpose: "The spectral gate measures the render against a real track's profile.".to_string(),
                tools: vec![
                    "import_reference_profile".to_string(),
                    "render_project".to_string(),
                    "get_audit_summary".to_string(),
                ],
                exit_criteria: "Render deviation from the reference profile is below the gate.".to_string(),
            },
        ],
        mix_master: "Rock drums get aggressive compression (threshold −16 dB, ratio 4:1); "
            .to_string()
            + "guitars get a presence lift at 2.5 kHz with short verb/delay sends; "
            + "the bass is driven (boosted low-mid) and NOT sidechained; master to "
            + "−1 dBTP ceiling.",
        common_mistakes: vec![
            "Putting power chords above octave 4 (they collide with the vocal)".to_string(),
            "Sidechaining the rock bass to the kick (that is an electronic-music move)".to_string(),
            "Writing a chorus with no guitar — the chorus is where the band peaks".to_string(),
            "Leaving hats out of octave 6+ — rock hats need air".to_string(),
        ],
    }
}

/// The house playbook.
fn house_playbook(genre: &str) -> ProductionPlaybook {
    ProductionPlaybook {
        genre: genre.to_string(),
        strategy: "House is a swung, offbeat groove: 4/4 kick, clap on 2&4, ".to_string()
            + "offbeat stabs, pumping root-octave bass, filtered chords. "
            + "Sidechain the bass and pads to the kick so the groove breathes.",
        arrangement_shape: "intro 16 · build 8 · drop 16 · breakdown 8 · outro 8".to_string(),
        octave_map: vec![
            "Kick: 20–60 Hz, mono".to_string(),
            "Bass: octaves 1–2, sidechained to kick".to_string(),
            "Chords/pads: octaves 3–5, voiced up".to_string(),
            "Stabs/leads: octaves 5–7".to_string(),
            "Hats: octaves 6–8, swung offbeats".to_string(),
        ],
        steps: vec![
            PlaybookStep {
                name: "Compose the groove".to_string(),
                purpose: "The swung offbeat hat + pump is what makes it house.".to_string(),
                tools: vec![
                    "create_pattern".to_string(),
                    "write_pattern_steps".to_string(),
                ],
                exit_criteria: "Kick 1&3, clap 2&4, swung offbeat hats, pumping bass.".to_string(),
            },
            PlaybookStep {
                name: "Arrange intro→drop".to_string(),
                purpose: "Strip the bed down in the intro, peak at the drop.".to_string(),
                tools: vec![
                    "apply_arrangement_template".to_string(),
                    "extend_arrangement".to_string(),
                ],
                exit_criteria: "Drop carries the full stack; intro is kick/hats only.".to_string(),
            },
            PlaybookStep {
                name: "Sidechain the groove".to_string(),
                purpose: "Bass and pads duck on every kick for the signature pump.".to_string(),
                tools: vec![
                    "set_track_fx".to_string(),
                    "apply_genre_mix_preset".to_string(),
                ],
                exit_criteria: "Bass + pad layers have sidechain_from_kick set.".to_string(),
            },
            PlaybookStep {
                name: "Master to club level".to_string(),
                purpose: "Loud and wide for the dancefloor.".to_string(),
                tools: vec!["ss mix master".to_string(), "ss render".to_string()],
                exit_criteria: "True peak ≤ −1 dBTP, LUFS −8 to −10.".to_string(),
            },
        ],
        mix_master: "Sidechain everything melodic to the kick; keep the low-mid ".to_string()
            + "scooped on non-bass elements; master hot (−8 to −10 LUFS).",
        common_mistakes: vec![
            "Forgetting the swing on offbeat hats".to_string(),
            "Not sidechaining the bass (loses the pump)".to_string(),
            "Voicing chords low and muddy (keep them 3–5)".to_string(),
        ],
    }
}

/// The ambient playbook.
fn ambient_playbook(genre: &str) -> ProductionPlaybook {
    ProductionPlaybook {
        genre: genre.to_string(),
        strategy: "Ambient is space and evolution: sparse percussion, a drone ".to_string()
            + "bass, bell tones and evolving pads. Let layers breathe with "
            + "generous reverb and slow automation.",
        arrangement_shape: "intro 32 · build 8 · drop 16 · breakdown 16 · outro 16".to_string(),
        octave_map: vec![
            "Drone bass: octave 1, sustained".to_string(),
            "Pads: octaves 3–5, wide".to_string(),
            "Bells/leads: octaves 5–7, sparse".to_string(),
        ],
        steps: vec![
            PlaybookStep {
                name: "Lay the drone bed".to_string(),
                purpose: "A sustained bass note anchors the whole piece.".to_string(),
                tools: vec![
                    "write_pattern_notes".to_string(),
                    "place_pattern_in_segment".to_string(),
                ],
                exit_criteria: "A long-sustained drone note in octave 1.".to_string(),
            },
            PlaybookStep {
                name: "Add evolving pads".to_string(),
                purpose: "Pads in 3–5 with slow cutoff automation.".to_string(),
                tools: vec![
                    "write_pattern_notes".to_string(),
                    "write_automation".to_string(),
                ],
                exit_criteria: "Pad layer with a slow filter sweep lane.".to_string(),
            },
            PlaybookStep {
                name: "Space the mix".to_string(),
                purpose: "Reverb sends are the instrument.".to_string(),
                tools: vec![
                    "set_track_fx".to_string(),
                    "apply_genre_mix_preset".to_string(),
                ],
                exit_criteria: "Pads have substantial reverb sends.".to_string(),
            },
        ],
        mix_master: "High-pass everything except the drone; keep master gain low ".to_string()
            + "(−14 to −16 LUFS is fine for ambient).",
        common_mistakes: vec![
            "Too many notes — ambient is sparse".to_string(),
            "Dry pads — reverb is the instrument".to_string(),
        ],
    }
}

/// The techno/trance playbook.
fn techno_playbook(genre: &str) -> ProductionPlaybook {
    ProductionPlaybook {
        genre: genre.to_string(),
        strategy: "Techno is a continuous driving baseline: 4/4 kick, 16th hats, ".to_string()
            + "root-fifth bass, arp lead, with elements swapping every 16/32 "
            + "bars and a strip-down valley in the middle.",
        arrangement_shape: "intro 16 · build 8 · drop 32 · breakdown 8 · outro 8".to_string(),
        octave_map: vec![
            "Sub/kick: octaves 1–2, mono".to_string(),
            "Bass: octaves 1–2, sidechained".to_string(),
            "Arp/leads: octaves 5–7".to_string(),
            "FX/risers: octaves 6–8, wide".to_string(),
        ],
        steps: vec![
            PlaybookStep {
                name: "Set the baseline".to_string(),
                purpose: "The 16th-note drive is the techno constant.".to_string(),
                tools: vec![
                    "write_pattern_steps".to_string(),
                    "write_pattern_notes".to_string(),
                ],
                exit_criteria: "4/4 kick + 16th hats + driving bass.".to_string(),
            },
            PlaybookStep {
                name: "Arrange the valley".to_string(),
                purpose: "Long drop, then a mid-track breakdown for the strip-down.".to_string(),
                tools: vec![
                    "apply_arrangement_template".to_string(),
                    "extend_arrangement".to_string(),
                ],
                exit_criteria: "A 32-bar drop and an 8-bar breakdown.".to_string(),
            },
            PlaybookStep {
                name: "Sidechain + automate".to_string(),
                purpose: "Pump the bass and open the filter through the build.".to_string(),
                tools: vec!["set_track_fx".to_string(), "write_automation".to_string()],
                exit_criteria: "Sidechain on bass; cutoff sweep lane in the build.".to_string(),
            },
        ],
        mix_master: "Sidechain bass to kick; keep the low-mid clean; master to −8/−10 LUFS."
            .to_string(),
        common_mistakes: vec![
            "Breaking the 4/4 drive in the first half".to_string(),
            "Skipping the mid-track breakdown".to_string(),
        ],
    }
}

/// The jazz/swing playbook: ride cymbal swing, walking bass, horn/keys line.
fn jazz_playbook(genre: &str) -> ProductionPlaybook {
    ProductionPlaybook {
        genre: genre.to_string(),
        strategy: "Jazz is a swung, live feel: ride cymbal eighths, backbeat snare, ".to_string()
            + "walking bass quarters, and a horn/keys melodic line. Keep the "
            + "swing on the ride and the walking bass steady; let the harmony breathe.",
        arrangement_shape: "head 8 · solo 8 · head 8 · outro 8".to_string(),
        octave_map: vec![
            "Kick: 20–60 Hz, sparse".to_string(),
            "Bass (upright feel): octaves 1–2, walking quarters".to_string(),
            "Keys/piano: octaves 3–5".to_string(),
            "Horns/lead: octaves 4–6, mid-forward".to_string(),
            "Ride/cymbals: octaves 6–8, swung".to_string(),
        ],
        steps: vec![
            PlaybookStep {
                name: "Set the swing".to_string(),
                purpose: "Establish the ride-cymbal swing and the 2&4 backbeat.".to_string(),
                tools: vec![
                    "create_pattern".to_string(),
                    "write_pattern_steps".to_string(),
                ],
                exit_criteria: "Swing ride eighths and a backbeat snare.".to_string(),
            },
            PlaybookStep {
                name: "Write the walking bass".to_string(),
                purpose: "Root-third-fifth lines that anchor the harmony.".to_string(),
                tools: vec![
                    "write_pattern_notes".to_string(),
                    "set_layer_register".to_string(),
                ],
                exit_criteria: "Walking quarters in octaves 1–2.".to_string(),
            },
            PlaybookStep {
                name: "Arrange head / solo / head".to_string(),
                purpose: "Lay the theme, open a solo section, restate the head.".to_string(),
                tools: vec![
                    "apply_arrangement_template".to_string(),
                    "extend_arrangement".to_string(),
                ],
                exit_criteria: "A recognizable head with a solo space.".to_string(),
            },
        ],
        mix_master: "Keep the ride bright but not harsh; the walking bass warm and ".to_string()
            + "present; master to −10/−12 LUFS for a live-room feel.",
        common_mistakes: vec![
            "Losing the swing (straight 16ths instead of swung eighths)".to_string(),
            "Burying the walking bass under the piano".to_string(),
        ],
    }
}

/// The hip-hop/trap playbook: boom-bap grid, sub-heavy bass, stab samples.
fn hiphop_playbook(genre: &str) -> ProductionPlaybook {
    ProductionPlaybook {
        genre: genre.to_string(),
        strategy: "Hip-hop is a groove bed: sparse boom-bap kick/snare, sub-heavy root "
            .to_string()
            + "bass, and sample-style stabs. The bass and kick share the low end — "
            + "tune them to the same root so the 808 sounds like one instrument.",
        arrangement_shape: "intro 8 · hook 8 · verse 8 · hook 8 · outro 8".to_string(),
        octave_map: vec![
            "Kick: 45–60 Hz, tight".to_string(),
            "808 bass: octaves 1–2, sub-forward".to_string(),
            "Keys/samples: octaves 3–5".to_string(),
            "Lead/stabs: octaves 4–6".to_string(),
            "Hats: octaves 7–8, swung".to_string(),
        ],
        steps: vec![
            PlaybookStep {
                name: "Lay the boom-bap".to_string(),
                purpose: "Sparse kick/snare grid with swung ghost hats.".to_string(),
                tools: vec![
                    "create_pattern".to_string(),
                    "write_pattern_steps".to_string(),
                ],
                exit_criteria: "Kick on 1, snare on 2&4, swung hats.".to_string(),
            },
            PlaybookStep {
                name: "Tune the 808".to_string(),
                purpose: "Sub bass rooted to the kick so the low end locks.".to_string(),
                tools: vec![
                    "write_pattern_notes".to_string(),
                    "set_layer_register".to_string(),
                ],
                exit_criteria: "Root-octave sub in octaves 1–2.".to_string(),
            },
            PlaybookStep {
                name: "Drop the stabs".to_string(),
                purpose: "Sample-style melodic stabs over the groove.".to_string(),
                tools: vec![
                    "apply_motivic_variation".to_string(),
                    "place_pattern_in_segment".to_string(),
                ],
                exit_criteria: "A memorable 4–8 note hook.".to_string(),
            },
        ],
        mix_master: "Sidechain nothing in trap; let the 808 ring. Keep hats bright, ".to_string()
            + "bass huge; master to −8/−10 LUFS.",
        common_mistakes: vec![
            "Clashing 808 and kick frequencies (tune them together)".to_string(),
            "Too-dense drums killing the swing".to_string(),
        ],
    }
}

/// The pop playbook: 4/4 kick + claps, catchy hook, singable lead.
fn pop_playbook(genre: &str) -> ProductionPlaybook {
    ProductionPlaybook {
        genre: genre.to_string(),
        strategy: "Pop is a song-form hook machine: 4/4 kick, claps on 2&4, a ".to_string()
            + "root-fifth bass with octave pickups, bright chords, and a singable "
            + "vocal-range lead. The chorus must hit harder than the verse — "
            + "thicker layers and a lifted octave.",
        arrangement_shape: "intro 8 · verse 8 · build 8 · chorus 24 · outro 8".to_string(),
        octave_map: vec![
            "Kick: 45–60 Hz".to_string(),
            "Bass: octaves 1–2, pop groove".to_string(),
            "Chords: octaves 3–5".to_string(),
            "Lead/vocal: octaves 4–6, front and center".to_string(),
            "Hats: octaves 7–8".to_string(),
        ],
        steps: vec![
            PlaybookStep {
                name: "Write the hook first".to_string(),
                purpose: "A singable 4-bar vocal-range melody is the song.".to_string(),
                tools: vec![
                    "write_pattern_notes".to_string(),
                    "set_layer_register".to_string(),
                ],
                exit_criteria: "A memorable lead in octaves 4–6.".to_string(),
            },
            PlaybookStep {
                name: "Build the groove bed".to_string(),
                purpose: "Kick/clap grid plus the root-fifth pop bass.".to_string(),
                tools: vec![
                    "create_pattern".to_string(),
                    "write_pattern_steps".to_string(),
                ],
                exit_criteria: "4/4 drive with claps on 2&4.".to_string(),
            },
            PlaybookStep {
                name: "Arrange verse → chorus lift".to_string(),
                purpose: "Chorus doubles the lead an octave and stacks the pads.".to_string(),
                tools: vec![
                    "apply_arrangement_template".to_string(),
                    "repeat_pattern_in_segment".to_string(),
                ],
                exit_criteria: "A clearly bigger chorus than verse.".to_string(),
            },
        ],
        mix_master: "Center the vocal-range lead, sidechain bass to kick, master to ".to_string()
            + "−7/−9 LUFS for streaming loudness.",
        common_mistakes: vec![
            "Chorus not louder/denser than the verse".to_string(),
            "Burying the lead under the chords".to_string(),
        ],
    }
}

/// The funk playbook: syncopated 16ths, popping bass, horn stabs.
fn funk_playbook(genre: &str) -> ProductionPlaybook {
    ProductionPlaybook {
        genre: genre.to_string(),
        strategy: "Funk lives in the pocket: syncopated 16th drums, a popping bass ".to_string()
            + "with ghost notes, and tight horn stabs on the offbeats. The bass is "
            + "the lead instrument — give it presence, not just lows.",
        arrangement_shape: "intro 8 · groove 8 · hook 16 · breakdown 8 · outro 8".to_string(),
        octave_map: vec![
            "Kick: 45–60 Hz, syncopated".to_string(),
            "Bass: octaves 1–2, popping 16ths".to_string(),
            "Keys: octaves 3–5".to_string(),
            "Horns: octaves 4–6, tight stabs".to_string(),
            "Hats: octaves 7–8, ghost notes".to_string(),
        ],
        steps: vec![
            PlaybookStep {
                name: "Lock the pocket".to_string(),
                purpose: "Syncopated kick/snare grid with ghost hats.".to_string(),
                tools: vec![
                    "create_pattern".to_string(),
                    "write_pattern_steps".to_string(),
                ],
                exit_criteria: "16th feel with offbeat accents.".to_string(),
            },
            PlaybookStep {
                name: "Pop the bass".to_string(),
                purpose: "Syncopated 16th bass with octave pops.".to_string(),
                tools: vec![
                    "write_pattern_notes".to_string(),
                    "set_layer_register".to_string(),
                ],
                exit_criteria: "Bass carries the groove in octaves 1–2.".to_string(),
            },
            PlaybookStep {
                name: "Add horn stabs".to_string(),
                purpose: "Tight offbeat stabs answer the bass.".to_string(),
                tools: vec![
                    "apply_motivic_variation".to_string(),
                    "place_pattern_in_segment".to_string(),
                ],
                exit_criteria: "A recognizable stab riff.".to_string(),
            },
        ],
        mix_master: "Give the bass a 200 Hz presence boost, keep horns tight, master ".to_string()
            + "to −8/−10 LUFS.",
        common_mistakes: vec![
            "Straight 16ths instead of the syncopated pocket".to_string(),
            "Bass lost in the low end (no presence)".to_string(),
        ],
    }
}

/// The blues playbook: shuffle feel, walking bass, wailing riff.
fn blues_playbook(genre: &str) -> ProductionPlaybook {
    ProductionPlaybook {
        genre: genre.to_string(),
        strategy: "Blues is a shuffle: triplet-feel hats, backbeat snare, a walking ".to_string()
            + "bass, and a wailing guitar-style lead. Keep the call-and-response "
            + "riff and let the lead bend the notes.",
        arrangement_shape: "head 8 · solo 8 · head 8 · outro 8".to_string(),
        octave_map: vec![
            "Kick: 45–60 Hz".to_string(),
            "Bass: octaves 1–2, shuffle walk".to_string(),
            "Keys: octaves 3–5".to_string(),
            "Guitar/lead: octaves 4–6, mid-forward".to_string(),
            "Hats: octaves 7–8, shuffle".to_string(),
        ],
        steps: vec![
            PlaybookStep {
                name: "Set the shuffle".to_string(),
                purpose: "Triplet-feel hats with the 2&4 backbeat.".to_string(),
                tools: vec![
                    "create_pattern".to_string(),
                    "write_pattern_steps".to_string(),
                ],
                exit_criteria: "A genuine shuffle, not straight eighths.".to_string(),
            },
            PlaybookStep {
                name: "Walk the bass".to_string(),
                purpose: "Shuffle-walking root-third-fifth line.".to_string(),
                tools: vec![
                    "write_pattern_notes".to_string(),
                    "set_layer_register".to_string(),
                ],
                exit_criteria: "Walking line in octaves 1–2.".to_string(),
            },
            PlaybookStep {
                name: "Write the riff".to_string(),
                purpose: "The wailing lead line is the song's voice.".to_string(),
                tools: vec![
                    "apply_motivic_variation".to_string(),
                    "place_pattern_in_segment".to_string(),
                ],
                exit_criteria: "A memorable call-and-response riff.".to_string(),
            },
        ],
        mix_master: "Warm mid-forward mix, gentle top end, master to −10/−12 LUFS.".to_string(),
        common_mistakes: vec![
            "Straight feel instead of the shuffle".to_string(),
            "The riff too quiet against the shuffle drums".to_string(),
        ],
    }
}

/// The synthwave playbook: pulsing 8th bass, 16th arp, dark retro pads.
fn synthwave_playbook(genre: &str) -> ProductionPlaybook {
    ProductionPlaybook {
        genre: genre.to_string(),
        strategy: "Synthwave is a driving retro machine: pulsing 8th-note bass, 16th ".to_string()
            + "arpeggios, dark pads, and a 4/4 kick with fast hats. The bass pulse "
            + "and the arp are the genre's signature — keep them locked.",
        arrangement_shape: "intro 16 · build 8 · drop 32 · breakdown 8 · outro 8".to_string(),
        octave_map: vec![
            "Kick: 45–60 Hz".to_string(),
            "Bass: octaves 1–2, pulsing 8ths".to_string(),
            "Pads: octaves 3–5, dark retro chords".to_string(),
            "Arp/lead: octaves 5–7, 16ths".to_string(),
            "Hats: octaves 7–8, driving 16ths".to_string(),
        ],
        steps: vec![
            PlaybookStep {
                name: "Set the pulse".to_string(),
                purpose: "4/4 kick with 16th hats and a clap.".to_string(),
                tools: vec![
                    "create_pattern".to_string(),
                    "write_pattern_steps".to_string(),
                ],
                exit_criteria: "Relentless 16th drive.".to_string(),
            },
            PlaybookStep {
                name: "Pulse the bass".to_string(),
                purpose: "Root-octave 8th-note pulse in octaves 1–2.".to_string(),
                tools: vec![
                    "write_pattern_notes".to_string(),
                    "set_layer_register".to_string(),
                ],
                exit_criteria: "A locked 8th-note bass pulse.".to_string(),
            },
            PlaybookStep {
                name: "Run the arp".to_string(),
                purpose: "16th arpeggio over dark pads.".to_string(),
                tools: vec![
                    "apply_motivic_variation".to_string(),
                    "place_pattern_in_segment".to_string(),
                ],
                exit_criteria: "A memorable 16th arp line.".to_string(),
            },
        ],
        mix_master: "Wide pads, tight kick/bass, master to −8/−10 LUFS with the ".to_string()
            + "retro high-end kept clear.",
        common_mistakes: vec![
            "Bass pulse losing sync with the kick".to_string(),
            "Pads too dark, swallowing the arp".to_string(),
        ],
    }
}

/// The drum-and-bass playbook: breakbeat, rolling bass, airy stabs.
fn dnb_playbook(genre: &str) -> ProductionPlaybook {
    ProductionPlaybook {
        genre: genre.to_string(),
        strategy: "Drum and bass is a fast breakbeat over a rolling bass: syncopated ".to_string()
            + "kick/snare, 16th hats, a rolling root-fifth bass, and airy stabs. "
            + "At 170+ BPM the drums are the engine — keep the break tight.",
        arrangement_shape: "intro 16 · build 8 · drop 32 · breakdown 8 · outro 8".to_string(),
        octave_map: vec![
            "Kick: 45–60 Hz, syncopated".to_string(),
            "Bass: octaves 1–2, rolling eighths".to_string(),
            "Pads: octaves 3–5".to_string(),
            "Stabs/lead: octaves 4–6, airy".to_string(),
            "Hats: octaves 7–8, 16ths".to_string(),
        ],
        steps: vec![
            PlaybookStep {
                name: "Build the break".to_string(),
                purpose: "Syncopated breakbeat with a 16th hat stream.".to_string(),
                tools: vec![
                    "create_pattern".to_string(),
                    "write_pattern_steps".to_string(),
                ],
                exit_criteria: "A tight, fast break.".to_string(),
            },
            PlaybookStep {
                name: "Roll the bass".to_string(),
                purpose: "Root-fifth eighths that drive under the break.".to_string(),
                tools: vec![
                    "write_pattern_notes".to_string(),
                    "set_layer_register".to_string(),
                ],
                exit_criteria: "A rolling bass in octaves 1–2.".to_string(),
            },
            PlaybookStep {
                name: "Float the stabs".to_string(),
                purpose: "Airy stabs over the break for the drop.".to_string(),
                tools: vec![
                    "apply_motivic_variation".to_string(),
                    "place_pattern_in_segment".to_string(),
                ],
                exit_criteria: "A distinctive stab motif.".to_string(),
            },
        ],
        mix_master: "Keep the sub tight, the break loud, master to −7/−9 LUFS.".to_string(),
        common_mistakes: vec![
            "The break losing energy at 170 BPM (too sparse)".to_string(),
            "Rolling bass clashing with the kick".to_string(),
        ],
    }
}

/// The cinematic/orchestral playbook: sparse percussion, big pads, strings.
fn cinematic_playbook(genre: &str) -> ProductionPlaybook {
    ProductionPlaybook {
        genre: genre.to_string(),
        strategy: "Cinematic music is a slow-build epic: sparse timpani, a low drone ".to_string()
            + "bass, long strings/choir lines, and big orchestral chord swells. "
            + "Dynamics do the work — save the biggest swell for the peak.",
        arrangement_shape: "intro 16 · build 8 · peak 24 · breakdown 8 · outro 16".to_string(),
        octave_map: vec![
            "Timpani: 40–80 Hz, sparse".to_string(),
            "Bass: octaves 1–2, drone".to_string(),
            "Strings/choir: octaves 3–5".to_string(),
            "Lead melody: octaves 4–6, long notes".to_string(),
            "Cymbals/swells: octaves 7–8, rare".to_string(),
        ],
        steps: vec![
            PlaybookStep {
                name: "Paint the bed".to_string(),
                purpose: "Low drone bass plus a big orchestral chord pad.".to_string(),
                tools: vec![
                    "create_pattern".to_string(),
                    "write_pattern_notes".to_string(),
                ],
                exit_criteria: "A sustained, evolving pad bed.".to_string(),
            },
            PlaybookStep {
                name: "Write the theme".to_string(),
                purpose: "Long strings/choir melody lines.".to_string(),
                tools: vec![
                    "apply_motivic_variation".to_string(),
                    "place_pattern_in_segment".to_string(),
                ],
                exit_criteria: "A memorable slow theme.".to_string(),
            },
            PlaybookStep {
                name: "Shape the dynamics".to_string(),
                purpose: "Automate the swell up to the peak section.".to_string(),
                tools: vec![
                    "write_automation".to_string(),
                    "apply_arrangement_template".to_string(),
                ],
                exit_criteria: "A clear build to a bigger peak.".to_string(),
            },
        ],
        mix_master: "Wide pads, warm lows, plenty of headroom; master to −12/−14 LUFS ".to_string()
            + "for the dynamics.",
        common_mistakes: vec![
            "Constant loudness killing the epic dynamics".to_string(),
            "Too many busy parts (cinematic needs space)".to_string(),
        ],
    }
}

/// The general fallback playbook for any other genre.
fn general_playbook(genre: &str) -> ProductionPlaybook {
    ProductionPlaybook {
        genre: genre.to_string(),
        strategy: "General production workflow: compose the foundational loop, ".to_string()
            + "arrange it across the timeline, assign octave registers per "
            + "instrument, automate for movement, mix against a genre "
            + "reference, and master with a limiter ceiling.",
        arrangement_shape: "intro 16 · build 8 · drop 16 · breakdown 8 · outro 8".to_string(),
        octave_map: vec![
            "Sub/kick: octaves 1–2, mono".to_string(),
            "Bass: octaves 1–2".to_string(),
            "Chords/pads: octaves 3–5".to_string(),
            "Leads: octaves 5–7".to_string(),
            "Hats/air: octaves 6–8, wide".to_string(),
        ],
        steps: vec![
            PlaybookStep {
                name: "Configure from the brief".to_string(),
                purpose: "Extract genre, BPM, key, duration.".to_string(),
                tools: vec![
                    "generate_config_from_brief".to_string(),
                    "validate_config".to_string(),
                ],
                exit_criteria: "Validated config with the detected genre/tempo/key.".to_string(),
            },
            PlaybookStep {
                name: "Compose foundational patterns".to_string(),
                purpose: "The 1-bar atoms: drums, bass, harmony, lead.".to_string(),
                tools: vec![
                    "create_pattern".to_string(),
                    "write_pattern_notes".to_string(),
                    "write_pattern_steps".to_string(),
                ],
                exit_criteria: "In-key patterns for the whole band.".to_string(),
            },
            PlaybookStep {
                name: "Arrange and extend".to_string(),
                purpose: "Apply the genre template, then extend to the target length.".to_string(),
                tools: vec![
                    "apply_arrangement_template".to_string(),
                    "extend_arrangement".to_string(),
                ],
                exit_criteria: "Segments sum to the target bar count.".to_string(),
            },
            PlaybookStep {
                name: "Discipline registers".to_string(),
                purpose: "Assign octave bands so layers don't fight.".to_string(),
                tools: vec![
                    "set_layer_register".to_string(),
                    "run_validation".to_string(),
                ],
                exit_criteria: "No register violations.".to_string(),
            },
            PlaybookStep {
                name: "Automate and repeat".to_string(),
                purpose: "Movement via CC lanes and pattern repetition/transposition.".to_string(),
                tools: vec![
                    "write_automation".to_string(),
                    "repeat_pattern_in_segment".to_string(),
                ],
                exit_criteria: "CC lanes visible in the timeline.".to_string(),
            },
            PlaybookStep {
                name: "Mix and master".to_string(),
                purpose: "FX chains, genre preset, master bus.".to_string(),
                tools: vec![
                    "set_track_fx".to_string(),
                    "apply_genre_mix_preset".to_string(),
                    "ss mix master".to_string(),
                ],
                exit_criteria: "Preset applied; master ceiling set.".to_string(),
            },
            PlaybookStep {
                name: "Render and validate".to_string(),
                purpose: "Prove the output against the gates.".to_string(),
                tools: vec![
                    "render_project".to_string(),
                    "get_audit_summary".to_string(),
                ],
                exit_criteria: "Render passes the spectral/golden gates.".to_string(),
            },
        ],
        mix_master: "Scoop 250–500 Hz on non-bass elements; master to a −1 dBTP ceiling."
            .to_string(),
        common_mistakes: vec![
            "Overlapping octave registers between layers".to_string(),
            "Skipping validation before render".to_string(),
        ],
    }
}

/// The per-genre arrangement template (G6).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ArrangementTemplate {
    /// Genre this template targets.
    pub genre: String,
    /// Section archetypes in order.
    pub sections: Vec<SectionTemplate>,
}

/// The arrangement template for a genre, or the default when unknown.
///
/// Mirrors the §1.2 reference-track research: intro 16–32, build 8–16, drop
/// 16–32, breakdown 8–16, outro 8–16 — with the energy arc rising through the
/// build, peaking at the drop, and settling on the outro.
#[must_use]
pub fn arrangement_template_for(genre: &str) -> ArrangementTemplate {
    let g = genre.to_lowercase();
    let (intro, build, drop, breakdown, outro) = if g.contains("ambient")
        || g.contains("lo-fi")
        || g.contains("lofi")
        || g.contains("classical")
    {
        // Slow-burn genres: long intro, gentle build, sustained middle.
        (32u32, 8u32, 16u32, 16u32, 16u32)
    } else if g.contains("jazz") || g.contains("swing") || g.contains("blues") {
        // Live-band forms: head / solo / head with a compact intro/outro.
        (8u32, 8u32, 16u32, 16u32, 8u32)
    } else if g.contains("hip")
        || g.contains("trap")
        || g.contains("rap")
        || g.contains("funk")
        || g.contains("reggae")
    {
        // Groove-driven: short intro, hook chorus, breakdown for the beat.
        (8u32, 8u32, 16u32, 8u32, 8u32)
    } else if g.contains("house") {
        (16u32, 8u32, 16u32, 8u32, 8u32)
    } else if g.contains("rock")
        || g.contains("metal")
        || g.contains("nu_metal")
        || g.contains("punk")
        || g.contains("country")
    {
        // Rock/nu-metal: shorter intro, big chorus (the peak), and a
        // breakdown for the quiet middle-eight — the Linkin Park shape.
        (8u32, 8u32, 32u32, 8u32, 8u32)
    } else if g.contains("pop") || g.contains("cinematic") || g.contains("folk") {
        // Song forms: verse/build/chorus with a song-length peak.
        (8u32, 8u32, 24u32, 8u32, 8u32)
    } else if g.contains("techno")
        || g.contains("trance")
        || g.contains("dubstep")
        || g.contains("drum")
        || g.contains("dnb")
        || g.contains("synthwave")
        || g.contains("electronic")
    {
        (16u32, 8u32, 32u32, 8u32, 8u32)
    } else {
        (16u32, 8u32, 16u32, 8u32, 8u32)
    };
    let mk = |role: &str, bars: u32, energy: f32, layers: Vec<&str>| SectionTemplate {
        role: role.to_string(),
        bars,
        energy,
        layers: layers.into_iter().map(str::to_string).collect(),
    };
    ArrangementTemplate {
        genre: genre.to_string(),
        sections: vec![
            mk("intro", intro, 0.35, vec!["Drums", "Bass"]),
            mk("build", build, 0.6, vec!["Drums", "Bass", "Lead"]),
            mk("drop", drop, 0.95, vec!["Drums", "Bass", "Lead", "Pad"]),
            mk("breakdown", breakdown, 0.4, vec!["Pad", "Lead"]),
            mk("outro", outro, 0.25, vec!["Drums"]),
        ],
    }
}

/// The `arrangement_templates` resource (G6) — every genre's section
/// archetypes (intro/build/drop/breakdown/outro) with production bar counts,
/// the energy arc, and the layers active in each section, from the §1.2
/// reference-track research. Inspectable by agents without mutating anything
/// ("tools mutate, resources inspect"), so a producer can see the available
/// arrangement shapes before choosing one.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ArrangementTemplates {
    /// Genres that have a named archetype table (unknown genres fall back to
    /// the default table).
    pub genres: Vec<String>,
    /// One template per named genre, in the same order.
    pub templates: Vec<ArrangementTemplate>,
    /// The fallback template used for any genre without a named table.
    pub default: ArrangementTemplate,
}

/// Build the `arrangement_templates` resource: the named-genre tables plus
/// the default fallback.
#[must_use]
pub fn arrangement_templates_resource() -> ArrangementTemplates {
    let genres = [
        "techno",
        "house",
        "ambient",
        "trance",
        "dubstep",
        "rock",
        "jazz",
        "hip-hop",
        "pop",
        "funk",
        "blues",
        "synthwave",
        "drum_and_bass",
        "lo-fi",
        "cinematic",
        "classical",
        "reggae",
        "country",
        "folk",
        "electronic",
    ];
    let templates: Vec<ArrangementTemplate> =
        genres.iter().map(|g| arrangement_template_for(g)).collect();
    ArrangementTemplates {
        genres: genres
            .iter()
            .map(std::string::ToString::to_string)
            .collect(),
        templates,
        default: arrangement_template_for("default"),
    }
}

/// Build the layered timeline view from a real project.
///
/// Pure derivation (no storage): sections come from the arrangement, lanes
/// from the compiled timeline's note-on events grouped per segment+layer,
/// with the occupied octave register computed from the notes themselves.
#[must_use]
pub fn build_timeline_view(project: &scorestrata_core::domain::Project) -> TimelineView {
    use scorestrata_compiler::timeline::EventKind;

    const MAX_NOTES_PER_LAYER: usize = 24;
    let ppq = u64::from(scorestrata_core::domain::PPQ);
    let ts_num = u64::from(project.arrangement.time_signature.numerator);
    let ticks_per_bar = ppq * ts_num;

    let mut sections = Vec::new();
    for segment in &project.arrangement.segments {
        let energy = project
            .arrangement
            .energy_curve
            .iter()
            .find(|e| e.segment_id == segment.id)
            .map(|e| e.target_energy);
        sections.push(TimelineSection {
            name: segment.name.clone(),
            role: format!("{:?}", segment.role).to_lowercase(),
            start_bar: segment.start_bar,
            bars: segment.bars,
            energy,
            layers: segment.layers.iter().map(|l| l.name.clone()).collect(),
        });
    }

    // Group note-on events per segment+layer, pairing each with its note-off
    // (same layer + pitch) to report real durations.
    let mut lanes: Vec<TimelineLayer> = Vec::new();
    if let Ok(timeline) = scorestrata_compiler::compile_project(project) {
        for segment in &project.arrangement.segments {
            for layer in &segment.layers {
                let events = timeline.events_for_layer(layer.id);
                let mut notes: Vec<TimelineNote> = Vec::new();
                for (i, e) in events.iter().enumerate() {
                    let EventKind::NoteOn { pitch, velocity } = &e.kind else {
                        continue;
                    };
                    // Next event with the same pitch on this layer (the note-off).
                    let duration_ticks = events[i + 1..]
                        .iter()
                        .find(|next| matches!(&next.kind, EventKind::NoteOff { pitch: p } if p == pitch))
                        .map_or(0, |off| off.tick.0.saturating_sub(e.tick.0));
                    notes.push(TimelineNote {
                        start_tick: e.tick.0,
                        bar: (e.tick.0 / ticks_per_bar) as u32,
                        pitch: pitch.0,
                        name: pitch.name(),
                        duration_ticks,
                        velocity: velocity.0,
                    });
                }
                if notes.is_empty() {
                    continue;
                }
                let (min_octave, max_octave) =
                    notes.iter().fold((i8::MAX, i8::MIN), |(lo, hi), n| {
                        let oct = n.pitch as i8 / 12 - 1;
                        (lo.min(oct), hi.max(oct))
                    });
                let note_count = notes.len() as u32;
                let automation: Vec<AutomationLaneView> = layer
                    .automation
                    .iter()
                    .map(|lane| AutomationLaneView {
                        param: match lane.param {
                            scorestrata_core::domain::AutomationParam::Cutoff => "cutoff",
                            scorestrata_core::domain::AutomationParam::Volume => "volume",
                            scorestrata_core::domain::AutomationParam::Pan => "pan",
                            scorestrata_core::domain::AutomationParam::Send => "send",
                        }
                        .to_string(),
                        controller: lane.param.cc_number(),
                        points: lane.points.iter().map(|p| (p.tick.0, p.value)).collect(),
                    })
                    .collect();
                let fx_summary = layer.fx.as_ref().map(|fx| {
                    let mut parts: Vec<String> = Vec::new();
                    if let Some(hz) = fx.high_pass_hz {
                        parts.push(format!("hpf {hz:.0}Hz"));
                    }
                    if !fx.eq.is_empty() {
                        parts.push(format!("eq{}", fx.eq.len()));
                    }
                    if fx.compressor.is_some() {
                        parts.push("comp".to_string());
                    }
                    if fx.sidechain_from_kick {
                        parts.push("sidechain".to_string());
                    }
                    if fx.sends.reverb > 0.0 {
                        parts.push(format!("rvb{:.1}", fx.sends.reverb));
                    }
                    if fx.sends.delay > 0.0 {
                        parts.push(format!("dly{:.1}", fx.sends.delay));
                    }
                    parts.join(",")
                });
                lanes.push(TimelineLayer {
                    layer_id: layer.id.to_string(),
                    name: layer.name.clone(),
                    instrument: layer.instrument.clone(),
                    segment: segment.name.clone(),
                    min_octave,
                    max_octave,
                    note_count,
                    notes: notes.into_iter().take(MAX_NOTES_PER_LAYER).collect(),
                    automation,
                    fx: fx_summary,
                });
            }
        }
    }

    TimelineView {
        project_id: project.id.to_string(),
        name: project.name.clone(),
        genre: project.config.genre.clone(),
        bpm: project.config.bpm,
        key: project.config.key.clone(),
        total_bars: project.arrangement.total_bars,
        sections,
        layers: lanes,
    }
}

// ── Existing Resource Types ──────────────────────────────────────────

/// A project summary resource — high-level state overview.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProjectSummary {
    /// Project ID.
    pub project_id: String,
    /// Project name.
    pub name: String,
    /// Current project state.
    pub state: String,
    /// Number of segments.
    pub segment_count: u32,
    /// Number of layers.
    pub layer_count: u32,
    /// Total bars.
    pub total_bars: u32,
    /// Whether a validation report exists.
    pub has_validation: bool,
    /// Overall validation score (None if not validated).
    pub validation_score: Option<f32>,
    /// Number of hard failures.
    pub hard_failures: u32,
    /// Number of warnings.
    pub warnings: u32,
    /// Suggested next actions.
    pub next_actions: Vec<String>,
}

/// A validation report summary — condensed view for agent orientation.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ValidationSummaryView {
    /// Project ID.
    pub project_id: String,
    /// Validation level.
    pub level: u8,
    /// Whether all hard gates passed.
    pub all_hard_gates_passed: bool,
    /// Overall score.
    pub overall_score: f32,
    /// Number of diagnostics by severity.
    pub summary: DiagnosticCounts,
    /// Top issues (max 5, sorted by severity).
    pub top_issues: Vec<TopIssue>,
    /// Metric summary.
    pub metrics: Vec<MetricSummary>,
}

/// Diagnostic count summary.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DiagnosticCounts {
    /// Informational diagnostics.
    pub info: u32,
    /// Warning diagnostics.
    pub warnings: u32,
    /// Error diagnostics.
    pub errors: u32,
    /// Critical diagnostics.
    pub critical: u32,
}

/// A condensed top issue for summary views.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TopIssue {
    /// Diagnostic code.
    pub code: String,
    /// Severity.
    pub severity: Severity,
    /// Message.
    pub message: String,
    /// Evidence URIs.
    pub evidence_uris: Vec<String>,
    /// Suggested patch operations.
    pub suggested_patches: Vec<String>,
}

/// A metric summary entry.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MetricSummary {
    /// Metric name.
    pub name: String,
    /// Metric value.
    pub value: f32,
    /// Severity.
    pub severity: Severity,
}

/// Build a project summary from a real [`Project`] entity.
///
/// Returns a high-level overview suitable for agent orientation,
/// including segment/layer counts, current status, and suggested next actions.
#[must_use]
pub fn get_project_summary_from_project(
    project: &scorestrata_core::domain::Project,
) -> ProjectSummary {
    let segment_count: u32 = project.arrangement.segments.len() as u32;
    let layer_count: u32 = project
        .arrangement
        .segments
        .iter()
        .map(|s| s.layers.len() as u32)
        .sum();
    let total_bars = project.arrangement.total_bars;
    let next_actions = suggest_next_actions(project.status);

    ProjectSummary {
        project_id: project.id.to_string(),
        name: project.name.clone(),
        state: format!("{:?}", project.status),
        segment_count,
        layer_count,
        total_bars,
        has_validation: false,
        validation_score: None,
        hard_failures: 0,
        warnings: 0,
        next_actions,
    }
}

/// Build a project summary from a project ID (stub for when project data is unavailable).
#[must_use]
pub fn get_project_summary(project_id: &str) -> ProjectSummary {
    ProjectSummary {
        project_id: project_id.to_string(),
        name: "untitled".to_string(),
        state: "BriefReceived".to_string(),
        segment_count: 0,
        layer_count: 0,
        total_bars: 0,
        has_validation: false,
        validation_score: None,
        hard_failures: 0,
        warnings: 0,
        next_actions: vec![
            "create_config_from_brief".to_string(),
            "plan_arrangement".to_string(),
        ],
    }
}

/// Suggest next actions based on the current project status.
fn suggest_next_actions(status: scorestrata_core::domain::ProjectStatus) -> Vec<String> {
    use scorestrata_core::domain::ProjectStatus;
    match status {
        ProjectStatus::BriefReceived => vec!["generate_config_from_brief".to_string()],
        ProjectStatus::ConfigDrafted => {
            vec!["validate_config".to_string(), "approve_config".to_string()]
        }
        ProjectStatus::ConfigValidationFailed => vec![
            "fix_config_issues".to_string(),
            "validate_config".to_string(),
        ],
        ProjectStatus::ConfigApproved => vec!["plan_arrangement".to_string()],
        ProjectStatus::ArrangementPlanned => vec!["create_skeleton".to_string()],
        ProjectStatus::SkeletonCreated => {
            vec!["draft_segments".to_string(), "create_patterns".to_string()]
        }
        ProjectStatus::SegmentDrafting => vec![
            "generate_segment_candidates".to_string(),
            "create_patterns".to_string(),
        ],
        ProjectStatus::SegmentCandidatesGenerated => vec![
            "evaluate_segment_candidates".to_string(),
            "select_best_candidate".to_string(),
        ],
        ProjectStatus::SegmentValidationFailed => vec![
            "fix_segment_issues".to_string(),
            "draft_segment".to_string(),
        ],
        ProjectStatus::SegmentsValidated => {
            vec!["validate_transitions".to_string(), "compile".to_string()]
        }
        ProjectStatus::TransitionValidationFailed => vec![
            "fix_transition_issues".to_string(),
            "add_transition_package".to_string(),
        ],
        ProjectStatus::TransitionValidated => vec!["draft_mix".to_string(), "compile".to_string()],
        ProjectStatus::MixDrafting => vec![
            "validate_mix".to_string(),
            "apply_mix_processing".to_string(),
        ],
        ProjectStatus::MixValidationFailed => {
            vec!["fix_mix_issues".to_string(), "draft_mix".to_string()]
        }
        ProjectStatus::Compiling => vec!["run_validation".to_string()],
        ProjectStatus::Validating => vec![
            "run_validation".to_string(),
            "get_audit_summary".to_string(),
        ],
        ProjectStatus::Experimenting => vec![
            "evaluate_candidates".to_string(),
            "select_best_candidate".to_string(),
        ],
        ProjectStatus::Calibrating => vec![
            "run_final_validation".to_string(),
            "render_preview".to_string(),
        ],
        ProjectStatus::Rendering => vec!["get_render_job".to_string()],
        ProjectStatus::RenderFailed => vec![
            "diagnose_render_failure".to_string(),
            "retry_render".to_string(),
        ],
        ProjectStatus::FinalValidationPassed => {
            vec!["finalize_render".to_string(), "export_midi".to_string()]
        }
        ProjectStatus::RenderFinalized => vec!["export_midi".to_string(), "archive".to_string()],
        ProjectStatus::AwaitingHumanDecision => vec!["await_human_decision".to_string()],
        ProjectStatus::NeedsPolicyReview => vec![
            "review_policy_violations".to_string(),
            "adjust_policy".to_string(),
        ],
        ProjectStatus::NeedsHumanReview => vec![
            "create_human_review_packet".to_string(),
            "await_human_decision".to_string(),
        ],
        ProjectStatus::Replaying => vec![
            "continue_replay".to_string(),
            "diagnose_replay_failure".to_string(),
        ],
        ProjectStatus::Quarantined => vec![
            "diagnose_quarantine".to_string(),
            "escalate_to_human".to_string(),
        ],
        ProjectStatus::Archived => vec![],
    }
}

/// Build a `ValidationSummaryView` from a real `ValidationReport`.
///
/// This enables agent drill-down into per-metric scoring and top issues,
/// returning a condensed view suitable for MCP resource responses.
#[must_use]
pub fn build_validation_summary_view(
    report: &scorestrata_core::diagnostic::ValidationReport,
) -> ValidationSummaryView {
    let top_issues: Vec<TopIssue> = report
        .diagnostics
        .iter()
        .filter(|d| d.severity >= Severity::Warning)
        .take(5)
        .map(|d| TopIssue {
            code: d.code.clone(),
            severity: d.severity,
            message: d.message.clone(),
            evidence_uris: d.evidence_uris.clone(),
            suggested_patches: d.suggested_patches.iter().map(|p| p.op.clone()).collect(),
        })
        .collect();

    let metrics: Vec<MetricSummary> = report
        .scorecard
        .metrics
        .iter()
        .map(|m| MetricSummary {
            name: m.name.clone(),
            value: m.value,
            severity: m.severity,
        })
        .collect();

    ValidationSummaryView {
        project_id: report.project_id.to_string(),
        level: report.level,
        all_hard_gates_passed: report.all_hard_gates_passed,
        overall_score: report.scorecard.overall_score,
        summary: DiagnosticCounts {
            info: report.summary.info,
            warnings: report.summary.warnings,
            errors: report.summary.errors,
            critical: report.summary.critical,
        },
        top_issues,
        metrics,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    #[expect(clippy::unwrap_used, reason = "Test assertions")]
    fn project_summary_serializes() {
        let summary = get_project_summary("proj_123");
        let json = serde_json::to_string_pretty(&summary).unwrap();
        assert!(json.contains("proj_123"));
        assert!(json.contains("BriefReceived"));
    }

    #[test]
    fn project_summary_has_next_actions() {
        let summary = get_project_summary("proj_123");
        assert!(!summary.next_actions.is_empty());
    }

    #[test]
    fn new_resource_types_serialize() {
        let policy = PolicyProfileView {
            project_id: "p1".to_string(),
            allow_destructive_commands: false,
            require_snapshot_before_patch: true,
            max_out_of_scale_ratio: 0.05,
            min_chorus_energy_delta: 0.15,
            max_true_peak_dbtp: -1.0,
            max_candidate_renders_per_segment: 8,
        };
        #[expect(clippy::unwrap_used, reason = "Test assertions")]
        let json = serde_json::to_string_pretty(&policy).unwrap();
        assert!(json.contains("max_out_of_scale_ratio"));

        let provenance = ProvenanceView {
            project_id: "p1".to_string(),
            compiler_version: "0.1.0".to_string(),
            validator_version: "0.1.0".to_string(),
            audio_engine_version: "0.1.0".to_string(),
            dsp_graph_version: "v3".to_string(),
            seed: Some(1337),
            asset_hashes: BTreeMap::new(),
            render_settings: BTreeMap::new(),
            policy_hash: "abc".to_string(),
            brief_hash: "brief".to_string(),
            config_hash: "config".to_string(),
            snapshot_hash: "snap".to_string(),
            provenance_hash: "h".to_string(),
            complete: true,
        };
        #[expect(clippy::unwrap_used, reason = "Test assertions")]
        let json = serde_json::to_string_pretty(&provenance).unwrap();
        assert!(json.contains("compiler_version"));

        // ProvenanceView derives from a real manifest and carries its hash.
        let mut manifest = scorestrata_storage::provenance::ProvenanceManifest::new("p1");
        manifest.compiler_version = "0.1.0".to_string();
        let view = ProvenanceView::from_manifest(&manifest);
        assert_eq!(view.project_id, "p1");
        assert_eq!(view.provenance_hash, manifest.provenance_hash());
        assert!(!view.complete);

        // RenderManifestView derives from a real render manifest (§17.2).
        let mut render = scorestrata_storage::render_manifest::RenderManifest::new("art_p1");
        render.artifact_type = "master_wav".to_string();
        render.path = "renders/master.wav".to_string();
        render.sha256 = "abc".to_string();
        render.sample_rate = 48_000;
        render.channels = 2;
        render.duration_seconds = 60.0;
        render.source_snapshot_id = "snap_1".to_string();
        render.command_id = "cmd_1".to_string();
        render.compiler_version = "0.1.0".to_string();
        render.audio_engine_version = "0.1.0".to_string();
        let rview = RenderManifestView::from_manifest(&render);
        assert_eq!(rview.artifact_id, "art_p1");
        assert_eq!(rview.manifest_hash, render.manifest_hash());
        assert!(rview.complete);
        #[expect(clippy::unwrap_used, reason = "Test assertions")]
        let json = serde_json::to_string_pretty(&rview).unwrap();
        assert!(json.contains("source_snapshot_id"));

        let ledger = LedgerSummary {
            project_id: "p1".to_string(),
            total_events: 42,
            events_by_type: BTreeMap::new(),
            last_event_timestamp: None,
            events_per_minute: 1.5,
        };
        #[expect(clippy::unwrap_used, reason = "Test assertions")]
        let json = serde_json::to_string_pretty(&ledger).unwrap();
        assert!(json.contains("total_events"));

        let branches = BranchesSummary {
            project_id: "p1".to_string(),
            active_branches: vec![],
            archived_count: 0,
        };
        #[expect(clippy::unwrap_used, reason = "Test assertions")]
        let json = serde_json::to_string_pretty(&branches).unwrap();
        assert!(json.contains("active_branches"));

        let candidates = CandidatesSummary {
            project_id: "p1".to_string(),
            total_count: 3,
            by_status: BTreeMap::new(),
            top_candidates: vec![],
        };
        #[expect(clippy::unwrap_used, reason = "Test assertions")]
        let json = serde_json::to_string_pretty(&candidates).unwrap();
        assert!(json.contains("total_count"));

        let lessons = MemoryLessonsView {
            project_id: "p1".to_string(),
            total_lessons: 5,
            by_trigger: BTreeMap::new(),
            top_lessons: vec![],
        };
        #[expect(clippy::unwrap_used, reason = "Test assertions")]
        let json = serde_json::to_string_pretty(&lessons).unwrap();
        assert!(json.contains("total_lessons"));

        let templates = RepairTemplatesView {
            project_id: "p1".to_string(),
            total_templates: 10,
            top_templates: vec![],
        };
        #[expect(clippy::unwrap_used, reason = "Test assertions")]
        let json = serde_json::to_string_pretty(&templates).unwrap();
        assert!(json.contains("total_templates"));
    }

    #[test]
    fn engineering_resources_serialize() {
        let specs = EngineeringSpecs {
            specs: vec![EngineeringSpecBrief {
                id: "SPEC-001".to_string(),
                title: "Scale compliance".to_string(),
                component: "validator".to_string(),
                priority: "high".to_string(),
                status: "implemented".to_string(),
            }],
        };
        #[expect(clippy::unwrap_used, reason = "Test assertions")]
        let json = serde_json::to_string_pretty(&specs).unwrap();
        assert!(json.contains("SPEC-001"));

        let next_task = NextTask {
            task_id: "TASK-001".to_string(),
            title: "Add transition heuristics".to_string(),
            status: "ready".to_string(),
            dependencies_met: true,
            acceptance_criteria: vec!["Transition energy delta >= 0.15".to_string()],
            test_commands: vec!["cargo test transition".to_string()],
        };
        #[expect(clippy::unwrap_used, reason = "Test assertions")]
        let json = serde_json::to_string_pretty(&next_task).unwrap();
        assert!(json.contains("TASK-001"));

        let eval = EngineeringEvalSummary {
            total_tests: 100,
            passing_tests: 95,
            failing_tests: 5,
            golden_pass_rate: 0.9,
            last_run: None,
        };
        #[expect(clippy::unwrap_used, reason = "Test assertions")]
        let json = serde_json::to_string_pretty(&eval).unwrap();
        assert!(json.contains("golden_pass_rate"));

        let trace = TraceabilityEntry {
            feature_id: "FEAT-001".to_string(),
            spec_id: Some("SPEC-001".to_string()),
            task_id: Some("TASK-001".to_string()),
            code_files: vec!["src/scale_check.rs".to_string()],
            test_files: vec!["tests/scale_check.rs".to_string()],
            golden_tests: vec!["golden/scale_compliance".to_string()],
            doc_files: vec!["docs/validation-metrics.md".to_string()],
            mcp_tools: vec!["run_validation".to_string()],
        };
        #[expect(clippy::unwrap_used, reason = "Test assertions")]
        let json = serde_json::to_string_pretty(&trace).unwrap();
        assert!(json.contains("FEAT-001"));

        let onboarding = OnboardingPacket {
            system_map: "5-plane architecture".to_string(),
            capabilities: "system://capabilities".to_string(),
            schemas: "system://contracts".to_string(),
            tools: "tool catalog".to_string(),
            playbooks: vec![
                "general".to_string(),
                "rock".to_string(),
                "house".to_string(),
                "ambient".to_string(),
                "techno".to_string(),
                "jazz".to_string(),
                "hip-hop".to_string(),
                "pop".to_string(),
                "funk".to_string(),
                "blues".to_string(),
                "synthwave".to_string(),
                "drum_and_bass".to_string(),
                "cinematic".to_string(),
                "lo-fi".to_string(),
            ],
            failure_taxonomy: vec![FailureTaxonomyEntry {
                code: "SCALE_VIOLATION".to_string(),
                category: "harmony".to_string(),
                description: "Note outside scale".to_string(),
                remediation: vec!["quantize_to_scale".to_string()],
                severity: Severity::Warning,
            }],
            budget_templates: vec![BudgetTemplate {
                name: "standard".to_string(),
                description: "Standard budget".to_string(),
                max_iterations: 12,
                max_tool_calls: 250,
                max_render_seconds: 600.0,
                max_cpu_seconds: 1800.0,
            }],
            success_criteria: vec!["All hard gates pass".to_string()],
            golden_examples: vec!["melodic_techno_64_bar".to_string()],
            do_not_do: vec!["Don't skip validation".to_string()],
        };
        #[expect(clippy::unwrap_used, reason = "Test assertions")]
        let json = serde_json::to_string_pretty(&onboarding).unwrap();
        assert!(json.contains("system_map"));
    }

    #[test]
    fn rock_playbook_guides_the_full_band_workflow() {
        // The skills layer: a rock producer gets the ordered workflow with
        // the exact tools, the octave map, mix/master guidance, and the
        // common mistakes — not a bare tool list.
        let playbook = production_playbook_for("nu_metal");
        assert_eq!(playbook.genre, "nu_metal");
        assert!(
            playbook.steps.len() >= 5,
            "rock playbook has the full workflow: {}",
            playbook.steps.len()
        );
        // Register discipline is explicit: guitar 2–4, vocal 4–6, bass 1–2.
        assert!(playbook.octave_map.iter().any(|o| o.contains("2–4")));
        assert!(playbook.octave_map.iter().any(|o| o.contains("1–2")));
        // The workflow names real tools in order.
        let first_tools = &playbook.steps[0].tools;
        assert!(
            first_tools
                .iter()
                .any(|t| t == "generate_config_from_brief")
        );
        assert!(
            playbook
                .steps
                .iter()
                .any(|s| s.tools.iter().any(|t| t == "apply_arrangement_template"))
        );
        assert!(
            playbook
                .steps
                .iter()
                .any(|s| s.tools.iter().any(|t| t == "set_layer_register"))
        );
        // Mix/master + mistakes complete the guidance.
        assert!(!playbook.mix_master.is_empty());
        assert!(
            playbook
                .common_mistakes
                .iter()
                .any(|m| m.to_lowercase().contains("sidechain")),
            "warns against sidechaining the rock bass"
        );
    }

    #[test]
    fn every_named_genre_has_a_playbook() {
        for genre in [
            "techno",
            "house",
            "ambient",
            "trance",
            "dubstep",
            "rock",
            "unknown_genre",
        ] {
            let playbook = production_playbook_for(genre);
            assert_eq!(playbook.genre, genre);
            assert!(!playbook.steps.is_empty(), "{genre} has guided steps");
        }
    }

    #[test]
    fn arrangement_templates_resource_covers_genres_and_energy_arc() {
        // The G6 resource must expose every named genre table plus the
        // default fallback, each with the production 5-section archetype and
        // an energy arc peaking at the drop.
        let resource = arrangement_templates_resource();
        assert!(
            !resource.genres.is_empty() && resource.genres.len() == resource.templates.len(),
            "one template per named genre"
        );
        for (genre, template) in resource.genres.iter().zip(&resource.templates) {
            assert_eq!(template.genre, *genre);
            assert_eq!(template.sections.len(), 5, "{genre} has 5 sections");
            assert!(
                template.sections.iter().all(|s| s.bars > 0),
                "{genre} sections all have bars"
            );
            // The drop (index 2) carries the energy peak of the arc.
            let drop_energy = template.sections[2].energy;
            assert!(
                template.sections.iter().all(|s| s.energy <= drop_energy),
                "{genre} energy arc peaks at the drop"
            );
            // Each section lists the layers active in it.
            assert!(template.sections.iter().all(|s| !s.layers.is_empty()));
        }
        assert_eq!(resource.default.sections.len(), 5);
        #[expect(clippy::unwrap_used, reason = "Test assertions")]
        let json = serde_json::to_string_pretty(&resource).unwrap();
        assert!(json.contains("\"genres\""));
        assert!(json.contains("\"default\""));
        assert!(json.contains("techno"));
    }
}
