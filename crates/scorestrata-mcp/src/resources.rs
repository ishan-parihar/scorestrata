//! MCP resources — inspectable state that agents can query.
//!
//! Resources follow the "tools mutate, resources inspect" principle.
//! Returns summary-level data by default, with drill-down via scoped lenses.
//!
//! Implements architecture spec §10.3 (system resources) and §10.4
//! (project resources and engineering resources).

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
    /// Create from a core PolicyProfile.
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
    let (intro, build, drop, breakdown, outro) = if g.contains("ambient") {
        (32u32, 8u32, 16u32, 16u32, 16u32)
    } else if g.contains("house") {
        (16u32, 8u32, 16u32, 8u32, 8u32)
    } else if g.contains("rock")
        || g.contains("metal")
        || g.contains("nu_metal")
        || g.contains("punk")
    {
        // Rock/nu-metal: shorter intro, big chorus (the peak), and a
        // breakdown for the quiet middle-eight — the Linkin Park shape.
        (8u32, 8u32, 32u32, 8u32, 8u32)
    } else if g.contains("techno") || g.contains("trance") || g.contains("dubstep") {
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
    let genres = ["techno", "house", "ambient", "trance", "dubstep", "rock"];
    let templates: Vec<ArrangementTemplate> =
        genres.iter().map(|g| arrangement_template_for(g)).collect();
    ArrangementTemplates {
        genres: genres.iter().map(|g| g.to_string()).collect(),
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
    let ppq = scorestrata_core::domain::PPQ as u64;
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
            playbooks: vec!["repair".to_string(), "composition".to_string()],
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
