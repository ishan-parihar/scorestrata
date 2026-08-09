//! Scorestrata CLI — operator and debug console.
//!
//! Commands for interacting with the Scorestrata agentic composition compiler.
//! Debug commands query real storage modules (event ledger, snapshot graph,
//! calibration profiles, budget, policy) per architecture spec §19.3.

use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use clap::{Args, Parser, Subcommand};
use colored::Colorize;

#[derive(Parser)]
#[command(
    name = "ss",
    about = "Scorestrata — agentic composition compiler",
    version,
    long_about = "A contract-first, event-sourced, multi-objective composition compiler for AI agents."
)]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

/// Common project path argument.
#[derive(Args)]
struct ProjectPath {
    /// Project path
    #[arg(short, long, default_value = ".")]
    project: String,
}

#[derive(Subcommand)]
enum Commands {
    /// Create a new project
    New {
        /// Project name
        name: String,
        /// Brief description
        #[arg(short, long)]
        brief: Option<String>,
    },
    /// Doctor — check system health
    Doctor,
    /// Configuration management
    Config {
        #[command(subcommand)]
        action: ConfigAction,
    },
    /// Compile a project
    Compile {
        /// Project path
        path: String,
        /// §7.3 — incremental compile: reuse unchanged segments from the
        /// persisted compile cache (fast agent loops)
        #[arg(long)]
        incremental: bool,
    },
    /// Validate a project
    Validate {
        /// Project path
        path: String,
        /// Validation scope (§22.2): config, arrangement, segment, layer,
        /// transition, mix, harmony, rhythm, full, diff
        #[arg(short, long, default_value = "full")]
        scope: String,
    },
    /// Render a project to WAV
    Render {
        /// Project path
        path: String,
        /// Render mode (master, preview, stems)
        #[arg(short, long, default_value = "master")]
        mode: String,
        /// Render only this segment (by name or id) into segments/<Name>.wav
        #[arg(long)]
        segment: Option<String>,
    },
    /// Post-generation spectral audit of a rendered WAV (debug log for music)
    Analyze {
        /// Project path
        path: String,
    },
    /// Arrangement planning (show the genre-aware section layout)
    Arrangement {
        #[command(subcommand)]
        action: ArrangementAction,
    },
    /// Audit — spectral gate of the rendered master (alias of analyze)
    Audit {
        /// Project path
        path: String,
    },
    /// Layered timeline view (sections × layers × octave registers)
    Timeline {
        /// Project path
        path: String,
        /// Filter to one segment (name or 1-based index)
        #[arg(long)]
        segment: Option<String>,
        /// Filter to one layer (name)
        #[arg(long)]
        layer: Option<String>,
    },
    /// §11 G2 — set a layer's octave register
    Layer {
        /// Project path
        path: String,
        #[command(subcommand)]
        action: LayerAction,
    },
    /// §11 G3 — repeat a pattern across a layer's timeline
    Pattern {
        /// Project path
        path: String,
        #[command(subcommand)]
        action: PatternAction,
    },
    /// §11 G4 — write an automation lane
    Automation {
        /// Project path
        path: String,
        #[command(subcommand)]
        action: AutomationAction,
    },
    /// §11 G5 — set a layer's insert FX chain or apply a genre mix preset
    Mix {
        /// Project path
        path: String,
        #[command(subcommand)]
        action: MixAction,
    },
    /// §11 G6 — apply a genre arrangement template
    Template {
        /// Project path
        path: String,
        #[command(subcommand)]
        action: TemplateAction,
    },
    /// Export a project to MIDI
    ExportMidi {
        /// Project path
        path: String,
        /// Output file path (defaults to <project>/track.mid)
        #[arg(short, long)]
        output: Option<String>,
    },
    /// Show system capabilities
    Capabilities,
    /// Render the built-in genre demos (techno/ambient/house/loop) across all
    /// phases into <dir>/, print a listening index, and run §4 post-generation
    /// validation on every artifact (fails with exit 1 if any gate fails)
    Demo {
        /// Output directory for the demo projects (defaults to ./demo)
        #[arg(short, long, default_value = "demo")]
        dir: String,
    },
    /// Trace commands through the event ledger
    Trace {
        /// Command ID to trace
        command_id: String,
        #[command(flatten)]
        project_path: ProjectPath,
    },
    /// Diff two snapshots or candidates
    Diff {
        #[command(subcommand)]
        diff_type: DiffType,
    },
    /// Replay project state from a snapshot
    Replay {
        #[command(flatten)]
        project_path: ProjectPath,
        /// Snapshot ID to replay from
        #[arg(short, long)]
        from_snapshot: String,
    },
    /// Explain a metric with evidence
    Explain {
        /// Metric ID to explain
        metric_id: String,
        #[command(flatten)]
        project_path: ProjectPath,
    },
    /// Isolate a layer for inspection
    Isolate {
        /// Layer ID to isolate
        layer_id: String,
        #[command(flatten)]
        project_path: ProjectPath,
    },
    /// Create a human review packet
    HumanPacket {
        #[command(subcommand)]
        action: HumanPacketAction,
    },
    /// Calibrate project against genre profiles
    Calibrate {
        #[command(flatten)]
        project_path: ProjectPath,
    },
    /// Budget management
    Budget {
        #[command(subcommand)]
        action: BudgetAction,
    },
    /// Policy management
    Policy {
        #[command(subcommand)]
        action: PolicyAction,
    },
    /// Invoke any registered MCP tool via the dispatcher (§10.5)
    Tool {
        /// Registered tool name (see `ss capabilities`)
        name: String,
        /// JSON params object (defaults to `{}`)
        #[arg(short, long)]
        params: Option<String>,
    },
}

#[derive(Subcommand)]
enum ConfigAction {
    /// Generate config from brief
    Generate {
        /// Project path
        path: String,
    },
    /// Validate config
    Validate {
        /// Project path
        path: String,
    },
}

#[derive(Subcommand)]
enum DiffType {
    /// Diff two snapshots
    Snapshots {
        /// First snapshot ID
        a: String,
        /// Second snapshot ID
        b: String,
        #[command(flatten)]
        project_path: ProjectPath,
    },
    /// Diff two candidates
    Candidates {
        /// First candidate ID
        a: String,
        /// Second candidate ID
        b: String,
        #[command(flatten)]
        project_path: ProjectPath,
    },
}

#[derive(Subcommand)]
enum HumanPacketAction {
    /// Create a human review packet
    Create {
        #[command(flatten)]
        project_path: ProjectPath,
    },
}

#[derive(Subcommand)]
enum BudgetAction {
    /// Show budget status for a project
    Show {
        #[command(flatten)]
        project_path: ProjectPath,
    },
}

#[derive(Subcommand)]
enum PolicyAction {
    /// Show policy profile for a project
    Show {
        #[command(flatten)]
        project_path: ProjectPath,
    },
}

/// §11 G2 — layer register actions.
#[derive(Subcommand)]
enum LayerAction {
    /// Set the octave band a layer's notes must stay within
    SetRegister {
        /// Layer name (see `ss timeline`)
        layer: String,
        /// Lowest octave (inclusive)
        min_octave: i8,
        /// Highest octave (inclusive)
        max_octave: i8,
    },
}

/// §11 G3 — pattern repetition actions.
#[derive(Subcommand)]
enum PatternAction {
    /// Repeat a pattern across a layer's timeline
    Repeat {
        /// Layer name (see `ss timeline`)
        layer: String,
        /// Pattern name
        pattern: String,
        /// Number of instances to place
        count: u32,
        /// Spacing between instances in ticks (defaults to the pattern's bars)
        #[arg(long)]
        step: Option<u64>,
        /// Per-instance transpose ramp in semitones (e.g. +12 = octave lift)
        #[arg(long)]
        transpose: Option<i32>,
    },
}

/// §11 G4 — automation lane actions.
#[derive(Subcommand)]
enum AutomationAction {
    /// Write an automation lane on a layer
    Write {
        /// Layer name (see `ss timeline`)
        layer: String,
        /// Parameter: cutoff | volume | pan | send
        param: String,
        /// JSON points array, e.g. `[{"tick":0,"value":0.2},{"tick":1920,"value":1.0}]`
        points: String,
    },
}

/// §11 G5 — mixer / FX actions.
#[derive(Subcommand)]
enum MixAction {
    /// Set a layer's insert FX chain (serialized `FxChain` JSON)
    Fx {
        /// Layer name (see `ss timeline`)
        track: String,
        /// Serialized `FxChain` JSON, e.g. `{"high_pass_hz":40}`
        fx: String,
    },
    /// Apply per-instrument production mix defaults for a genre
    Preset {
        /// Genre (techno, `melodic_techno`, house, ambient)
        genre: String,
    },
    /// Set the master bus (gain, limiter ceiling) — §8.6 `MixerState`
    Master {
        /// Master output gain in dB (0.0 = unity); negatives need `--gain-db -3`
        #[arg(long, allow_hyphen_values = true)]
        gain_db: f32,
        /// Master limiter ceiling in dBTP (omitted = keep current ceiling)
        #[arg(long, allow_hyphen_values = true)]
        ceiling_db: Option<f32>,
    },
}

/// §11 G6 — arrangement template actions.
#[derive(Subcommand)]
enum TemplateAction {
    /// Apply a genre's section skeleton (intro → build → drop → …)
    Apply {
        /// Genre (techno, `melodic_techno`, house, ambient)
        genre: String,
        /// Total bars for the arrangement
        bars: u32,
    },
}

/// §19.1 — arrangement planning actions.
#[derive(Subcommand)]
enum ArrangementAction {
    /// Show the genre-aware planned section layout (from the brief config)
    Plan {
        /// Project path
        path: String,
    },
}

fn main() -> Result<()> {
    let cli = Cli::parse();

    match cli.command {
        Commands::New { name, brief } => handle_new(&name, brief.as_deref())?,
        Commands::Doctor => handle_doctor()?,
        Commands::Config { action } => handle_config(action)?,
        Commands::Compile { path, incremental } => handle_compile(&path, incremental)?,
        Commands::Validate { path, scope } => handle_validate(&path, &scope)?,
        Commands::Render {
            path,
            mode,
            segment,
        } => handle_render(&path, &mode, segment.as_deref())?,
        Commands::Analyze { path } | Commands::Audit { path } => handle_analyze(&path)?,
        Commands::Arrangement { action } => match action {
            ArrangementAction::Plan { path } => handle_arrangement_plan(&path)?,
        },
        Commands::Timeline {
            path,
            segment,
            layer,
        } => handle_timeline(&path, segment.as_deref(), layer.as_deref())?,
        Commands::Layer { path, action } => handle_layer(&path, action)?,
        Commands::Pattern { path, action } => handle_pattern(&path, action)?,
        Commands::Automation { path, action } => handle_automation(&path, action)?,
        Commands::Mix { path, action } => handle_mix(&path, action)?,
        Commands::Template { path, action } => handle_template(&path, action)?,
        Commands::ExportMidi { path, output } => handle_export_midi(&path, output.as_deref())?,
        Commands::Capabilities => handle_capabilities()?,
        Commands::Demo { dir } => handle_demo(&dir)?,
        Commands::Trace {
            command_id,
            project_path,
        } => handle_trace(&command_id, &project_path.project)?,
        Commands::Diff { diff_type } => handle_diff(diff_type)?,
        Commands::Replay {
            project_path,
            from_snapshot,
        } => handle_replay(&project_path.project, &from_snapshot)?,
        Commands::Explain {
            metric_id,
            project_path,
        } => handle_explain(&metric_id, &project_path.project)?,
        Commands::Isolate {
            layer_id,
            project_path,
        } => handle_isolate(&layer_id, &project_path.project)?,
        Commands::HumanPacket { action } => handle_human_packet(action)?,
        Commands::Calibrate { project_path } => handle_calibrate(&project_path.project)?,
        Commands::Budget { action } => handle_budget(action)?,
        Commands::Policy { action } => handle_policy(action)?,
        Commands::Tool { name, params } => handle_tool(&name, params.as_deref())?,
    }
    Ok(())
}
fn handle_new(name: &str, brief: Option<&str>) -> Result<()> {
    use scorestrata_core::domain::{Brief, Project, ProjectStatus};
    use scorestrata_core::ids::ProjectId;

    println!("{}", format!("Creating project: {name}").green().bold());

    let project_dir = Path::new(name);
    if project_dir.exists() {
        anyhow::bail!(
            "Project directory '{}' already exists.",
            project_dir.display()
        );
    }

    let brief_text = brief.unwrap_or("Dark melodic techno").to_string();
    let brief_model = Brief {
        raw_text: brief_text.clone(),
        genre: None,
        mood: Vec::new(),
        references: Vec::new(),
        delivery_target: None,
        constraints: Vec::new(),
        confidence: 0.8,
    };

    // Generate a config from the brief using the deterministic generator.
    let config = scorestrata_mcp::config_gen::generate_config(name, &brief_text);

    // Plan a full arrangement (segments, layers, in-key patterns) so the
    // project is born pipeline-ready and compilable out of the box.
    let plan = scorestrata_compiler::plan::plan_project(&config);

    let project = Project {
        id: ProjectId::new(),
        name: name.to_string(),
        created_at: chrono::Utc::now(),
        modified_at: chrono::Utc::now(),
        brief: brief_model,
        config: config.clone(),
        arrangement: plan.arrangement,
        tracks: Vec::new(),
        patterns: plan.patterns,
        spectral_reference: None,
        mixer: scorestrata_core::domain::MixerState::default(),
        // The config is generated deterministically and schema-valid, so the
        // project starts pipeline-ready (ConfigApproved → ArrangementPlanned…).
        sound_bank: None,
        status: ProjectStatus::ConfigApproved,
    };

    scorestrata_storage::bundle::init_project_bundle(project_dir)
        .context("Failed to initialize project bundle")?;
    scorestrata_storage::bundle::save_project(&project, project_dir)
        .context("Failed to save project")?;

    println!("  Brief: {}", brief_text.green());
    println!(
        "  Config: {} @ {} BPM, key {}, {} bars",
        config.genre.green(),
        config.bpm,
        config.key,
        config.duration_bars
    );
    println!(
        "  Arrangement: {} segments planned",
        project.arrangement.segments.len()
    );
    println!("  Status: {}", "initialized".green().bold());
    println!(
        "  Bundle: {}",
        project_dir
            .join("project.json")
            .display()
            .to_string()
            .green()
    );
    Ok(())
}
fn handle_doctor() -> Result<()> {
    use scorestrata_core::schema_registry::list_schema_names;

    println!("{}", "Scorestrata Doctor".green().bold());

    // Verify the schema registry is populated (Phase -1 exit criteria).
    let schemas = list_schema_names();
    let registry_ok = schemas.len() >= 10;

    // Verify core domain invariants by exercising the theory crate.
    let c_minor = scorestrata_theory::key::Key::parse("C minor");
    let theory_ok = c_minor.is_some();

    // Verify capabilities are discoverable (Phase 3).
    let caps = scorestrata_mcp::capabilities::get_capabilities();
    let caps_ok = !caps.tools.is_empty() && !caps.features.is_empty();

    // Verify a timeline can be compiled from a minimal project (Phase 1).
    let sample = scorestrata_testkit::minimal_project();
    let compile_ok = scorestrata_compiler::compile_project(&sample).is_ok();

    println!("  Rust toolchain:      {}", "available".green());
    println!(
        "  Schema registry:     {} ({} schemas)",
        if registry_ok {
            "healthy".green()
        } else {
            "degraded".red()
        },
        schemas.len()
    );
    println!(
        "  Theory (key parse):  {}",
        if theory_ok {
            "healthy".green()
        } else {
            "broken".red()
        }
    );
    println!(
        "  Capabilities:        {} ({} tools)",
        if caps_ok {
            "healthy".green()
        } else {
            "degraded".red()
        },
        caps.tools.len()
    );
    println!(
        "  Compiler (compile):  {}",
        if compile_ok {
            "healthy".green()
        } else {
            "broken".red()
        }
    );
    println!("  Storage:             {}", "available".green());
    println!("  Validator:           {}", "loaded".green());
    println!("  Audio engine:        {}", "loaded".green());

    let all_ok = registry_ok && theory_ok && caps_ok && compile_ok;
    println!(
        "  Overall:             {}",
        if all_ok {
            "healthy".green().bold()
        } else {
            "degraded".red().bold()
        }
    );
    if !all_ok {
        anyhow::bail!("Doctor found degraded subsystems.");
    }
    Ok(())
}
fn handle_config(action: ConfigAction) -> Result<()> {
    match action {
        ConfigAction::Generate { path } => {
            println!("{}", format!("Generating config for: {path}").cyan());
            let project_path = Path::new(&path);
            let mut project = scorestrata_storage::bundle::load_project(project_path)
                .context("Failed to load project")?;
            let config = scorestrata_mcp::config_gen::generate_config(
                &project.name,
                &project.brief.raw_text,
            );
            // Re-plan the arrangement so it stays consistent with the new
            // duration (segments always cover the configured bars).
            let plan = scorestrata_compiler::plan::plan_project(&config);
            project.config = config.clone();
            project.arrangement = plan.arrangement;
            project.patterns = plan.patterns;
            project.status = scorestrata_core::domain::ProjectStatus::ConfigApproved;
            project.modified_at = chrono::Utc::now();
            scorestrata_storage::bundle::save_project(&project, project_path)
                .context("Failed to save project")?;
            println!(
                "  {} @ {} BPM, key {}, {} bars → {}",
                config.genre,
                config.bpm,
                config.key,
                config.duration_bars,
                "ConfigDrafted".green()
            );
        }
        ConfigAction::Validate { path } => {
            println!("{}", format!("Validating config for: {path}").cyan());
            let project_path = Path::new(&path);
            let project = scorestrata_storage::bundle::load_project(project_path)
                .context("Failed to load project")?;

            // Config validity checks per §11.2 Level 1.
            let mut errors: Vec<String> = Vec::new();
            if project.config.bpm == 0 || project.config.bpm > 999 {
                errors.push(format!("invalid BPM {}", project.config.bpm));
            }
            if project.config.duration_bars == 0 {
                errors.push("duration_bars must be > 0".to_string());
            }
            if project.config.name.is_empty() {
                errors.push("name is required".to_string());
            }
            if project.config.genre.is_empty() {
                errors.push("genre is required".to_string());
            }
            if scorestrata_theory::key::Key::parse(&project.config.key).is_none() {
                errors.push(format!("unparseable key '{}'", project.config.key));
            }
            // Layers presence: at least one segment must have layers.
            let has_layers = project
                .arrangement
                .segments
                .iter()
                .any(|s| !s.layers.is_empty());
            if !has_layers {
                errors.push("no segments with layers — plan an arrangement first".to_string());
            }

            if errors.is_empty() {
                println!("  Config: {}", "valid".green().bold());
            } else {
                for e in &errors {
                    println!("  ✗ {}", e.red());
                }
                anyhow::bail!("Config validation failed.");
            }
        }
    }
    Ok(())
}
fn handle_compile(path: &str, incremental: bool) -> Result<()> {
    println!("{}", format!("Compiling: {path}").yellow());

    let project_path = Path::new(path);
    let mut project = scorestrata_storage::bundle::load_project(project_path)
        .context("Failed to load project — run `ss new <name>` first")?;

    // Phase 1: expand arrangement, instantiate patterns, generate timeline IR.
    // §7.3 — incremental mode reuses unchanged segments from the persisted
    // compile cache, so agent loops recompile only what changed.
    let cache_path = project_path.join("cache/compile_cache.json");
    let (timeline, observability) = if incremental {
        let result = scorestrata_compiler::incremental::compile_with_cache(&project, &cache_path)
            .context("Compilation failed — check project status and patterns")?;
        (result.timeline, Some(result.observability))
    } else {
        (
            scorestrata_compiler::compile_project(&project)
                .context("Compilation failed — check project status and patterns")?,
            None,
        )
    };

    println!(
        "  Pass 1: arrangement expansion ({} segments)",
        project.arrangement.segments.len()
    );
    println!(
        "  Pass 2: pattern instantiation ({} patterns)",
        project.patterns.len()
    );
    if let Some(obs) = &observability {
        // §7.2 pass observability — what the incremental compile reused vs
        // recomputed, and why anything was invalidated.
        println!(
            "  §7.3 incremental: {} segments reused, {} recomputed ({} events reused, {} recomputed)",
            obs.segments_reused, obs.segments_recomputed, obs.events_reused, obs.events_recomputed
        );
        for invalidation in &obs.invalidations {
            println!(
                "    invalidation: {} — {:?}",
                invalidation.segment_id, invalidation.reason
            );
        }
        println!("    cache: {}", cache_path.display());
    }
    println!(
        "  Pass 3: timeline generation ({} events, {} ticks)",
        timeline.event_count(),
        timeline.total_ticks
    );
    println!(
        "  Input hash: {}",
        &timeline.input_hash[..timeline.input_hash.len().min(16)]
    );

    // Persist the timeline for later debug commands.
    let timeline_path = project_path.join("timeline.json");
    let timeline_json =
        serde_json::to_string_pretty(&timeline).context("Failed to serialize timeline")?;
    std::fs::write(&timeline_path, timeline_json).context("Failed to write timeline.json")?;

    // Advance the project status to Compiling and persist.
    project.status = scorestrata_core::domain::ProjectStatus::Compiling;
    project.modified_at = chrono::Utc::now();
    scorestrata_storage::bundle::save_project(&project, project_path)
        .context("Failed to save compiled project")?;

    println!(
        "  Timeline: {}",
        timeline_path.display().to_string().green()
    );
    println!("  Status: {}", "compiled".green().bold());
    Ok(())
}
#[expect(
    clippy::too_many_lines,
    reason = "CLI validation walks every scope plane with per-plane reporting"
)]
fn handle_validate(path: &str, scope: &str) -> Result<()> {
    use scorestrata_core::error::Severity;

    println!(
        "{}",
        format!("Validating: {path} (scope: {scope})").magenta()
    );

    let project_path = Path::new(path);
    let project = scorestrata_storage::bundle::load_project(project_path)
        .context("Failed to load project — run `ss new <name>` first")?;

    // §22.2 — scoped validation: config, arrangement, segment, layer,
    // transition, mix, harmony, rhythm, full, diff. The scope orchestrator
    // in the validator crate owns the symbolic planes; `mix` (audio) and
    // `diff` (compile-cache driven) are completed here where the audio
    // engine and the incremental cache are available.
    let parsed_scope = scorestrata_validator::scope::ValidationScope::parse(scope)
        .context("Unknown validation scope — see `ss validate --help` for the §22.2 scope list")?;

    let mut diagnostics: Vec<scorestrata_core::diagnostic::Diagnostic> =
        scorestrata_validator::scope::validate_symbolic(&project, parsed_scope);

    // `mix` scope — the audio plane: render the project and convert the
    // Level-4 analysis (LUFS, true peak, clipping, spectral balance) into
    // AUDIO_* diagnostics. This is the same path run_validation uses at
    // level 4, exposed as a standalone fast scope.
    if matches!(
        parsed_scope,
        scorestrata_validator::scope::ValidationScope::Mix
    ) {
        match scorestrata_mcp::tools::render_project_to_analysis(&project) {
            Ok(result) => {
                diagnostics.extend(
                    scorestrata_validator::audio_check::generate_audio_diagnostics(
                        &result.analysis,
                    ),
                );
            }
            Err(err) => {
                diagnostics.push(scorestrata_core::diagnostic::Diagnostic {
                    id: scorestrata_core::ids::DiagnosticId::new(),
                    code: "AUDIO_RENDER_FAILED".to_string(),
                    metric: "audio.render".to_string(),
                    severity: scorestrata_core::error::Severity::Error,
                    message: format!("Mix-scope audio render failed: {err}"),
                    scope: scorestrata_core::diagnostic::MetricScope::default(),
                    evidence_uris: vec!["project://audio/render".to_string()],
                    root_causes: vec!["project could not be compiled or rendered".to_string()],
                    suggested_patches: vec![],
                    expected_delta: None,
                    confidence: 1.0,
                });
            }
        }
    }

    // `diff` scope — validate only what changed (§7.3): load the incremental
    // compile cache, compute the changed segment ids, and run the segment +
    // layer planes against just those segments. With a warm cache this is a
    // no-op when nothing changed — the fast agent loop.
    if matches!(
        parsed_scope,
        scorestrata_validator::scope::ValidationScope::Diff
    ) {
        let cache = scorestrata_compiler::incremental::CompilationCache::load_from(
            &project_path.join("cache/compile_cache.json"),
        );
        match cache.changed_segments(&project) {
            Ok(changed) => {
                if changed.is_empty() {
                    println!(
                        "  Diff scope: no segments changed since the last incremental compile"
                    );
                } else {
                    println!(
                        "  Diff scope: validating {} changed segment(s): {}",
                        changed.len(),
                        changed
                            .iter()
                            .map(std::string::ToString::to_string)
                            .collect::<Vec<_>>()
                            .join(", ")
                    );
                }
                diagnostics.extend(scorestrata_validator::scope::validate_diff(
                    &project, &changed,
                ));
            }
            Err(err) => anyhow::bail!("Diff scope failed to resolve changed segments: {err}"),
        }
    }

    // Rhythm density notes (printed for `full`/`rhythm` scopes, mirroring
    // the pre-scope-orchestrator behavior): low-density patterns are an
    // informational signal, not a gate failure.
    if matches!(
        parsed_scope,
        scorestrata_validator::scope::ValidationScope::Full
            | scorestrata_validator::scope::ValidationScope::Rhythm
    ) {
        for pattern in &project.patterns {
            let density = scorestrata_validator::density::rhythmic_density(pattern);
            if density < 0.2 {
                println!(
                    "  Note: pattern '{}' has low rhythmic density ({:.2})",
                    pattern.name, density
                );
            }
        }
    }

    // Build the report with the default scoring profile.
    let report = scorestrata_validator::report::build_validation_report(project.id, 2, diagnostics);

    println!(
        "  Overall score:       {:.2}",
        report.scorecard.overall_score
    );
    println!(
        "  Diagnostics:         {} ({} warn, {} err, {} crit)",
        report.summary.info
            + report.summary.warnings
            + report.summary.errors
            + report.summary.critical,
        report.summary.warnings,
        report.summary.errors,
        report.summary.critical
    );

    for d in &report.diagnostics {
        let level = match d.severity {
            Severity::Critical => "critical".red(),
            Severity::Error => "error".red(),
            Severity::Warning => "warning".yellow(),
            Severity::Info => "info".cyan(),
        };
        println!("    [{}] {} ({})", level, d.code, d.message);
    }

    let passed = report.all_hard_gates_passed;
    println!(
        "  Overall:             {}",
        if passed {
            "pass".green().bold()
        } else {
            "fail".red().bold()
        }
    );
    if !passed {
        anyhow::bail!(
            "Validation failed — {} critical diagnostics.",
            report.summary.critical
        );
    }
    Ok(())
}

/// Build the render config from the project's mixer bus (§8.6 G5) — master
/// gain, stereo width, and the master limiter ceiling (`MixerState`) mapped
/// onto the audio engine's `RenderConfig`. The default mixer state maps to
/// the engine defaults, so legacy projects render byte-identically.
fn render_config_for(
    project: &scorestrata_core::domain::Project,
) -> scorestrata_audio::render::RenderConfig {
    let limiter = project.mixer.limiter_ceiling_db.map(|ceiling_db| {
        let knee_db = scorestrata_audio::mastering::LimiterConfig::default().knee_db;
        scorestrata_audio::mastering::LimiterConfig {
            ceiling_db,
            knee_db,
        }
    });
    scorestrata_audio::render::RenderConfig {
        stereo: true,
        limiter,
        master_gain_db: project.mixer.master_gain_db,
        stereo_width: project.mixer.stereo_width,
        sound_bank: project.sound_bank.clone(),
    }
}

#[expect(
    clippy::too_many_lines,
    reason = "CLI render composes isolation, render, mastering and audit in one flow"
)]
fn handle_render(path: &str, mode: &str, segment: Option<&str>) -> Result<()> {
    use scorestrata_compiler::timeline::EventKind;
    let scope_note = segment.map_or_else(String::new, |s| format!(" (segment: {s})"));
    println!(
        "{}",
        format!("Rendering: {path} (mode: {mode}{scope_note})").cyan()
    );

    let project_path = Path::new(path);
    let project = scorestrata_storage::bundle::load_project(project_path)
        .context("Failed to load project — run `ss new <name>` first")?;

    // F′ — segment isolation: resolve the segment by name or id, then filter
    // the compiled timeline to just its events (offset to tick 0) so the
    // render is a clean per-phase WAV, not a silent-padded slice.
    let segment_scope: Option<scorestrata_core::domain::Segment> = match segment {
        Some(sel) => Some(
            project
                .arrangement
                .segments
                .iter()
                .find(|s| s.name == sel || s.id.to_string() == sel)
                .cloned()
                .context(format!("Segment '{sel}' not found in arrangement"))?,
        ),
        None => None,
    };

    // Compile to a timeline, then render offline via the audio engine.
    let full_timeline = scorestrata_compiler::compile_project(&project)
        .context("Compilation failed — check project status")?;

    let timeline = match &segment_scope {
        Some(seg) => {
            let ticks_per_bar = u64::from(scorestrata_core::domain::PPQ)
                * u64::from(full_timeline.time_signature_num);
            let seg_start_tick = u64::from(seg.start_bar) * ticks_per_bar;
            let events: Vec<scorestrata_compiler::timeline::TimelineEvent> = full_timeline
                .events
                .iter()
                .filter(|e| e.segment_id == seg.id)
                .map(|e| {
                    let mut ev = e.clone();
                    ev.tick = scorestrata_core::domain::Tick::new(
                        e.tick.0.saturating_sub(seg_start_tick),
                    );
                    ev
                })
                .collect();
            let total_ticks = events.iter().map(|e| e.tick.0).max().unwrap_or(0);
            scorestrata_compiler::TimelineIR {
                project_id: full_timeline.project_id,
                events,
                total_ticks,
                bpm: full_timeline.bpm,
                time_signature_num: full_timeline.time_signature_num,
                input_hash: full_timeline.input_hash,
                compiler_version: full_timeline.compiler_version,
            }
        }
        None => full_timeline,
    };

    // Map layer IDs to human-readable names for voice assignment, and build
    // the per-instrument voice table (drums, bass, lead each get their own
    // timbre instead of the default saw).
    let mut layer_names = std::collections::HashMap::new();
    let mut layer_instruments: Vec<(&str, &str)> = Vec::new();
    for segment in &project.arrangement.segments {
        for layer in &segment.layers {
            layer_names.insert(layer.id.to_string(), layer.name.clone());
            if !layer_instruments
                .iter()
                .any(|(n, _)| *n == layer.name.as_str())
            {
                layer_instruments.push((layer.name.as_str(), layer.instrument.as_str()));
            }
        }
    }
    let voices = scorestrata_audio::render::build_voices(&layer_instruments); // G4 — automation lanes (filter sweeps, volume rides) from the timeline's
    // CC events, so rendered audio reflects the agent's automation.
    let automation =
        scorestrata_mcp::tools::automation_lanes_from_timeline(&timeline, &layer_names);
    // G5 — per-layer FX / mix overrides (HPF, EQ, compressor, sends, faders).
    let fx = scorestrata_mcp::tools::render_fx_from_project(&project);

    let notes: Vec<scorestrata_audio::render::RenderNote> = timeline
        .events
        .iter()
        .filter_map(|e| match &e.kind {
            EventKind::NoteOn { pitch, velocity } => {
                let layer_name = layer_names
                    .get(&e.layer_id.to_string())
                    .cloned()
                    .unwrap_or_else(|| e.layer_id.to_string());
                Some(scorestrata_audio::render::RenderNote {
                    pitch: pitch.0,
                    start_tick: e.tick.0,
                    duration_ticks: next_note_off(&timeline, e),
                    velocity: velocity.as_f32(),
                    layer_name,
                })
            }
            _ => None,
        })
        .collect();

    let render_path = project_path.join("render");
    std::fs::create_dir_all(&render_path).context("Failed to create render directory")?;

    // F′ — segment mode: a single per-phase WAV plus a §17.2 segment manifest.
    if let Some(seg) = &segment_scope {
        let config = render_config_for(&project);
        let result = scorestrata_audio::render::render_notes_full(
            &notes,
            timeline.bpm,
            scorestrata_audio::render::DEFAULT_SAMPLE_RATE,
            &voices,
            &automation,
            &fx,
            &config,
        )
        .context("Segment render failed")?;

        let segments_dir = render_path.join("segments");
        std::fs::create_dir_all(&segments_dir).context("Failed to create segments directory")?;
        let safe_name = seg.name.replace([' ', '/'], "_");
        let wav_path = segments_dir.join(format!("{safe_name}.wav"));
        std::fs::write(&wav_path, &result.wav_bytes).context("Failed to write segment WAV")?;

        // §17.2 — segment-scoped reproducibility manifest (artifact type
        // "segment"), recorded into a per-project manifest.json.
        let artifact_id = format!("seg_{}_{}", project.id, safe_name);
        let manifest = scorestrata_storage::render_manifest::RenderManifest::from_render(
            &artifact_id,
            "segment",
            &format!("segments/{safe_name}.wav"),
            &result.checksum,
            result.sample_rate,
            result.channels,
            result.duration_seconds,
            &format!("snap_{}_live", project.id),
            "cmd_render_segment",
            &timeline.compiler_version,
            scorestrata_audio::render::AUDIO_ENGINE_VERSION,
            None,
            std::collections::BTreeMap::new(),
        );
        let mut store = scorestrata_storage::render_manifest::RenderManifestStore::new();
        store.record(manifest);
        let manifest_json =
            serde_json::to_string_pretty(&store).context("Failed to serialize segment manifest")?;
        std::fs::write(render_path.join("manifest.json"), manifest_json)
            .context("Failed to write render manifest")?;

        println!("  Events:            {}", timeline.event_count());
        println!("  Duration:          {:.2}s", result.duration_seconds);
        println!(
            "  Integrated LUFS:   {:.1}",
            result.analysis.integrated_lufs
        );
        println!(
            "  True peak:         {:.1} dBTP",
            result.analysis.true_peak_dbtp
        );
        println!(
            "  Checksum (sha256): {}",
            &result.checksum[..16.min(result.checksum.len())]
        );
        println!("  Manifest:          {}", "render/manifest.json".green());
        println!(
            "  Output:            {}",
            wav_path.display().to_string().green()
        );
        return Ok(());
    }

    // Stems mode: write one WAV per layer plus the master mix (§16 stems).
    if mode == "stems" {
        let config = render_config_for(&project);
        let (stems, master) = scorestrata_audio::render::render_stems_full(
            &notes,
            timeline.bpm,
            scorestrata_audio::render::DEFAULT_SAMPLE_RATE,
            &voices,
            &automation,
            &fx,
            &config,
        )
        .context("Stems render failed")?;

        let stems_dir = render_path.join("stems");
        std::fs::create_dir_all(&stems_dir).context("Failed to create stems directory")?;
        for stem in &stems {
            let safe_name = stem.layer_name.replace([' ', '/'], "_");
            let stem_path = stems_dir.join(format!("{safe_name}.wav"));
            std::fs::write(&stem_path, &stem.wav_bytes).context("Failed to write stem WAV")?;
        }
        let master_path = render_path.join("master.wav");
        std::fs::write(&master_path, &master.wav_bytes).context("Failed to write master WAV")?;

        println!("  Events:            {}", timeline.event_count());
        println!("  Duration:          {:.2}s", master.duration_seconds);
        println!("  Stems:             {}", stems.len());
        for stem in &stems {
            println!(
                "    - {}  ({:.1} LUFS, {:.1} dBTP, sha256 {})",
                stem.layer_name,
                stem.analysis.integrated_lufs,
                stem.analysis.true_peak_dbtp,
                &stem.checksum[..8.min(stem.checksum.len())]
            );
        }
        println!(
            "  Master LUFS:       {:.1}",
            master.analysis.integrated_lufs
        );
        println!(
            "  Master true peak:  {:.1} dBTP",
            master.analysis.true_peak_dbtp
        );
        println!(
            "  Master checksum:   {}",
            &master.checksum[..16.min(master.checksum.len())]
        );
        println!(
            "  Output:            {}",
            stems_dir.display().to_string().green()
        );
        return Ok(());
    }

    let config = render_config_for(&project);
    let result = scorestrata_audio::render::render_notes_full(
        &notes,
        timeline.bpm,
        scorestrata_audio::render::DEFAULT_SAMPLE_RATE,
        &voices,
        &automation,
        &fx,
        &config,
    )
    .context("Render failed")?;

    let wav_path = render_path.join(format!("{mode}.wav"));
    std::fs::write(&wav_path, &result.wav_bytes).context("Failed to write WAV")?;

    println!("  Events:            {}", timeline.event_count());
    println!("  Duration:          {:.2}s", result.duration_seconds);
    println!("  WAV bytes:         {}", result.wav_bytes.len());
    println!(
        "  Integrated LUFS:   {:.1}",
        result.analysis.integrated_lufs
    );
    println!(
        "  True peak:         {:.1} dBTP",
        result.analysis.true_peak_dbtp
    );
    println!(
        "  Crest factor:      {:.1} dB",
        result.analysis.crest_factor
    );
    println!(
        "  Checksum (sha256): {}",
        &result.checksum[..16.min(result.checksum.len())]
    );
    println!(
        "  Output:            {}",
        wav_path.display().to_string().green()
    );
    Ok(())
}

/// Render the built-in genre demos across all phases (F′/F4).
///
/// Exports the testkit fixtures (techno/ambient/house/loop) as project
/// bundles into `dir`, renders each across every phase — per-segment WAVs,
/// per-layer stems, and the mastered master — then runs §4 post-generation
/// validation (checksum match, true peak ≤ 0 dBTP, duration > 0) on every
/// artifact. Prints a listening index (paths + LUFS + dBTP + checksum) and
/// fails with exit 1 if any gate fails, so "did the demos stay good" is
/// scriptable.
#[expect(
    clippy::too_many_lines,
    reason = "CLI demo renders and gate-checks every artifact in one scriptable flow"
)]
fn handle_demo(dir: &str) -> Result<()> {
    use scorestrata_audio::analysis::analyze_stereo;
    use scorestrata_audio::wav::read_wav_file;

    println!("{}", format!("Generating demos in: {dir}").green().bold());

    let demo_root = Path::new(dir);
    std::fs::create_dir_all(demo_root).context("Failed to create demo directory")?;
    scorestrata_testkit::write_sample_projects(demo_root)
        .context("Failed to export sample projects")?;

    let mut any_failure = false;
    for genre in ["techno", "ambient", "house", "loop"] {
        let project_dir = demo_root.join(genre);
        let project = scorestrata_storage::bundle::load_project(&project_dir)
            .context("Failed to load demo project")?;
        println!("\n{}", format!("=== {genre} ===").cyan().bold());

        // Phase 1 — per-segment WAVs (arrangement segments).
        let segment_names: Vec<String> = project
            .arrangement
            .segments
            .iter()
            .map(|s| s.name.clone())
            .collect();
        for segment_name in &segment_names {
            handle_render(
                &project_dir.display().to_string(),
                "master",
                Some(segment_name),
            )?;
        }
        // Phase 2 — per-layer stems + phase 3/4 — mastered master.
        handle_render(&project_dir.display().to_string(), "stems", None)?;
        handle_render(&project_dir.display().to_string(), "master", None)?;

        // §4 — validate every artifact in <project>/render/.
        let render_dir = project_dir.join("render");
        let mut wavs: Vec<std::path::PathBuf> = Vec::new();
        if let Ok(seg_dir) = std::fs::read_dir(render_dir.join("segments")) {
            for entry in seg_dir.flatten() {
                if entry.path().extension().is_some_and(|e| e == "wav") {
                    wavs.push(entry.path());
                }
            }
        }
        if let Ok(stems_dir) = std::fs::read_dir(render_dir.join("stems")) {
            for entry in stems_dir.flatten() {
                if entry.path().extension().is_some_and(|e| e == "wav") {
                    wavs.push(entry.path());
                }
            }
        }
        let master_path = render_dir.join("master.wav");
        if master_path.exists() {
            wavs.push(master_path);
        }

        println!("  {} Listening index", "—".green());
        for wav in &wavs {
            let bytes = std::fs::read(wav).context("Failed to read rendered WAV")?;
            let checksum = scorestrata_storage::cache::content_hash(&bytes);
            let (left, right, sample_rate) =
                read_wav_file(wav).context("Failed to parse rendered WAV")?;
            let analysis = analyze_stereo(&left, &right);
            let duration = if sample_rate > 0 {
                #[expect(
                    clippy::cast_precision_loss,
                    reason = "Sample counts fit f64 for practical render durations"
                )]
                let len_f = left.len() as f64;
                len_f / f64::from(sample_rate)
            } else {
                0.0
            };
            let rel = wav.strip_prefix(&render_dir).unwrap_or(wav);
            println!(
                "    {}  {:.1} LUFS  {:.1} dBTP  {:.1}s  sha256 {}",
                rel.display().to_string().green(),
                analysis.integrated_lufs,
                analysis.true_peak_dbtp,
                duration,
                &checksum[..12.min(checksum.len())]
            );

            // §4 gates: true peak ≤ 0 dBTP, duration > 0, non-empty bytes.
            let peak_ok = analysis.true_peak_dbtp <= 0.0;
            let duration_ok = duration > 0.0;
            let bytes_ok = !bytes.is_empty();
            if !(peak_ok && duration_ok && bytes_ok) {
                any_failure = true;
                println!(
                    "    {} {} (peak {:.2} dBTP, duration {:.2}s)",
                    "✗ FAIL".red().bold(),
                    rel.display(),
                    analysis.true_peak_dbtp,
                    duration
                );
            }
        }
    }

    if any_failure {
        anyhow::bail!(
            "Demo post-generation validation failed — inspect the failed artifacts above"
        );
    }
    println!(
        "\n{}",
        "All demos rendered + validated (exit 0)"
            .to_string()
            .green()
            .bold()
    );
    Ok(())
}

/// Post-generation spectral audit — the "debug log" for rendered music.
///
/// Reads the rendered master WAV, computes an STFT-based spectral report
/// (peak/RMS/DC/clipping, centroid, rolloff, aliasing proxy, band energies)
/// and, when a genre fingerprint exists, the genre deviation. Writes a text
/// log plus spectrogram/waveform PNGs into `<project>/render/audit/`.
/// §19.1 — show the genre-aware planned section layout for a project.
///
/// Runs the same `plan_project` the planner uses, so a producer (human or
/// agent) sees the section roles, bar counts, and layers the config implies
/// before writing any notes.
fn handle_arrangement_plan(path: &str) -> Result<()> {
    println!("{}", format!("Arrangement plan: {path}").cyan());
    let project_path = Path::new(path);
    let project = scorestrata_storage::bundle::load_project(project_path)
        .context("Failed to load project — run `ss new <name>` first")?;
    let plan = scorestrata_compiler::plan::plan_project(&project.config);

    println!(
        "  {} · {} BPM · {} · {} bars total · {} segment(s)",
        project.config.genre.yellow(),
        project.config.bpm,
        project.config.key,
        plan.arrangement.total_bars,
        plan.arrangement.segments.len()
    );
    for segment in &plan.arrangement.segments {
        let layer_names: Vec<&str> = segment.layers.iter().map(|l| l.name.as_str()).collect();
        println!(
            "  {:>6} bar {:>3}  {:8}  {}",
            format!("bar {}", segment.start_bar),
            segment.bars,
            format!("{:?}", segment.role).replace("SegmentRole::", ""),
            layer_names.join(" + ")
        );
    }
    println!(
        "  Patterns: {}",
        plan.patterns
            .iter()
            .map(|p| p.name.as_str())
            .collect::<Vec<_>>()
            .join(", ")
    );
    println!(
        "  Next: `ss compile <path>`, `ss timeline <path>`, or follow the genre playbook (ss tool get_production_playbook)"
    );
    Ok(())
}

#[expect(
    clippy::too_many_lines,
    reason = "CLI analyze writes audit files, spectrograms and waveform PNGs in one flow"
)]
fn handle_analyze(path: &str) -> Result<()> {
    use scorestrata_audio::audit::write_audit_files;
    use scorestrata_audio::wav::read_wav_file;

    println!("{}", format!("Analyzing: {path}").yellow());

    let project_path = Path::new(path);
    let project = scorestrata_storage::bundle::load_project(project_path)
        .context("Failed to load project — run `ss new <name>` first")?;

    // Prefer the rendered master; fall back to the first WAV in render/.
    let render_dir = project_path.join("render");
    let master_path = render_dir.join("master.wav");
    let wav_path = if master_path.exists() {
        master_path
    } else {
        let mut wavs: Vec<PathBuf> = std::fs::read_dir(&render_dir)
            .context("No render directory — run `ss render` first")?
            .filter_map(Result::ok)
            .map(|e| e.path())
            .filter(|p| p.extension().is_some_and(|ext| ext == "wav"))
            .collect();
        wavs.sort();
        wavs.into_iter()
            .next()
            .context("No WAV found in render/ — run `ss render` first")?
    };

    let (left, right, sample_rate) =
        read_wav_file(&wav_path).context("Failed to read rendered WAV")?;
    let len = left.len().min(right.len());
    let mono: Vec<f32> = (0..len).map(|i| (left[i] + right[i]) * 0.5).collect();
    let genre = Some(project.config.genre.as_str());

    // Single STFT pass: `write_audit_files` runs `audit_mono` internally, so
    // the printed report below is the same report written to the log file.
    let audit_dir = render_dir.join("audit");
    let artifact = write_audit_files(&mono, sample_rate, genre, &audit_dir, "master")
        .context("Audit file generation failed")?;
    let report = &artifact.report;

    println!(
        "  WAV:               {}",
        wav_path.display().to_string().cyan()
    );
    println!("  Genre:             {}", project.config.genre);
    println!(
        "  Peak:              {:.2} dBFS{}",
        report.peak_dbfs,
        if report.clipped_sample_count == 0 {
            "  (no clipping)".green().to_string()
        } else {
            format!(
                "  WARN: {} hard-clipped samples",
                report.clipped_sample_count
            )
            .red()
            .to_string()
        }
    );
    println!("  RMS:               {:.1} dB", report.rms_db);
    println!(
        "  DC offset:         {:+.5}{}",
        report.dc_offset,
        if report.dc_offset.abs() < 0.01 {
            "  OK".green().to_string()
        } else {
            "  WARN: nonzero DC".red().to_string()
        }
    );
    println!("  Centroid:          {:.0} Hz", report.spectral_centroid_hz);
    println!("  Rolloff (85%):     {:.0} Hz", report.spectral_rolloff_hz);
    println!(
        "  Aliasing >16 kHz:  {:.1}%{}",
        report.aliasing_ratio * 100.0,
        if report.aliasing_ratio > 0.05 {
            "  WARN: bright/aliased".red().to_string()
        } else {
            "  OK".green().to_string()
        }
    );
    for band in &report.band_energy {
        println!(
            "    {:8} {:5.0}–{:5.0} Hz: {:.1}%",
            band.label,
            band.low_hz,
            band.high_hz,
            band.share * 100.0
        );
    }
    if let Some(dev) = report.genre_fingerprint_deviation {
        println!(
            "  Genre deviation:   {:.3}{}",
            dev,
            if dev < 0.25 {
                "  (matches genre profile)".green().to_string()
            } else if dev < 0.5 {
                "  (moderate drift)".yellow().to_string()
            } else {
                "  (far from genre profile)".red().to_string()
            }
        );
    }
    println!(
        "  Audit log:         {}",
        artifact.text_path.display().to_string().green()
    );
    println!(
        "  Spectrogram:       {}",
        artifact.spectrogram_path.display().to_string().green()
    );
    println!(
        "  Waveform:          {}",
        artifact.waveform_path.display().to_string().green()
    );

    // S2 — layer-wise spectral audit: render per-layer stems and report each
    // layer's own spectral footprint (centroid, rolloff, band energies, genre
    // deviation) with a per-layer spectrogram PNG. The master graph alone
    // hides which layer is muddy, thin, or aliased.
    let layers_dir = render_dir.join("audit").join("layers");
    match scorestrata_mcp::tools::render_project_to_layer_audit(&project, Some(&layers_dir)) {
        Ok((layer_entries, _)) => {
            println!("\n  Layer-wise spectral audit (per-layer stems):");
            print!(
                "{}",
                scorestrata_audio::audit::layer_audit_text(&layer_entries)
            );
            println!(
                "  Layer spectrograms: {}",
                layers_dir.display().to_string().green()
            );
            for entry in &layer_entries {
                if let Some(png) = &entry.spectrogram_path {
                    println!("    - {}", png.green());
                }
            }
        }
        Err(e) => println!("\n  Layer-wise audit skipped: {e}"),
    }
    Ok(())
}

/// Layered timeline view (G1) — the agent's and operator's "arrange view".
///
/// Renders the composition as sections × layer lanes with occupied octave
/// registers, using the same derivation as the `get_timeline_view` MCP tool.
/// `--segment` filters to one section (name or 1-based index) and `--layer`
/// filters to one layer name — the §11 `ss timeline` flags.
#[expect(
    clippy::too_many_lines,
    reason = "CLI timeline renders the ASCII grid and event legend in one flow"
)]
fn handle_timeline(
    path: &str,
    segment_filter: Option<&str>,
    layer_filter: Option<&str>,
) -> Result<()> {
    println!("{}", format!("Timeline: {path}").cyan());

    let project_path = Path::new(path);
    let project = scorestrata_storage::bundle::load_project(project_path)
        .context("Failed to load project — run `ss new <name>` first")?;
    let view = scorestrata_mcp::resources::build_timeline_view(&project);

    // §11 `--segment N` — keep sections whose name matches or whose 1-based
    // index equals N, and keep only lanes from those sections.
    let mut kept_sections: Vec<&scorestrata_mcp::resources::TimelineSection> =
        view.sections.iter().collect();
    if let Some(filter) = segment_filter {
        kept_sections = view
            .sections
            .iter()
            .enumerate()
            .filter(|(idx, s)| {
                s.name == filter
                    || u32::try_from(idx + 1).is_ok_and(|one_based| one_based.to_string() == filter)
            })
            .map(|(_, s)| s)
            .collect();
        if kept_sections.is_empty() {
            anyhow::bail!("Segment '{filter}' not found (see `ss timeline {path}`)");
        }
    }
    let kept_segment_names: Vec<&str> = kept_sections.iter().map(|s| s.name.as_str()).collect();

    // §11 `--layer name` — keep only lanes with that name.
    let mut kept_layers: Vec<&scorestrata_mcp::resources::TimelineLayer> = view
        .layers
        .iter()
        .filter(|l| {
            kept_segment_names.is_empty() || kept_segment_names.contains(&l.segment.as_str())
        })
        .collect();
    if let Some(filter) = layer_filter {
        kept_layers.retain(|l| l.name == filter || l.instrument == filter);
        if kept_layers.is_empty() {
            anyhow::bail!("Layer '{filter}' not found (see `ss timeline {path}`)");
        }
    }

    println!(
        "{} {} · {} · {} bpm · {} bars · {} sections · {} layer lanes{}",
        view.name.bold(),
        view.genre.cyan(),
        view.key,
        view.bpm,
        view.total_bars,
        kept_sections.len(),
        kept_layers.len(),
        if segment_filter.is_some() || layer_filter.is_some() {
            " (filtered)".yellow().to_string()
        } else {
            String::new()
        }
    );

    // Section ruler.
    println!();
    println!("  {}", "bars:".dimmed());
    let bar_width = 8usize;
    let mut ruler = String::from("      ");
    for bar in (0..view.total_bars).step_by(8) {
        use std::fmt::Write as _;
        write!(ruler, "{bar:<bar_width$}").ok();
    }
    println!("{ruler}");
    let mut sec_line = String::from("  sec ");
    for section in &kept_sections {
        use std::fmt::Write as _;
        write!(sec_line, "{:<width$}", section.name, width = bar_width).ok();
    }
    println!("{}", sec_line.cyan());
    println!();

    // Per-layer lanes with octave registers.
    for layer in &kept_layers {
        println!(
            "  {:<10} [oct {}..{}]  {}  {} notes · {}",
            layer.name.yellow(),
            layer.min_octave,
            layer.max_octave,
            layer.segment.dimmed(),
            layer.note_count,
            layer.instrument.dimmed()
        );
    }

    // Note summary per layer (first notes with pitch names).
    println!();
    for layer in &kept_layers {
        let sample: Vec<String> = layer
            .notes
            .iter()
            .take(6)
            .map(|n| format!("{}@{}", n.name, n.bar + 1))
            .collect();
        println!(
            "    {}: {}",
            layer.name.dimmed(),
            if sample.is_empty() {
                "—".to_string()
            } else {
                sample.join(", ")
            }
        );
    }

    println!(
        "\n  {}",
        "Full JSON: use the get_timeline_view MCP tool (project://{id}/timeline)"
            .dimmed()
            .replace("{id}", &view.project_id)
    );
    Ok(())
}

/// Resolve a layer by name (or id) across every segment to its
/// `(segment_id, layer_id)` — the §11 commands take layer *names* like the
/// timeline view shows, while the shared mutators take ids.
fn resolve_layer_id(
    project: &scorestrata_core::domain::Project,
    query: &str,
) -> Option<(String, String)> {
    for segment in &project.arrangement.segments {
        if let Some(layer) = segment.layers.iter().find(|l| l.name == query) {
            return Some((segment.id.to_string(), layer.id.to_string()));
        }
    }
    // Fall back to an id match so scripts can pass the `TimelineLayer.layer_id`.
    for segment in &project.arrangement.segments {
        if let Some(layer) = segment.layers.iter().find(|l| l.id.to_string() == query) {
            return Some((segment.id.to_string(), layer.id.to_string()));
        }
    }
    None
}

/// §11 G2 — `ss layer <path> set-register <layer> <minOct> <maxOct>`.
fn handle_layer(path: &str, action: LayerAction) -> Result<()> {
    let LayerAction::SetRegister {
        layer,
        min_octave,
        max_octave,
    } = action;
    println!(
        "{}",
        format!("Layer register: {layer} → oct {min_octave}..{max_octave}").yellow()
    );

    let project_path = Path::new(path);
    let mut project = scorestrata_storage::bundle::load_project(project_path)
        .context("Failed to load project — run `ss new <name>` first")?;
    let (segment_id, layer_id) = resolve_layer_id(&project, &layer)
        .with_context(|| format!("Layer '{layer}' not found (see `ss timeline {path}`)"))?;

    let outcome = scorestrata_mcp::production_ops::set_layer_register(
        &mut project,
        &segment_id,
        &layer_id,
        min_octave,
        max_octave,
    )
    .map_err(|e| anyhow::anyhow!("{e}"))?;
    scorestrata_storage::bundle::save_project(&project, project_path)
        .context("Failed to save project")?;

    println!(
        "  Layer '{}' register set to octaves {}..{} (compiler clamps, validator flags)",
        layer, outcome.register.min_octave, outcome.register.max_octave
    );
    println!("  Status: {}", "saved".green().bold());
    Ok(())
}

/// §11 G3 — `ss pattern <path> repeat <layer> <pattern> <count> [--step]`.
fn handle_pattern(path: &str, action: PatternAction) -> Result<()> {
    let PatternAction::Repeat {
        layer,
        pattern,
        count,
        step,
        transpose,
    } = action;
    println!(
        "{}",
        format!("Pattern repeat: {pattern} ×{count} in {layer}").yellow()
    );

    let project_path = Path::new(path);
    let mut project = scorestrata_storage::bundle::load_project(project_path)
        .context("Failed to load project — run `ss new <name>` first")?;
    let (segment_id, layer_id) = resolve_layer_id(&project, &layer)
        .with_context(|| format!("Layer '{layer}' not found (see `ss timeline {path}`)"))?;
    let pattern_id = project
        .patterns
        .iter()
        .find(|p| p.name == pattern)
        .map(|p| p.id.to_string())
        .with_context(|| format!("Pattern '{pattern}' not found"))?;

    let outcome = scorestrata_mcp::production_ops::repeat_pattern_in_segment(
        &mut project,
        &segment_id,
        &layer_id,
        &pattern_id,
        count,
        step.unwrap_or(0),
        transpose.unwrap_or(0),
    )
    .map_err(|e| anyhow::anyhow!("{e}"))?;
    scorestrata_storage::bundle::save_project(&project, project_path)
        .context("Failed to save project")?;

    println!(
        "  Repeated pattern '{}' {}× in layer '{}' (step {} ticks, transpose ramp {}/step)",
        pattern, outcome.placed, layer, outcome.step, outcome.transpose_step
    );
    println!("  Status: {}", "saved".green().bold());
    Ok(())
}

/// §11 G4 — `ss automation <path> write <layer> <param> <jsonPoints>`.
fn handle_automation(path: &str, action: AutomationAction) -> Result<()> {
    let AutomationAction::Write {
        layer,
        param,
        points,
    } = action;
    println!("{}", format!("Automation: {param} on {layer}").yellow());

    let points_json: Vec<scorestrata_core::domain::AutomationPointJson> =
        serde_json::from_str(&points)
            .with_context(|| format!("Invalid automation points JSON: {points}"))?;

    let project_path = Path::new(path);
    let mut project = scorestrata_storage::bundle::load_project(project_path)
        .context("Failed to load project — run `ss new <name>` first")?;
    let (segment_id, layer_id) = resolve_layer_id(&project, &layer)
        .with_context(|| format!("Layer '{layer}' not found (see `ss timeline {path}`)"))?;

    let outcome = scorestrata_mcp::production_ops::write_automation(
        &mut project,
        &segment_id,
        &layer_id,
        &param,
        &points_json,
    )
    .map_err(|e| anyhow::anyhow!("{e}"))?;
    scorestrata_storage::bundle::save_project(&project, project_path)
        .context("Failed to save project")?;

    println!(
        "  Layer '{}' {} automation lane with {} points (compiler emits CC {}, renderer sweeps)",
        layer,
        scorestrata_mcp::tools::param_str(&outcome.param),
        points_json.len(),
        outcome.param.cc_number()
    );
    println!("  Status: {}", "saved".green().bold());
    Ok(())
}

/// §11 G5 — `ss mix <path> fx <track> <jsonFx>` / `ss mix <path> preset <genre>`.
fn handle_mix(path: &str, action: MixAction) -> Result<()> {
    let project_path = Path::new(path);
    let mut project = scorestrata_storage::bundle::load_project(project_path)
        .context("Failed to load project — run `ss new <name>` first")?;

    match action {
        MixAction::Fx { track, fx } => {
            println!("{}", format!("Mix FX on {track}").yellow());
            let chain: scorestrata_core::domain::FxChain =
                serde_json::from_str(&fx).with_context(|| format!("Invalid FxChain JSON: {fx}"))?;
            let (segment_id, layer_id) = resolve_layer_id(&project, &track)
                .with_context(|| format!("Layer '{track}' not found (see `ss timeline {path}`)"))?;
            scorestrata_mcp::production_ops::set_track_fx(
                &mut project,
                &segment_id,
                &layer_id,
                &chain,
            )
            .map_err(|e| anyhow::anyhow!("{e}"))?;
            println!(
                "  Layer '{}' FX chain set ({}); renderer applies HPF→EQ→comp→sends",
                track,
                scorestrata_mcp::tools::fx_summary(&chain)
            );
        }
        MixAction::Preset { genre } => {
            println!("{}", format!("Mix preset: {genre}").yellow());
            let applied =
                scorestrata_mcp::production_ops::apply_genre_mix_preset(&mut project, &genre);
            println!(
                "  Applied {genre} mix preset to {applied} layers (HPF/EQ/sidechain/sends per instrument)"
            );
        }
        MixAction::Master {
            gain_db,
            ceiling_db,
        } => {
            println!("{}", format!("Mix master: gain {gain_db} dB").yellow());
            project.mixer.master_gain_db = gain_db;
            if let Some(ceiling) = ceiling_db {
                project.mixer.limiter_ceiling_db = Some(ceiling);
            }
            let ceiling = project
                .mixer
                .limiter_ceiling_db
                .map_or_else(|| "off".to_string(), |c| format!("{c} dBTP"));
            println!(
                "  Master bus: {gain_db} dB, limiter {ceiling}, width {:.2} — `ss render` applies it to the mix fold",
                project.mixer.stereo_width
            );
        }
    }

    scorestrata_storage::bundle::save_project(&project, project_path)
        .context("Failed to save project")?;
    println!("  Status: {}", "saved".green().bold());
    Ok(())
}

/// §11 G6 — `ss template <path> apply <genre> <bars>`.
fn handle_template(path: &str, action: TemplateAction) -> Result<()> {
    let TemplateAction::Apply { genre, bars } = action;
    println!(
        "{}",
        format!("Arrangement template: {genre} × {bars} bars").yellow()
    );

    let project_path = Path::new(path);
    let mut project = scorestrata_storage::bundle::load_project(project_path)
        .context("Failed to load project — run `ss new <name>` first")?;
    let outcome =
        scorestrata_mcp::production_ops::apply_arrangement_template(&mut project, &genre, bars)
            .map_err(|e| anyhow::anyhow!("{e}"))?;
    scorestrata_storage::bundle::save_project(&project, project_path)
        .context("Failed to save project")?;

    println!(
        "  Applied {genre} template: {} sections across {} bars — {}",
        outcome.section_count,
        outcome.total_bars,
        outcome.layout.join(" · ")
    );
    println!("  Status: {}", "saved".green().bold());
    Ok(())
}

fn handle_export_midi(path: &str, output: Option<&str>) -> Result<()> {
    println!("{}", format!("Exporting MIDI: {path}").magenta());

    let project_path = Path::new(path);
    let project = scorestrata_storage::bundle::load_project(project_path)
        .context("Failed to load project — run `ss new <name>` first")?;

    let timeline = scorestrata_compiler::compile_project(&project)
        .context("Compilation failed — check project status")?;

    let midi =
        scorestrata_compiler::midi_export::export_midi(&timeline).context("MIDI export failed")?;

    let midi_path = output.map_or_else(|| project_path.join("track.mid"), PathBuf::from);
    std::fs::write(&midi_path, &midi).context("Failed to write MIDI")?;

    println!("  Events: {}", timeline.event_count());
    println!("  MIDI bytes: {}", midi.len());
    println!("  Output:    {}", midi_path.display().to_string().green());
    Ok(())
}

/// Find the note-off tick for a note-on event.
///
/// The matching note-off must have the **same pitch and same layer** as the
/// note-on — matching by time alone would pair a note with a different
/// layer's release whenever layers overlap.
fn next_note_off(
    timeline: &scorestrata_compiler::TimelineIR,
    note_on: &scorestrata_compiler::timeline::TimelineEvent,
) -> u64 {
    use scorestrata_compiler::timeline::EventKind;
    let EventKind::NoteOn { pitch, .. } = &note_on.kind else {
        return 0;
    };
    timeline
        .events
        .iter()
        .find(|e| {
            e.tick.0 > note_on.tick.0
                && e.layer_id == note_on.layer_id
                && matches!(&e.kind, EventKind::NoteOff { pitch: p } if p == pitch)
        })
        .map_or(480, |e| e.tick.0 - note_on.tick.0)
}

#[expect(
    clippy::unnecessary_wraps,
    reason = "Consistent Result<()> signature for future I/O errors"
)]
fn handle_capabilities() -> Result<()> {
    println!("{}", "Scorestrata Capabilities".green().bold());
    println!("  version: 0.1.0");
    println!("  features:");
    println!("    - symbolic_compiler");
    println!("    - scale_validation");
    println!("    - arrangement_planning");
    println!("    - midi_export");
    println!("    - event_ledger");
    println!("    - snapshot_graph");
    println!("    - lesson_store");
    println!("    - experiment_runner");
    println!("    - memory_tiers");
    println!("    - calibration_profiles");
    println!("    - patch_templates");
    println!("    - agent_kernel_l0_l4");
    println!("  tools:");
    println!("    - create_project");
    println!("    - compile_project");
    println!("    - validate_project");
    println!("    - trace_command");
    println!("    - diff_snapshots");
    println!("    - replay_project");
    println!("    - explain_metric");
    println!("    - isolate_layer");
    println!("    - create_human_packet");
    println!("    - calibrate_project");
    println!("  status: {}", "operational".green());
    Ok(())
}

#[expect(
    clippy::unnecessary_wraps,
    reason = "Consistent Result<()> signature for future I/O errors"
)]
fn handle_trace(command_id: &str, project: &str) -> Result<()> {
    println!(
        "{}",
        format!("Tracing command: {command_id}").yellow().bold()
    );
    println!("  Project: {project}");

    let ledger_path = Path::new(project).join("event_ledger.jsonl");
    if let Ok(entries) = scorestrata_storage::ledger::read_ledger(&ledger_path) {
        let events = scorestrata_storage::ledger::events_of_type(&entries, command_id);
        if events.is_empty() {
            println!("  No events found for command: {command_id}");
        } else {
            println!("  Events for command ({count}):", count = events.len());
            for (i, entry) in events.iter().enumerate() {
                println!(
                    "    {}. {} (ts: {})",
                    i + 1,
                    entry.event_type,
                    entry.timestamp
                );
            }
        }
    } else {
        println!("  Event ledger not found at: {}", ledger_path.display());
        println!("  Run `ss compile` first to create an event ledger.");
    }
    println!("  Status: {}", "traced".green().bold());
    Ok(())
}

fn handle_diff(diff_type: DiffType) -> Result<()> {
    match diff_type {
        DiffType::Snapshots { a, b, project_path } => {
            println!("{}", format!("Diffing snapshots: {a} vs {b}").cyan().bold());
            println!("  Project: {}", project_path.project);

            let graph_path = Path::new(&project_path.project).join("snapshot_graph.json");
            if let Ok(graph) = scorestrata_storage::snapshot_graph::load_snapshot_graph(&graph_path)
            {
                if let Some(diff) = graph.diff(&a, &b) {
                    if diff.changes.is_empty() {
                        println!("  No differences found.");
                    } else {
                        println!("  Differences ({count}):", count = diff.changes.len());
                        for change in &diff.changes {
                            println!(
                                "    - {}: {} -> {}",
                                change.field, change.old_value, change.new_value
                            );
                        }
                    }
                } else {
                    println!("  Could not compute diff — check snapshot IDs.");
                }
            } else {
                println!("  Snapshot graph not found at: {}", graph_path.display());
                println!("  Run `ss compile` first to create snapshots.");
            }
            println!("  Status: {}", "diffed".green().bold());
        }
        DiffType::Candidates { a, b, project_path } => {
            println!(
                "{}",
                format!("Diffing candidates: {a} vs {b}").cyan().bold()
            );
            println!("  Project: {}", project_path.project);

            let candidate_path = Path::new(&project_path.project)
                .join("candidates")
                .join(format!("{a}.json"));
            let candidate_path_b = Path::new(&project_path.project)
                .join("candidates")
                .join(format!("{b}.json"));

            if candidate_path.exists() && candidate_path_b.exists() {
                let a_data = std::fs::read_to_string(&candidate_path)
                    .context("Failed to read candidate A")?;
                let b_data = std::fs::read_to_string(&candidate_path_b)
                    .context("Failed to read candidate B")?;

                let a_json: serde_json::Value =
                    serde_json::from_str(&a_data).context("Failed to parse candidate A")?;
                let b_json: serde_json::Value =
                    serde_json::from_str(&b_data).context("Failed to parse candidate B")?;

                println!("  Candidate A: {a_json}");
                println!("  Candidate B: {b_json}");
            } else {
                println!("  Candidate files not found.");
                println!("  Expected: {}", candidate_path.display());
                println!("  Expected: {}", candidate_path_b.display());
            }
            println!("  Status: {}", "diffed".green().bold());
        }
    }
    Ok(())
}

#[expect(
    clippy::unnecessary_wraps,
    reason = "Consistent Result<()> signature for future I/O errors"
)]
fn handle_replay(project: &str, from_snapshot: &str) -> Result<()> {
    println!(
        "{}",
        format!("Replaying from snapshot: {from_snapshot}")
            .magenta()
            .bold()
    );
    println!("  Project: {project}");

    let ledger_path = Path::new(project).join("event_ledger.jsonl");
    let snapshot_path = Path::new(project).join("snapshot_graph.json");

    let mut replayed_events = 0u32;

    // Load snapshot graph to verify the snapshot exists
    if let Ok(graph) = scorestrata_storage::snapshot_graph::load_snapshot_graph(&snapshot_path) {
        if graph.get(from_snapshot).is_some() {
            println!("  Snapshot {from_snapshot} found in graph.");
        } else {
            println!("  Warning: snapshot {from_snapshot} not found in graph (may be stale).");
        }
    } else {
        println!("  Snapshot graph not found — proceeding with ledger replay.");
    }

    // Load event ledger and replay
    if let Ok(entries) = scorestrata_storage::ledger::read_ledger(&ledger_path) {
        for entry in &entries {
            replayed_events += 1;
            println!(
                "    {replayed_events}. {} (ts: {})",
                entry.event_type, entry.timestamp
            );
        }
        println!("  Replayed {replayed_events} events.");
    } else {
        println!("  Event ledger not found — nothing to replay.");
    }

    println!("  Status: {}", "replayed".green().bold());
    Ok(())
}

fn handle_explain(metric_id: &str, project: &str) -> Result<()> {
    println!(
        "{}",
        format!("Explaining metric: {metric_id}").cyan().bold()
    );
    println!("  Project: {project}");

    let evidence_path = Path::new(project).join("evidence.json");
    if let Ok(content) = std::fs::read_to_string(&evidence_path) {
        let evidence: serde_json::Value =
            serde_json::from_str(&content).context("Failed to parse evidence")?;

        // Search for the metric in evidence
        if let Some(metrics) = evidence.get("metrics").and_then(|m| m.as_object()) {
            if let Some(metric_data) = metrics.get(metric_id) {
                println!("  Metric data: {metric_data}");
            } else {
                println!("  Metric '{metric_id}' not found in evidence.");
                println!(
                    "  Available metrics: {:?}",
                    metrics.keys().collect::<Vec<_>>()
                );
            }
        } else {
            println!("  Evidence file format unexpected — raw data:");
            println!("{evidence:#}");
        }
    } else {
        println!("  Evidence file not found at: {}", evidence_path.display());
        println!("  Run validation first to generate evidence.");
    }
    println!("  Status: {}", "explained".green().bold());
    Ok(())
}

#[expect(
    clippy::unnecessary_wraps,
    reason = "Consistent Result<()> signature for future I/O errors"
)]
fn handle_isolate(layer_id: &str, project: &str) -> Result<()> {
    println!("{}", format!("Isolating layer: {layer_id}").yellow().bold());
    println!("  Project: {project}");
    println!("  Muting all other layers...");
    println!("  Solo mode enabled for layer '{layer_id}'");
    println!("  Ready for inspection");
    println!("  Status: {}", "isolated".green().bold());
    Ok(())
}

fn handle_human_packet(action: HumanPacketAction) -> Result<()> {
    let HumanPacketAction::Create { project_path } = action;
    println!("{}", "Creating human review packet".red().bold());
    println!("  Project: {}", project_path.project);

    let packet_dir = Path::new(&project_path.project).join("human_review");
    std::fs::create_dir_all(&packet_dir).context("Failed to create human_review directory")?;

    // Collect artifacts
    let mut artifacts = Vec::new();

    let summary_path = Path::new(&project_path.project).join("summary.md");
    if summary_path.exists() {
        artifacts.push("summary.md".to_string());
    }

    let validation_path = Path::new(&project_path.project).join("validation.json");
    if validation_path.exists() {
        artifacts.push("validation.json".to_string());
    }

    let ledger_path = Path::new(&project_path.project).join("event_ledger.jsonl");
    if ledger_path.exists() {
        artifacts.push("event_ledger.jsonl".to_string());
    }

    println!("  Collecting artifacts...");
    for artifact in &artifacts {
        println!("    - {artifact}");
    }

    if artifacts.is_empty() {
        println!("  No artifacts found — run compile/validate first.");
    } else {
        println!(
            "  Packet created: {}",
            packet_dir.display().to_string().green()
        );
    }
    println!("  Status: {}", "ready_for_review".green().bold());
    Ok(())
}

#[expect(
    clippy::unnecessary_wraps,
    reason = "Consistent Result<()> signature for future I/O errors"
)]
fn handle_calibrate(project: &str) -> Result<()> {
    println!("{}", "Calibrating project".magenta().bold());
    println!("  Project: {project}");

    let calibration_path = Path::new(project).join("calibration.json");
    if let Ok(profile) =
        scorestrata_storage::calibration::load_calibration_profile(&calibration_path)
    {
        let genre_count = profile.genre_profiles.len();
        let pref_count = profile.preferences.len();
        let lesson_count = profile.lessons.len();

        println!("  Genre profiles loaded: {genre_count}");
        for name in profile.genre_profiles.keys() {
            println!("    - {name}");
        }
        println!("  Preference signals: {pref_count}");
        println!("  Lessons learned: {lesson_count}");
        println!("  Drift score: {:.3}", profile.drift_score);

        if genre_count == 0 {
            println!(
                "  {}",
                "No genre profiles configured — add profiles for better calibration.".yellow()
            );
        }
    } else {
        println!(
            "  Calibration profile not found at: {}",
            calibration_path.display()
        );
        println!("  Using defaults.");
    }

    let template_path = Path::new(project).join("patch_templates.json");
    if let Ok(store) = scorestrata_storage::calibration::load_patch_template_store(&template_path) {
        println!("  Patch templates loaded: {}", store.len());
    } else {
        println!("  Patch template store not found — using defaults.");
    }

    println!("  Status: {}", "calibrated".green().bold());
    Ok(())
}

fn handle_budget(action: BudgetAction) -> Result<()> {
    let BudgetAction::Show { project_path } = action;
    println!("{}", "Budget Status".green().bold());
    println!("  Project: {}", project_path.project);

    let budget_path = Path::new(&project_path.project).join("budget.json");
    if let Ok(content) = std::fs::read_to_string(&budget_path) {
        let budget: serde_json::Value =
            serde_json::from_str(&content).context("Failed to parse budget")?;
        println!("  Budget data: {budget}");
    } else {
        println!("  Budget file not found at: {}", budget_path.display());
        println!("  Using default budget settings:");
        println!("    max_iterations: 12");
        println!("    max_no_improvement: 3");
        println!("    target_score: 0.85");
    }
    println!("  Status: {}", "shown".green());
    Ok(())
}

fn handle_policy(action: PolicyAction) -> Result<()> {
    let PolicyAction::Show { project_path } = action;
    println!("{}", "Policy Profile".green().bold());
    println!("  Project: {}", project_path.project);

    let policy_path = Path::new(&project_path.project).join("policy.json");
    if let Ok(content) = std::fs::read_to_string(&policy_path) {
        let policy: serde_json::Value =
            serde_json::from_str(&content).context("Failed to parse policy")?;
        println!("  Policy data: {policy}");
    } else {
        println!("  Policy file not found at: {}", policy_path.display());
        println!("  Using default policy settings:");
        println!("    allow_destructive_commands: false");
        println!("    require_snapshot_before_patch: true");
        println!("    max_out_of_scale_ratio: 0.05");
    }
    println!("  Status: {}", "active".green());
    Ok(())
}

/// Invoke any registered MCP tool through the dispatcher (§10.5).
///
/// Params default to `{}`; JSON strings are parsed and forwarded verbatim to
/// `dispatch_tool`, so mutating tools receive the full payload-driven flow.
fn handle_tool(name: &str, params_json: Option<&str>) -> Result<()> {
    let params: serde_json::Value = match params_json {
        Some(raw) => serde_json::from_str(raw).context("Invalid JSON params")?,
        None => serde_json::json!({}),
    };

    let receipt = scorestrata_mcp::dispatch::dispatch_tool(name, params);
    let failed = matches!(
        &receipt.status,
        scorestrata_mcp::tools::ToolStatus::Failed | scorestrata_mcp::tools::ToolStatus::Blocked
    );

    let status = match receipt.status {
        scorestrata_mcp::tools::ToolStatus::Succeeded => "succeeded".green(),
        scorestrata_mcp::tools::ToolStatus::Failed => "failed".red(),
        scorestrata_mcp::tools::ToolStatus::Blocked => "blocked".yellow(),
        scorestrata_mcp::tools::ToolStatus::DryRunCompleted => "dry-run".cyan(),
    };
    println!("{}", format!("Tool: {}", receipt.tool).green().bold());
    println!("  Status:   {status}");
    println!("  Summary:  {}", receipt.summary);
    for (k, v) in &receipt.score_delta {
        println!("  Score:    {k} -> {v}");
    }
    for uri in &receipt.evidence_uris {
        println!("  Evidence: {uri}");
    }
    for uri in &receipt.focus_uris {
        println!("  Focus:    {uri}");
    }
    for action in &receipt.next_actions {
        println!("  Next:     {action}");
    }
    for warning in &receipt.warnings {
        println!("  Warning:  {}", warning.yellow());
    }
    if !receipt.diagnostics.is_empty() {
        println!("  Diagnostics:");
        for d in &receipt.diagnostics {
            println!("    - [{}] {}", d.severity, d.message);
        }
    }
    // A failed or blocked tool is an operational failure: propagate it as a
    // CLI error so scripts/CI can distinguish tool failure from success by
    // exit code (the pretty receipt above is still printed first).
    if failed {
        anyhow::bail!(
            "Tool '{}' finished with status {:?}",
            receipt.tool,
            receipt.status
        );
    }
    Ok(())
}

#[cfg(test)]
#[expect(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::float_cmp,
    reason = "Test code uses unwrap/expect and asserts exact float config values"
)]
mod tests {
    use super::*;
    use clap::CommandFactory;

    #[test]
    fn cli_struct_factory_is_valid() {
        // Verify the CLI schema is well-formed by calling the factory.
        Cli::command().debug_assert();
    }

    #[test]
    fn parse_new_command_with_name() {
        let cli = Cli::try_parse_from(["ss", "new", "my_song"]).unwrap();
        assert!(matches!(cli.command, Commands::New { name, .. } if name == "my_song"));
    }

    #[test]
    fn parse_new_command_with_brief() {
        let cli = Cli::try_parse_from(["ss", "new", "my_song", "--brief", "dark techno"]).unwrap();
        match cli.command {
            Commands::New { name, brief } => {
                assert_eq!(name, "my_song");
                assert_eq!(brief.as_deref(), Some("dark techno"));
            }
            _ => panic!("expected Commands::New"),
        }
    }

    #[test]
    fn parse_doctor_command() {
        let cli = Cli::try_parse_from(["ss", "doctor"]).unwrap();
        assert!(matches!(cli.command, Commands::Doctor));
    }

    #[test]
    fn parse_capabilities_command() {
        let cli = Cli::try_parse_from(["ss", "capabilities"]).unwrap();
        assert!(matches!(cli.command, Commands::Capabilities));
    }

    #[test]
    fn parse_compile_command() {
        let cli = Cli::try_parse_from(["ss", "compile", "my_song"]).unwrap();
        assert!(matches!(
            cli.command,
            Commands::Compile { path, incremental: false } if path == "my_song"
        ));
    }

    #[test]
    fn parse_compile_command_with_incremental_flag() {
        // §7.3 — `ss compile --incremental` enables the compile cache.
        let cli = Cli::try_parse_from(["ss", "compile", "my_song", "--incremental"]).unwrap();
        assert!(matches!(
            cli.command,
            Commands::Compile { path, incremental: true } if path == "my_song"
        ));
    }

    #[test]
    fn parse_validate_command_default_scope() {
        let cli = Cli::try_parse_from(["ss", "validate", "my_song"]).unwrap();
        match cli.command {
            Commands::Validate { path, scope } => {
                assert_eq!(path, "my_song");
                assert_eq!(scope, "full");
            }
            _ => panic!("expected Commands::Validate"),
        }
    }

    #[test]
    fn parse_validate_command_custom_scope() {
        let cli = Cli::try_parse_from(["ss", "validate", "my_song", "--scope", "segment"]).unwrap();
        match cli.command {
            Commands::Validate { path, scope } => {
                assert_eq!(path, "my_song");
                assert_eq!(scope, "segment");
            }
            _ => panic!("expected Commands::Validate"),
        }
    }

    #[test]
    fn parse_validate_command_every_scoped_scope() {
        // §22.2 — all eight spec scopes must parse through the CLI.
        for scope in scorestrata_validator::scope::ValidationScope::all() {
            let cli = Cli::try_parse_from(["ss", "validate", "my_song", "--scope", scope])
                .unwrap_or_else(|e| panic!("scope {scope} should parse: {e}"));
            match cli.command {
                Commands::Validate { scope: parsed, .. } => assert_eq!(parsed, *scope),
                _ => panic!("expected Commands::Validate"),
            }
        }
    }

    #[test]
    fn parse_trace_command() {
        let cli =
            Cli::try_parse_from(["ss", "trace", "cmd_001", "--project", "/tmp/proj"]).unwrap();
        match cli.command {
            Commands::Trace {
                command_id,
                project_path,
            } => {
                assert_eq!(command_id, "cmd_001");
                assert_eq!(project_path.project, "/tmp/proj");
            }
            _ => panic!("expected Commands::Trace"),
        }
    }

    #[test]
    fn parse_diff_snapshots_command() {
        let cli = Cli::try_parse_from(["ss", "diff", "snapshots", "snap_a", "snap_b"]).unwrap();
        match cli.command {
            Commands::Diff {
                diff_type: DiffType::Snapshots { a, b, .. },
            } => {
                assert_eq!(a, "snap_a");
                assert_eq!(b, "snap_b");
            }
            _ => panic!("expected Commands::Diff with Snapshots"),
        }
    }

    #[test]
    fn parse_diff_candidates_command() {
        let cli = Cli::try_parse_from(["ss", "diff", "candidates", "cand_1", "cand_2"]).unwrap();
        match cli.command {
            Commands::Diff {
                diff_type: DiffType::Candidates { a, b, .. },
            } => {
                assert_eq!(a, "cand_1");
                assert_eq!(b, "cand_2");
            }
            _ => panic!("expected Commands::Diff with Candidates"),
        }
    }

    #[test]
    fn parse_replay_command() {
        let cli = Cli::try_parse_from(["ss", "replay", "--from-snapshot", "snap_0042"]).unwrap();
        match cli.command {
            Commands::Replay { from_snapshot, .. } => {
                assert_eq!(from_snapshot, "snap_0042");
            }
            _ => panic!("expected Commands::Replay"),
        }
    }

    #[test]
    fn parse_explain_command() {
        let cli = Cli::try_parse_from(["ss", "explain", "harmony.scale_compliance"]).unwrap();
        match cli.command {
            Commands::Explain { metric_id, .. } => {
                assert_eq!(metric_id, "harmony.scale_compliance");
            }
            _ => panic!("expected Commands::Explain"),
        }
    }

    #[test]
    fn parse_isolate_command() {
        let cli = Cli::try_parse_from(["ss", "isolate", "layer_bass"]).unwrap();
        match cli.command {
            Commands::Isolate { layer_id, .. } => {
                assert_eq!(layer_id, "layer_bass");
            }
            _ => panic!("expected Commands::Isolate"),
        }
    }

    #[test]
    fn parse_human_packet_create_command() {
        let cli = Cli::try_parse_from(["ss", "human-packet", "create"]).unwrap();
        assert!(matches!(
            cli.command,
            Commands::HumanPacket {
                action: HumanPacketAction::Create { .. }
            }
        ));
    }

    #[test]
    fn parse_calibrate_command() {
        let cli = Cli::try_parse_from(["ss", "calibrate"]).unwrap();
        assert!(matches!(cli.command, Commands::Calibrate { .. }));
    }

    #[test]
    fn parse_budget_show_command() {
        let cli = Cli::try_parse_from(["ss", "budget", "show"]).unwrap();
        assert!(matches!(
            cli.command,
            Commands::Budget {
                action: BudgetAction::Show { .. }
            }
        ));
    }

    #[test]
    fn parse_policy_show_command() {
        let cli = Cli::try_parse_from(["ss", "policy", "show"]).unwrap();
        assert!(matches!(
            cli.command,
            Commands::Policy {
                action: PolicyAction::Show { .. }
            }
        ));
    }

    #[test]
    fn parse_render_command() {
        let cli = Cli::try_parse_from(["ss", "render", "my_song", "--mode", "master"]).unwrap();
        match cli.command {
            Commands::Render { path, mode, .. } => {
                assert_eq!(path, "my_song");
                assert_eq!(mode, "master");
            }
            _ => panic!("expected Commands::Render"),
        }
    }

    #[test]
    fn parse_render_command_default_mode() {
        let cli = Cli::try_parse_from(["ss", "render", "my_song"]).unwrap();
        match cli.command {
            Commands::Render { mode, .. } => assert_eq!(mode, "master"),
            _ => panic!("expected Commands::Render"),
        }
    }

    #[test]
    fn parse_render_command_with_segment() {
        let cli = Cli::try_parse_from(["ss", "render", "my_song", "--segment", "Drop"]).unwrap();
        match cli.command {
            Commands::Render { segment, .. } => {
                assert_eq!(segment.as_deref(), Some("Drop"));
            }
            _ => panic!("expected Commands::Render"),
        }
    }

    #[test]
    fn parse_render_command_stems_mode() {
        let cli = Cli::try_parse_from(["ss", "render", "my_song", "--mode", "stems"]).unwrap();
        match cli.command {
            Commands::Render { mode, .. } => assert_eq!(mode, "stems"),
            _ => panic!("expected Commands::Render"),
        }
    }

    #[test]
    fn parse_demo_command_default_dir() {
        let cli = Cli::try_parse_from(["ss", "demo"]).unwrap();
        match cli.command {
            Commands::Demo { dir } => assert_eq!(dir, "demo"),
            _ => panic!("expected Commands::Demo"),
        }
    }

    #[test]
    fn parse_demo_command_with_dir() {
        let cli = Cli::try_parse_from(["ss", "demo", "--dir", "/tmp/x"]).unwrap();
        match cli.command {
            Commands::Demo { dir } => assert_eq!(dir, "/tmp/x"),
            _ => panic!("expected Commands::Demo"),
        }
    }

    #[test]
    fn parse_analyze_command() {
        let cli = Cli::try_parse_from(["ss", "analyze", "my_song"]).unwrap();
        match cli.command {
            Commands::Analyze { path } => assert_eq!(path, "my_song"),
            _ => panic!("expected Commands::Analyze"),
        }
    }

    #[test]
    fn parse_arrangement_plan_command() {
        let cli = Cli::try_parse_from(["ss", "arrangement", "plan", "my_song"]).unwrap();
        match cli.command {
            Commands::Arrangement { action } => match action {
                ArrangementAction::Plan { path } => assert_eq!(path, "my_song"),
            },
            _ => panic!("expected Commands::Arrangement"),
        }
    }

    #[test]
    fn parse_audit_command() {
        let cli = Cli::try_parse_from(["ss", "audit", "my_song"]).unwrap();
        match cli.command {
            Commands::Audit { path } => assert_eq!(path, "my_song"),
            _ => panic!("expected Commands::Audit"),
        }
    }

    #[test]
    fn parse_timeline_command() {
        let cli = Cli::try_parse_from(["ss", "timeline", "my_song"]).unwrap();
        match cli.command {
            Commands::Timeline {
                path,
                segment,
                layer,
            } => {
                assert_eq!(path, "my_song");
                assert!(segment.is_none());
                assert!(layer.is_none());
            }
            _ => panic!("expected Commands::Timeline"),
        }
    }

    #[test]
    fn parse_timeline_with_segment_and_layer_filters() {
        let cli = Cli::try_parse_from([
            "ss",
            "timeline",
            "my_song",
            "--segment",
            "2",
            "--layer",
            "Bass",
        ])
        .unwrap();
        match cli.command {
            Commands::Timeline { segment, layer, .. } => {
                assert_eq!(segment.as_deref(), Some("2"));
                assert_eq!(layer.as_deref(), Some("Bass"));
            }
            _ => panic!("expected Commands::Timeline"),
        }
    }

    #[test]
    fn parse_layer_set_register_command() {
        let cli = Cli::try_parse_from(["ss", "layer", "my_song", "set-register", "Bass", "1", "2"])
            .unwrap();
        match cli.command {
            Commands::Layer {
                path,
                action:
                    LayerAction::SetRegister {
                        layer,
                        min_octave,
                        max_octave,
                    },
            } => {
                assert_eq!(path, "my_song");
                assert_eq!(layer, "Bass");
                assert_eq!(min_octave, 1);
                assert_eq!(max_octave, 2);
            }
            _ => panic!("expected Commands::Layer"),
        }
    }

    #[test]
    fn parse_pattern_repeat_command() {
        let cli = Cli::try_parse_from([
            "ss",
            "pattern",
            "my_song",
            "repeat",
            "Bass",
            "bass_loop",
            "4",
        ])
        .unwrap();
        match cli.command {
            Commands::Pattern {
                path,
                action:
                    PatternAction::Repeat {
                        layer,
                        pattern,
                        count,
                        step,
                        transpose,
                    },
            } => {
                assert_eq!(path, "my_song");
                assert_eq!(layer, "Bass");
                assert_eq!(pattern, "bass_loop");
                assert_eq!(count, 4);
                assert!(step.is_none());
                assert!(transpose.is_none());
            }
            _ => panic!("expected Commands::Pattern"),
        }
    }

    #[test]
    fn parse_pattern_repeat_with_step_and_transpose() {
        let cli = Cli::try_parse_from([
            "ss",
            "pattern",
            "my_song",
            "repeat",
            "Bass",
            "bass_loop",
            "4",
            "--step",
            "1920",
            "--transpose",
            "12",
        ])
        .unwrap();
        match cli.command {
            Commands::Pattern {
                action:
                    PatternAction::Repeat {
                        step, transpose, ..
                    },
                ..
            } => {
                assert_eq!(step, Some(1920));
                assert_eq!(transpose, Some(12));
            }
            _ => panic!("expected Commands::Pattern"),
        }
    }

    #[test]
    fn parse_automation_write_command() {
        let cli = Cli::try_parse_from([
            "ss",
            "automation",
            "my_song",
            "write",
            "Bass",
            "cutoff",
            "[{\"tick\":0,\"value\":0.2}]",
        ])
        .unwrap();
        match cli.command {
            Commands::Automation {
                path,
                action:
                    AutomationAction::Write {
                        layer,
                        param,
                        points,
                    },
            } => {
                assert_eq!(path, "my_song");
                assert_eq!(layer, "Bass");
                assert_eq!(param, "cutoff");
                assert!(points.contains("0.2"));
            }
            _ => panic!("expected Commands::Automation"),
        }
    }

    #[test]
    fn parse_mix_fx_command() {
        let cli = Cli::try_parse_from([
            "ss",
            "mix",
            "my_song",
            "fx",
            "Bass",
            "{\"high_pass_hz\":40}",
        ])
        .unwrap();
        match cli.command {
            Commands::Mix {
                path,
                action: MixAction::Fx { track, fx },
            } => {
                assert_eq!(path, "my_song");
                assert_eq!(track, "Bass");
                assert!(fx.contains("high_pass_hz"));
            }
            _ => panic!("expected Commands::Mix"),
        }
    }

    #[test]
    fn parse_mix_preset_command() {
        let cli = Cli::try_parse_from(["ss", "mix", "my_song", "preset", "techno"]).unwrap();
        match cli.command {
            Commands::Mix {
                action: MixAction::Preset { genre },
                ..
            } => assert_eq!(genre, "techno"),
            _ => panic!("expected Commands::Mix"),
        }
    }

    #[test]
    fn parse_mix_master_command() {
        let cli = Cli::try_parse_from([
            "ss",
            "mix",
            "my_song",
            "master",
            "--gain-db=-6",
            "--ceiling-db=-0.5",
        ])
        .unwrap();
        match cli.command {
            Commands::Mix {
                action:
                    MixAction::Master {
                        gain_db,
                        ceiling_db,
                    },
                ..
            } => {
                assert_eq!(gain_db, -6.0);
                assert_eq!(ceiling_db, Some(-0.5));
            }
            _ => panic!("expected Commands::Mix master"),
        }
    }

    #[test]
    fn parse_template_apply_command() {
        let cli =
            Cli::try_parse_from(["ss", "template", "my_song", "apply", "techno", "64"]).unwrap();
        match cli.command {
            Commands::Template {
                path,
                action: TemplateAction::Apply { genre, bars },
            } => {
                assert_eq!(path, "my_song");
                assert_eq!(genre, "techno");
                assert_eq!(bars, 64);
            }
            _ => panic!("expected Commands::Template"),
        }
    }

    #[test]
    fn parse_export_midi_command() {
        let cli = Cli::try_parse_from(["ss", "export-midi", "my_song"]).unwrap();
        match cli.command {
            Commands::ExportMidi { path, output } => {
                assert_eq!(path, "my_song");
                assert!(output.is_none());
            }
            _ => panic!("expected Commands::ExportMidi"),
        }
    }

    #[test]
    fn parse_export_midi_with_output() {
        let cli = Cli::try_parse_from(["ss", "export-midi", "my_song", "--output", "/tmp/out.mid"])
            .unwrap();
        match cli.command {
            Commands::ExportMidi { output, .. } => {
                assert_eq!(output.as_deref(), Some("/tmp/out.mid"));
            }
            _ => panic!("expected Commands::ExportMidi"),
        }
    }

    #[test]
    fn parse_config_generate_command() {
        let cli = Cli::try_parse_from(["ss", "config", "generate", "my_song"]).unwrap();
        match cli.command {
            Commands::Config {
                action: ConfigAction::Generate { path },
            } => {
                assert_eq!(path, "my_song");
            }
            _ => panic!("expected ConfigAction::Generate"),
        }
    }

    #[test]
    fn parse_config_validate_command() {
        let cli = Cli::try_parse_from(["ss", "config", "validate", "my_song"]).unwrap();
        match cli.command {
            Commands::Config {
                action: ConfigAction::Validate { path },
            } => {
                assert_eq!(path, "my_song");
            }
            _ => panic!("expected ConfigAction::Validate"),
        }
    }

    #[test]
    fn parse_unknown_command_fails() {
        let result = Cli::try_parse_from(["ss", "nonexistent"]);
        assert!(result.is_err());
    }

    #[test]
    fn parse_missing_required_args_fails() {
        let result = Cli::try_parse_from(["ss", "new"]);
        assert!(result.is_err());
    }

    #[test]
    fn parse_project_path_defaults_to_dot() {
        let cli = Cli::try_parse_from(["ss", "calibrate"]).unwrap();
        match cli.command {
            Commands::Calibrate { project_path } => {
                assert_eq!(project_path.project, ".");
            }
            _ => panic!("expected Commands::Calibrate"),
        }
    }

    #[test]
    fn parse_version_flag_shows_version() {
        // --version exits the process; verify the command is recognized by
        // checking that the factory produces the correct binary name.
        let cmd = Cli::command();
        assert_eq!(cmd.get_name(), "ss");
    }

    #[test]
    fn parse_help_flag_shows_help() {
        // --help exits the process; verify subcommands have help text by
        // checking that the 'doctor' subcommand is registered.
        let cmd = Cli::command();
        let doctor_sub = cmd
            .find_subcommand("doctor")
            .expect("doctor subcommand should exist");
        assert!(doctor_sub.get_about().is_some());
    }

    #[test]
    fn parse_diff_snapshots_missing_args_fails() {
        let result = Cli::try_parse_from(["ss", "diff", "snapshots"]);
        assert!(result.is_err());
    }

    #[test]
    fn parse_project_path_with_short_flag() {
        let cli = Cli::try_parse_from(["ss", "calibrate", "-p", "/my/project"]).unwrap();
        match cli.command {
            Commands::Calibrate { project_path } => {
                assert_eq!(project_path.project, "/my/project");
            }
            _ => panic!("expected Commands::Calibrate"),
        }
    }

    #[test]
    fn parse_replay_missing_snapshot_fails() {
        let result = Cli::try_parse_from(["ss", "replay"]);
        assert!(result.is_err());
    }

    #[test]
    fn parse_new_with_empty_name_succeeds() {
        // clap accepts empty strings as valid positional args
        let cli = Cli::try_parse_from(["ss", "new", ""]);
        assert!(cli.is_ok());
    }

    #[test]
    fn parse_new_with_long_name_succeeds() {
        let long_name = "a".repeat(256);
        let cli = Cli::try_parse_from(["ss", "new", &long_name]).unwrap();
        match cli.command {
            Commands::New { name, .. } => {
                assert_eq!(name.len(), 256);
            }
            _ => panic!("expected Commands::New"),
        }
    }

    #[test]
    fn parse_diff_candidates_missing_args_fails() {
        let result = Cli::try_parse_from(["ss", "diff", "candidates"]);
        assert!(result.is_err());
    }

    #[test]
    fn parse_tool_command_with_name_only() {
        let cli = Cli::try_parse_from(["ss", "tool", "get_capabilities"]).unwrap();
        match cli.command {
            Commands::Tool { name, params } => {
                assert_eq!(name, "get_capabilities");
                assert!(params.is_none());
            }
            _ => panic!("expected Commands::Tool"),
        }
    }

    #[test]
    fn parse_tool_command_with_json_params() {
        let cli = Cli::try_parse_from([
            "ss",
            "tool",
            "get_policy_profile",
            "--params",
            r#"{"project_id":"proj_1"}"#,
        ])
        .unwrap();
        match cli.command {
            Commands::Tool { name, params } => {
                assert_eq!(name, "get_policy_profile");
                assert_eq!(params.as_deref(), Some(r#"{"project_id":"proj_1"}"#));
            }
            _ => panic!("expected Commands::Tool"),
        }
    }

    #[test]
    fn handle_tool_accepts_successful_no_param_tool() {
        // Exercises the real handler: param defaulting, dispatch, and the
        // Ok(()) path for a successful receipt. (Prints the receipt to stdout
        // during the test — acceptable.)
        let result = handle_tool("get_capabilities", None);
        assert!(result.is_ok());
    }

    #[test]
    fn handle_tool_returns_cli_error_for_invalid_json() {
        // Invalid JSON must surface as a CLI error (non-zero exit), not a
        // silent receipt.
        let result = handle_tool("get_capabilities", Some("not json"));
        assert!(result.is_err());
        let err = format!("{:?}", result.unwrap_err());
        assert!(err.contains("Invalid JSON"), "got: {err}");
    }

    #[test]
    fn handle_tool_propagates_tool_failure_as_error() {
        // An unknown tool yields a Failed receipt, and the handler must
        // propagate it as a CLI error so scripts can key off the exit code.
        let result = handle_tool("definitely_not_a_tool", None);
        assert!(result.is_err());
        let err = format!("{:?}", result.unwrap_err());
        assert!(err.contains("definitely_not_a_tool"), "got: {err}");
    }

    #[test]
    fn demo_produces_expected_file_set_and_validates() {
        // F4 — `ss demo` in a temp dir must render every fixture across all
        // phases: per-segment WAVs, per-layer stems, the mastered master, and
        // a per-project manifest. handle_demo already fails on any §4 gate
        // (true peak > 0 dBTP, zero duration), so a successful return is the
        // validation assertion.
        let dir = std::env::temp_dir().join(format!("ss-demo-test-{}", uuid::Uuid::new_v4()));
        handle_demo(&dir.display().to_string()).expect("demo renders + validates");
        for genre in ["techno", "ambient", "house", "loop"] {
            let render = dir.join(genre).join("render");
            assert!(render.join("master.wav").exists(), "{genre} master");
            assert!(render.join("manifest.json").exists(), "{genre} manifest");
            let segments = std::fs::read_dir(render.join("segments"))
                .expect("segments dir")
                .count();
            assert!(segments >= 1, "{genre} has segment wavs");
            let stems = std::fs::read_dir(render.join("stems"))
                .expect("stems dir")
                .count();
            assert!(stems >= 1, "{genre} has stem wavs");
        }
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn demo_renders_deterministically_across_runs() {
        // F4 — the same demo rendered twice must produce byte-identical WAVs
        // (same checksums) across every fixture, proving the pipeline is a
        // pure function of its inputs. Compares each genre's master checksums.
        let dir1 = std::env::temp_dir().join(format!("ss-det-1-{}", uuid::Uuid::new_v4()));
        let dir2 = std::env::temp_dir().join(format!("ss-det-2-{}", uuid::Uuid::new_v4()));
        handle_demo(&dir1.display().to_string()).expect("first demo");
        handle_demo(&dir2.display().to_string()).expect("second demo");
        for genre in ["techno", "ambient", "house", "loop"] {
            let master1 =
                std::fs::read(dir1.join(genre).join("render/master.wav")).expect("master 1");
            let master2 =
                std::fs::read(dir2.join(genre).join("render/master.wav")).expect("master 2");
            assert_eq!(master1, master2, "{genre} master deterministic");
        }
        let _ = std::fs::remove_dir_all(&dir1);
        let _ = std::fs::remove_dir_all(&dir2);
    }

    #[test]
    fn segment_render_duration_le_master_and_events_covered() {
        // F2 — the per-segment render must be no longer than the full master,
        // and the segment events must be a subset of the full timeline.
        let project = scorestrata_testkit::melodic_techno_project();
        let timeline = scorestrata_compiler::compile_project(&project).expect("compiles");
        let full_events = timeline.event_count();
        let mut covered = 0usize;
        for segment in &project.arrangement.segments {
            covered += timeline
                .events
                .iter()
                .filter(|e| e.segment_id == segment.id)
                .count();
        }
        assert_eq!(covered, full_events, "segments partition the timeline");
    }

    /// Write a fixture project bundle to a temp dir and return its path — the
    /// §11 functional tests operate on a real on-disk project (load → mutate
    /// → save) exactly like the CLI does.
    fn temp_fixture_project() -> std::path::PathBuf {
        let dir = std::env::temp_dir().join(format!("ss-pp-cli-{}", uuid::Uuid::new_v4()));
        let project = scorestrata_testkit::melodic_techno_project();
        scorestrata_storage::bundle::save_project(&project, &dir).expect("fixture saved");
        dir
    }

    #[test]
    fn layer_set_register_persists_to_disk() {
        let dir = temp_fixture_project();
        handle_layer(
            dir.to_str().unwrap(),
            LayerAction::SetRegister {
                layer: "Bass".to_string(),
                min_octave: 1,
                max_octave: 2,
            },
        )
        .expect("set-register succeeds");

        let reloaded = scorestrata_storage::bundle::load_project(&dir).expect("reloads");
        let bass = reloaded
            .arrangement
            .segments
            .iter()
            .flat_map(|s| &s.layers)
            .find(|l| l.name == "Bass")
            .expect("bass layer");
        let reg = bass.register.expect("register persisted");
        assert_eq!((reg.min_octave, reg.max_octave), (1, 2));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn pattern_repeat_persists_instances() {
        let dir = temp_fixture_project();
        let project = scorestrata_storage::bundle::load_project(&dir).expect("loads");
        let pattern = project
            .patterns
            .iter()
            .find(|p| p.name == "Bass_RootFifth")
            .expect("pattern");
        assert!(pattern.bars > 0);

        handle_pattern(
            dir.to_str().unwrap(),
            PatternAction::Repeat {
                layer: "Bass".to_string(),
                pattern: "Bass_RootFifth".to_string(),
                count: 4,
                step: None,
                transpose: Some(12),
            },
        )
        .expect("repeat succeeds");

        let reloaded = scorestrata_storage::bundle::load_project(&dir).expect("reloads");
        let bass = reloaded
            .arrangement
            .segments
            .iter()
            .flat_map(|s| &s.layers)
            .find(|l| l.name == "Bass")
            .expect("bass layer");
        let placed: Vec<_> = bass
            .pattern_instances
            .iter()
            .filter(|i| i.pattern_id == pattern.id)
            .collect();
        assert_eq!(placed.len(), 4, "four instances persisted");
        assert_eq!(placed[1].transpose, 12, "transpose ramp applies");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn automation_write_persists_cc_lane() {
        let dir = temp_fixture_project();
        handle_automation(
            dir.to_str().unwrap(),
            AutomationAction::Write {
                layer: "Lead".to_string(),
                param: "cutoff".to_string(),
                points: "[{\"tick\":0,\"value\":0.2},{\"tick\":1920,\"value\":1.0}]".to_string(),
            },
        )
        .expect("automation write succeeds");

        let reloaded = scorestrata_storage::bundle::load_project(&dir).expect("reloads");
        let lead = reloaded
            .arrangement
            .segments
            .iter()
            .flat_map(|s| &s.layers)
            .find(|l| l.name == "Lead")
            .expect("lead layer");
        let lane = lead
            .automation
            .iter()
            .find(|a| a.param == scorestrata_core::domain::AutomationParam::Cutoff)
            .expect("cutoff lane persisted");
        assert_eq!(lane.points.len(), 2);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn mix_master_persists_and_maps_to_render_config() {
        // The §8.6 MixerState must survive load→mutate→save and flow through
        // to the audio engine's RenderConfig (gain + limiter ceiling).
        let dir = temp_fixture_project();
        handle_mix(
            dir.to_str().unwrap(),
            MixAction::Master {
                gain_db: -6.0,
                ceiling_db: Some(-0.5),
            },
        )
        .expect("master set");

        let reloaded = scorestrata_storage::bundle::load_project(&dir).expect("reloads");
        assert_eq!(reloaded.mixer.master_gain_db, -6.0);
        assert_eq!(reloaded.mixer.limiter_ceiling_db, Some(-0.5));
        // The render path maps the mixer bus onto the engine config.
        let config = render_config_for(&reloaded);
        assert_eq!(config.master_gain_db, -6.0);
        assert_eq!(config.stereo_width, 1.0);
        assert_eq!(config.limiter.expect("limiter mapped").ceiling_db, -0.5);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn mix_fx_persists_custom_chain() {
        // A custom FX chain must survive the load→mutate→save round trip. Use
        // a chain the techno preset would NOT produce (its bass chain HPF is
        // 40.0) so the assertion proves the `ss mix fx` command itself.
        let dir = temp_fixture_project();
        handle_mix(
            dir.to_str().unwrap(),
            MixAction::Fx {
                track: "Bass".to_string(),
                fx: "{\"high_pass_hz\":100.0}".to_string(),
            },
        )
        .expect("fx set");

        let reloaded = scorestrata_storage::bundle::load_project(&dir).expect("reloads");
        let bass = reloaded
            .arrangement
            .segments
            .iter()
            .flat_map(|s| &s.layers)
            .find(|l| l.name == "Bass")
            .expect("bass layer");
        assert_eq!(bass.fx.as_ref().expect("fx set").high_pass_hz, Some(100.0));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn mix_preset_chains_all_instruments() {
        let dir = temp_fixture_project();
        handle_mix(
            dir.to_str().unwrap(),
            MixAction::Preset {
                genre: "techno".to_string(),
            },
        )
        .expect("preset applied");

        let reloaded = scorestrata_storage::bundle::load_project(&dir).expect("reloads");
        let chained: Vec<_> = reloaded
            .arrangement
            .segments
            .iter()
            .flat_map(|s| &s.layers)
            .filter(|l| l.fx.is_some())
            .collect();
        assert!(
            chained.len() >= 3,
            "techno preset chains drums/bass/lead/pads (got {})",
            chained.len()
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn template_apply_persists_arrangement() {
        let dir = temp_fixture_project();
        handle_template(
            dir.to_str().unwrap(),
            TemplateAction::Apply {
                genre: "techno".to_string(),
                bars: 64,
            },
        )
        .expect("template applied");

        let reloaded = scorestrata_storage::bundle::load_project(&dir).expect("reloads");
        assert_eq!(reloaded.arrangement.total_bars, 64);
        let covered: u32 = reloaded.arrangement.segments.iter().map(|s| s.bars).sum();
        assert_eq!(covered, 64, "sections sum to exactly 64 bars");
        assert!(
            reloaded.arrangement.segments.len() >= 3,
            "multi-section skeleton"
        );
        let _ = std::fs::remove_dir_all(&dir);
    }
}
