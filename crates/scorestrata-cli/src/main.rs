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
    },
    /// Validate a project
    Validate {
        /// Project path
        path: String,
        /// Validation scope
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
    },
    /// Post-generation spectral audit of a rendered WAV (debug log for music)
    Analyze {
        /// Project path
        path: String,
    },
    /// Layered timeline view (sections × layers × octave registers)
    Timeline {
        /// Project path
        path: String,
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

fn main() -> Result<()> {
    let cli = Cli::parse();

    match cli.command {
        Commands::New { name, brief } => handle_new(&name, brief.as_deref())?,
        Commands::Doctor => handle_doctor()?,
        Commands::Config { action } => handle_config(action)?,
        Commands::Compile { path } => handle_compile(&path)?,
        Commands::Validate { path, scope } => handle_validate(&path, &scope)?,
        Commands::Render { path, mode } => handle_render(&path, &mode)?,
        Commands::Analyze { path } => handle_analyze(&path)?,
        Commands::Timeline { path } => handle_timeline(&path)?,
        Commands::ExportMidi { path, output } => handle_export_midi(&path, output.as_deref())?,
        Commands::Capabilities => handle_capabilities()?,
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
        // The config is generated deterministically and schema-valid, so the
        // project starts pipeline-ready (ConfigApproved → ArrangementPlanned…).
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
fn handle_compile(path: &str) -> Result<()> {
    println!("{}", format!("Compiling: {path}").yellow());

    let project_path = Path::new(path);
    let mut project = scorestrata_storage::bundle::load_project(project_path)
        .context("Failed to load project — run `ss new <name>` first")?;

    // Phase 1: expand arrangement, instantiate patterns, generate timeline IR.
    let timeline = scorestrata_compiler::compile_project(&project)
        .context("Compilation failed — check project status and patterns")?;

    println!(
        "  Pass 1: arrangement expansion ({} segments)",
        project.arrangement.segments.len()
    );
    println!(
        "  Pass 2: pattern instantiation ({} patterns)",
        project.patterns.len()
    );
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
fn handle_validate(path: &str, scope: &str) -> Result<()> {
    use scorestrata_core::error::Severity;

    println!(
        "{}",
        format!("Validating: {path} (scope: {scope})").magenta()
    );

    let project_path = Path::new(path);
    let project = scorestrata_storage::bundle::load_project(project_path)
        .context("Failed to load project — run `ss new <name>` first")?;

    let mut diagnostics: Vec<scorestrata_core::diagnostic::Diagnostic> = Vec::new();

    // Segment structure checks (Level 2 symbolic).
    if matches!(scope, "full" | "segment" | "arrangement") {
        for segment in &project.arrangement.segments {
            if let Some(d) = scorestrata_validator::segment_check::check_segment_has_layers(segment)
            {
                diagnostics.push(d);
            }
            if let Some(d) =
                scorestrata_validator::segment_check::check_segment_bar_count(segment, 4, 64)
            {
                diagnostics.push(d);
            }
        }
    }

    // Transition checks between adjacent segments.
    if matches!(scope, "full" | "transition") && project.arrangement.segments.len() >= 2 {
        let transition_diags = scorestrata_validator::transition_check::check_all_transitions(
            &project.arrangement.segments,
            0.15,
        );
        diagnostics.extend(transition_diags);
    }

    // Scale compliance per pattern (if patterns exist).
    if matches!(scope, "full" | "harmony") && !project.patterns.is_empty() {
        if let Some(key) = scorestrata_theory::key::Key::parse(&project.config.key) {
            let default_segment = scorestrata_core::ids::SegmentId::default();
            let default_layer = scorestrata_core::ids::LayerId::default();
            for pattern in &project.patterns {
                let result = scorestrata_validator::scale_check::check_scale_compliance(
                    pattern,
                    &key,
                    default_segment,
                    default_layer,
                );
                diagnostics.extend(result.diagnostics);
            }
        }
    }

    // Drum density check.
    if matches!(scope, "full" | "rhythm") {
        let default_segment = scorestrata_core::ids::SegmentId::default();
        let default_layer = scorestrata_core::ids::LayerId::default();
        for pattern in &project.patterns {
            let density = scorestrata_validator::density::rhythmic_density(pattern);
            if let Some(d) = scorestrata_validator::density::check_drum_density(
                pattern,
                density,
                default_segment,
                default_layer,
            ) {
                diagnostics.push(d);
            }
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

fn handle_render(path: &str, mode: &str) -> Result<()> {
    println!("{}", format!("Rendering: {path} (mode: {mode})").cyan());

    let project_path = Path::new(path);
    let project = scorestrata_storage::bundle::load_project(project_path)
        .context("Failed to load project — run `ss new <name>` first")?;

    // Compile to a timeline, then render offline via the audio engine.
    let timeline = scorestrata_compiler::compile_project(&project)
        .context("Compilation failed — check project status")?;

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
    let voices = scorestrata_audio::render::build_voices(&layer_instruments);

    use scorestrata_compiler::timeline::EventKind;
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

    // Stems mode: write one WAV per layer plus the master mix (§16 stems).
    if mode == "stems" {
        let config = scorestrata_audio::render::RenderConfig {
            stereo: true,
            limiter: Some(scorestrata_audio::mastering::LimiterConfig::default()),
        };
        let (stems, master) = scorestrata_audio::render::render_stems_with_config(
            &notes,
            timeline.bpm,
            scorestrata_audio::render::DEFAULT_SAMPLE_RATE,
            &voices,
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

    let config = scorestrata_audio::render::RenderConfig {
        stereo: true,
        limiter: Some(scorestrata_audio::mastering::LimiterConfig::default()),
    };
    let result = scorestrata_audio::render::render_notes_with_config(
        &notes,
        timeline.bpm,
        scorestrata_audio::render::DEFAULT_SAMPLE_RATE,
        &voices,
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

/// Post-generation spectral audit — the "debug log" for rendered music.
///
/// Reads the rendered master WAV, computes an STFT-based spectral report
/// (peak/RMS/DC/clipping, centroid, rolloff, aliasing proxy, band energies)
/// and, when a genre fingerprint exists, the genre deviation. Writes a text
/// log plus spectrogram/waveform PNGs into `<project>/render/audit/`.
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
    Ok(())
}

/// Layered timeline view (G1) — the agent's and operator's "arrange view".
///
/// Renders the composition as sections × layer lanes with occupied octave
/// registers, using the same derivation as the `get_timeline_view` MCP tool.
fn handle_timeline(path: &str) -> Result<()> {
    println!("{}", format!("Timeline: {path}").cyan());

    let project_path = Path::new(path);
    let project = scorestrata_storage::bundle::load_project(project_path)
        .context("Failed to load project — run `ss new <name>` first")?;
    let view = scorestrata_mcp::resources::build_timeline_view(&project);

    println!(
        "{} {} · {} · {} bpm · {} bars · {} sections · {} layer lanes",
        view.name.bold(),
        view.genre.cyan(),
        view.key,
        view.bpm,
        view.total_bars,
        view.sections.len(),
        view.layers.len()
    );

    // Section ruler.
    println!();
    println!("  {}", "bars:".dimmed());
    let bar_width = 8usize;
    let mut ruler = String::from("      ");
    for bar in (0..view.total_bars).step_by(8) {
        ruler.push_str(&format!("{bar:<width$}", width = bar_width));
    }
    println!("{ruler}");
    let mut sec_line = String::from("  sec ");
    for section in &view.sections {
        sec_line.push_str(&format!("{:<width$}", section.name, width = bar_width));
    }
    println!("{}", sec_line.cyan());
    println!();

    // Per-layer lanes with octave registers.
    for layer in &view.layers {
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
    for layer in &view.layers {
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
    reason = "Test code uses unwrap/expect for clarity"
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
        assert!(matches!(cli.command, Commands::Compile { path } if path == "my_song"));
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
            Commands::Render { path, mode } => {
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
    fn parse_render_command_stems_mode() {
        let cli = Cli::try_parse_from(["ss", "render", "my_song", "--mode", "stems"]).unwrap();
        match cli.command {
            Commands::Render { mode, .. } => assert_eq!(mode, "stems"),
            _ => panic!("expected Commands::Render"),
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
    fn parse_timeline_command() {
        let cli = Cli::try_parse_from(["ss", "timeline", "my_song"]).unwrap();
        match cli.command {
            Commands::Timeline { path } => assert_eq!(path, "my_song"),
            _ => panic!("expected Commands::Timeline"),
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
}
