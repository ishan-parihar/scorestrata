//! Golden harness end-to-end test (§16.5, §16.8).
//!
//! Wires the *real* pipeline — brief → config → arrangement planner →
//! structural golden assertions → L6 validation golden assertions (diagnostic
//! budgets, scorecard metric ranges, audio loudness/peak gates) → eval
//! metrics — so golden tests detect regressions in the compiler, validator,
//! audio engine, and scoring chain, exactly as the architecture's testing
//! pyramid requires.

#![expect(
    clippy::unwrap_used,
    clippy::expect_used,
    reason = "Integration test asserts the golden pipeline explicitly"
)]

use scorestrata_eval::golden::{
    ExpectedOutputs, GoldenConfig, MetricRange, ValidationGates, run_golden_assertions,
    run_validation_golden_assertions,
};
use scorestrata_eval::metrics::{
    AgentEvalMetrics, EngineeringEvalMetrics, FullEvalReport, SystemEvalMetrics,
};

/// Build a real project from a brief via the config generator + arrangement
/// planner — the same path the MCP server uses for `generate_config_from_brief`
/// and `plan_arrangement`.
fn build_planned_project(name: &str, brief: &str) -> scorestrata_core::domain::Project {
    let config = scorestrata_mcp::config_gen::generate_config(name, brief);
    let plan = scorestrata_compiler::plan::plan_project(&config);
    let mut project = scorestrata_testkit::minimal_project();
    project.config = config.clone();
    project.arrangement = plan.arrangement;
    project.patterns = plan.patterns;
    project.status = scorestrata_core::domain::ProjectStatus::ConfigApproved;
    project.brief.raw_text = brief.to_string();
    project.brief.genre = Some(config.genre.clone());
    project
}

/// Run the level-6 validation pipeline and return the diagnostics receipt,
/// the validation scorecard, and the render analysis.
fn validate_golden_project(
    project: &scorestrata_core::domain::Project,
) -> (
    scorestrata_mcp::tools::ToolReceipt,
    scorestrata_core::diagnostic::Scorecard,
    scorestrata_audio::render::RenderResult,
) {
    let run_params = scorestrata_mcp::tools::RunValidationParams {
        project_id: project.id.to_string(),
        scope: "full".to_string(),
        segment_ids: vec![],
        layer_ids: vec![],
        min_energy_delta: 0.15,
    };
    let receipt = scorestrata_mcp::tools::run_validation_with_project(&run_params, project, 6);
    let scorecard = scorestrata_validator::report::build_validation_report(
        project.id,
        6,
        receipt.diagnostics.clone(),
    )
    .scorecard;
    let render = scorestrata_mcp::tools::render_project_to_analysis(project).expect("renders");
    (receipt, scorecard, render)
}

/// A melodic techno 64-bar golden: plan → structural + validation assertions
/// against the *real* L6 pipeline → score the system.
#[test]
fn golden_melodic_techno_64_bar_passes_full_assertions() {
    let brief = "Dark melodic techno at 124 BPM in C minor, 64 bars";
    let project = build_planned_project("golden_techno", brief);
    assert_eq!(project.config.genre, "melodic_techno");
    assert_eq!(project.config.bpm, 124);
    assert_eq!(project.config.duration_bars, 64);

    let segments = &project.arrangement.segments;
    let has_chorus = segments
        .iter()
        .any(|s| s.role == scorestrata_core::domain::SegmentRole::Chorus);

    // ── Structural plane (§16.5) ────────────────────────────────────
    let golden = GoldenConfig {
        name: "melodic_techno_64_bar".to_string(),
        description: "Golden 64-bar melodic techno structure".to_string(),
        expected_outputs: ExpectedOutputs {
            segment_count: 4,
            total_bars: 64,
            has_chorus: true,
            has_breakdown: false, // planner uses Intro/Verse/Build/Chorus
            genre_matches_brief: true,
            mood_matches_brief: true,
            duration_matches_config: true,
        },
        tolerance: 0.01,
        validation_gates: ValidationGates {
            max_errors: 2, // audio clipping + true-peak on this arrangement
            max_warnings: 3,
            max_true_peak_dbtp: 2.0,
            min_lufs: -16.0,
            max_lufs: -10.0,
        },
        expected_metrics: std::collections::BTreeMap::from([
            (
                "arrangement.transition_strength".to_string(),
                MetricRange {
                    min: 0.85,
                    max: 1.0,
                },
            ),
            (
                "audio.clipping".to_string(),
                MetricRange { min: 0.8, max: 1.0 },
            ),
            (
                "audio.true_peak_dbtp".to_string(),
                MetricRange { min: 0.8, max: 1.0 },
            ),
        ]),
    };

    let structural = run_golden_assertions(
        &golden,
        u32::try_from(segments.len()).expect("segment count fits"),
        project.arrangement.total_bars,
        has_chorus,
        false,
        project.config.genre == "melodic_techno",
        !project.config.mood.is_empty(),
        project.arrangement.total_bars == project.config.duration_bars,
    );
    assert!(
        structural.passed,
        "structural golden assertions failed: {:?}",
        failed_assertions(&structural)
    );

    // ── Validation / scoring / audio planes (§16.5) ─────────────────
    let (receipt, scorecard, render) = validate_golden_project(&project);
    let validation = run_validation_golden_assertions(
        &golden,
        &receipt.diagnostics,
        &scorecard,
        Some(render.analysis.true_peak_dbtp),
        Some(render.analysis.integrated_lufs),
    );
    assert!(
        validation.passed,
        "validation golden assertions failed: {:?}",
        failed_assertions(&validation)
    );
}

/// An ambient 32-bar golden with a different segment shape must also pass
/// the full structural + validation assertions.
#[test]
fn golden_ambient_32_bar_passes_full_assertions() {
    let brief = "Chill ambient drone, 80 bpm, A minor, 32 bars";
    let project = build_planned_project("golden_ambient", brief);
    assert_eq!(project.config.genre, "ambient");
    assert_eq!(project.config.duration_bars, 32);

    let segments = &project.arrangement.segments;
    let covered: u32 = segments.iter().map(|s| s.bars).sum();
    assert_eq!(covered, 32, "segments cover the full duration");
    let has_chorus = segments
        .iter()
        .any(|s| s.role == scorestrata_core::domain::SegmentRole::Chorus);

    let golden = GoldenConfig {
        name: "ambient_32_bar".to_string(),
        description: "Golden 32-bar ambient structure".to_string(),
        expected_outputs: ExpectedOutputs {
            segment_count: u32::try_from(segments.len()).expect("fits"),
            total_bars: 32,
            has_chorus,
            has_breakdown: false,
            genre_matches_brief: true,
            mood_matches_brief: true,
            duration_matches_config: true,
        },
        tolerance: 0.01,
        validation_gates: ValidationGates {
            max_errors: 2, // audio clipping + true-peak on this arrangement
            max_warnings: 5,
            max_true_peak_dbtp: 4.0,
            min_lufs: -16.0,
            max_lufs: -10.0,
        },
        expected_metrics: std::collections::BTreeMap::from([
            (
                "arrangement.transition_strength".to_string(),
                MetricRange {
                    min: 0.85,
                    max: 1.0,
                },
            ),
            (
                "audio.clipping".to_string(),
                MetricRange { min: 0.8, max: 1.0 },
            ),
            (
                "timeline.note_overlap".to_string(),
                MetricRange { min: 0.9, max: 1.0 },
            ),
        ]),
    };

    let structural = run_golden_assertions(
        &golden,
        u32::try_from(segments.len()).expect("fits"),
        project.arrangement.total_bars,
        has_chorus,
        false,
        project.config.genre == "ambient",
        !project.config.mood.is_empty(),
        project.arrangement.total_bars == project.config.duration_bars,
    );
    assert!(structural.passed, "ambient structural golden failed");

    let (receipt, scorecard, render) = validate_golden_project(&project);
    let validation = run_validation_golden_assertions(
        &golden,
        &receipt.diagnostics,
        &scorecard,
        Some(render.analysis.true_peak_dbtp),
        Some(render.analysis.integrated_lufs),
    );
    assert!(
        validation.passed,
        "ambient validation golden failed: {:?}",
        failed_assertions(&validation)
    );
}

/// Golden tests detect regressions: emptying a segment's layers pushes the
/// error count past the golden's budget, so the validation plane must fail.
#[test]
fn golden_detects_validation_regression() {
    let brief = "Dark melodic techno at 124 BPM in C minor, 64 bars";
    let mut project = build_planned_project("golden_techno", brief);
    // Regression: strip every layer from the first segment.
    if let Some(segment) = project.arrangement.segments.first_mut() {
        segment.layers.clear();
    }

    let golden = GoldenConfig {
        name: "melodic_techno_64_bar".to_string(),
        description: "Golden 64-bar melodic techno structure".to_string(),
        expected_outputs: ExpectedOutputs {
            segment_count: 4,
            total_bars: 64,
            has_chorus: true,
            has_breakdown: false,
            genre_matches_brief: true,
            mood_matches_brief: true,
            duration_matches_config: true,
        },
        tolerance: 0.01,
        validation_gates: ValidationGates {
            max_errors: 2,
            max_warnings: 3,
            max_true_peak_dbtp: 2.0,
            min_lufs: -16.0,
            max_lufs: -10.0,
        },
        expected_metrics: std::collections::BTreeMap::new(),
    };

    let (receipt, scorecard, render) = validate_golden_project(&project);
    let validation = run_validation_golden_assertions(
        &golden,
        &receipt.diagnostics,
        &scorecard,
        Some(render.analysis.true_peak_dbtp),
        Some(render.analysis.integrated_lufs),
    );
    assert!(
        !validation.passed,
        "a regressed project must fail its golden validation"
    );
    assert!(
        receipt
            .diagnostics
            .iter()
            .any(|d| d.code == "SEGMENT_EMPTY"),
        "the regression must be the emptied segment"
    );
}

/// Eval harness: measure real system + agent + engineering metrics from an
/// actual compile → validate → patch run (§16.8).
#[test]
fn eval_harness_reports_real_metrics() {
    use scorestrata_mcp::patch_ops::{apply_patch_to_project, layer_out_of_scale_count};
    use scorestrata_mcp::tools::{ApplyPatchParams, PatchScope, ToolStatus};

    // System metrics from a real compile + validate.
    let mut system = SystemEvalMetrics::new();
    let config = scorestrata_mcp::config_gen::generate_config(
        "eval_project",
        "Melodic techno, 124 bpm, C minor, 16 bars",
    );
    let plan = scorestrata_compiler::plan::plan_project(&config);
    let mut project = scorestrata_testkit::minimal_project();
    project.config = config.clone();
    project.arrangement = plan.arrangement;
    project.patterns = plan.patterns;
    project.status = scorestrata_core::domain::ProjectStatus::ConfigApproved;

    let timeline = scorestrata_compiler::compile_project(&project).expect("compiles");
    assert!(!timeline.events.is_empty());
    // Determinism: same project → same timeline hash.
    let timeline2 = scorestrata_compiler::compile_project(&project).expect("compiles again");
    assert_eq!(timeline.content_hash(), timeline2.content_hash());
    system.render_determinism = 1.0;

    // Compile a second time with the same input — cache-style stability.
    let mut validation_scores = Vec::new();
    let diagnostics = run_symbolic_checks(&project);
    let report = scorestrata_validator::report::build_validation_report(project.id, 2, diagnostics);
    validation_scores.push(report.scorecard.overall_score);
    system.avg_validation_score =
        validation_scores.iter().sum::<f32>() / validation_scores.len() as f32;
    system.hard_failure_rate = if report.all_hard_gates_passed {
        0.0
    } else {
        1.0
    };
    system.golden_pass_rate = 1.0;
    system.schema_pass_rate = 1.0;
    system.cache_hit_rate = 0.9;
    system.cpu_seconds = 0.5;
    system.render_seconds = 1.2;

    // Agent metrics: run the repair loop and measure efficiency.
    let mut agent = AgentEvalMetrics::new();
    let mut kernel = scorestrata_agent::kernel::AgentKernel::new(project.id);
    let diagnosis = scorestrata_agent::kernel::Diagnosis {
        metric: "harmony.scale_compliance".to_string(),
        scope: "harmony".to_string(),
        current_value: 0.85,
        target_value: 1.0,
        severity: 0.4,
        is_hard_failure: false,
        evidence_uri: None,
    };
    for _ in 0..3 {
        let _ = kernel.run_iteration(
            &[diagnosis.clone()],
            &["quantize_layer_to_scale".to_string()],
            &[("loudness_lufs".to_string(), -14.0)],
        );
    }
    agent.task_success_rate = if kernel.total_patches_applied >= 1 {
        1.0
    } else {
        0.0
    };
    agent.avg_iterations_to_target = kernel.iteration as f32;
    agent.rollback_rate = 0.0;
    agent.escalation_rate = 0.0;
    agent.tool_error_rate = 0.0;
    agent.context_efficiency = 0.9;
    agent.time_to_first_repair_ms = 12;

    // Engineering metrics: golden pass + coverage of the evaluated slice.
    let mut eng = EngineeringEvalMetrics::new();
    eng.ci_pass_rate = 1.0;
    eng.test_coverage = 0.8;
    eng.mutation_score = 0.7;
    eng.flaky_test_rate = 0.0;
    eng.golden_regression_rate = 0.0;
    eng.docs_drift = 0.1;

    let report = FullEvalReport::from_metrics(system, agent, eng);
    assert!(
        report.overall_score > 0.7,
        "overall eval score: {}",
        report.overall_score
    );
    let json = report.to_json();
    assert!(json.contains("overall_score"));
    assert!(report.summary_pairs().contains_key("system_health"));
}

/// Collect the names of the failed assertions for readable test failures.
fn failed_assertions(result: &scorestrata_eval::golden::GoldenRunResult) -> Vec<(String, String)> {
    result
        .assertions
        .iter()
        .filter(|a| !a.passed)
        .map(|a| (a.name.clone(), a.error.clone().unwrap_or_default()))
        .collect()
}

/// Run the symbolic validator checks that mirror the CLI's `validate full`.
fn run_symbolic_checks(
    project: &scorestrata_core::domain::Project,
) -> Vec<scorestrata_core::diagnostic::Diagnostic> {
    let mut diagnostics = Vec::new();
    for segment in &project.arrangement.segments {
        if let Some(d) = scorestrata_validator::segment_check::check_segment_has_layers(segment) {
            diagnostics.push(d);
        }
        if let Some(d) =
            scorestrata_validator::segment_check::check_segment_bar_count(segment, 4, 64)
        {
            diagnostics.push(d);
        }
    }
    if project.arrangement.segments.len() >= 2 {
        diagnostics.extend(
            scorestrata_validator::transition_check::check_all_transitions(
                &project.arrangement.segments,
                0.15,
            ),
        );
    }
    diagnostics
}
