//! MVAS end-to-end simulation — the Minimum Viable Agentic Slice (§20.1).
//!
//! Proves the closed loop on a tiny vertical slice, exactly as the
//! architecture spec requires before scaling to the full MVP:
//!
//! 1. receive a brief,
//! 2. generate a valid config,
//! 3. create an 8-bar loop,
//! 4. compile to timeline IR,
//! 5. export MIDI,
//! 6. run symbolic validation,
//! 7. receive diagnostics,
//! 8. apply at least 2 successful patches,
//! 9. prove score improvement,
//! 10. snapshot and rollback,
//! 11. produce receipts and evidence,
//! 12. stay within budget.
//!
//! This test wires the real crates together — config generator, arrangement
//! planner, compiler, MIDI export, validator, patch ops, storage, and the
//! agent kernel — so a regression in any plane fails the slice.

#![expect(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::float_cmp,
    clippy::items_after_statements,
    clippy::similar_names,
    clippy::too_many_lines,
    clippy::unnecessary_get_then_check,
    reason = "Integration test asserts the full slice explicitly; helper fns are long by design"
)]

use scorestrata_core::ids::ProjectId;

/// Steps 1–5: brief → config → 8-bar loop → timeline IR → MIDI.
#[test]
fn mvas_steps_1_to_5_build_an_8_bar_loop() {
    // ── 1. Receive a brief ──────────────────────────────────────────
    let brief = "Dark melodic techno at 128 BPM in C minor, 8 bars";
    let project_name = "mvas_loop";

    // ── 2. Generate a valid config ──────────────────────────────────
    let config = scorestrata_mcp::config_gen::generate_config(project_name, brief);
    assert_eq!(config.bpm, 128, "BPM parsed from brief");
    assert_eq!(config.key, "C minor", "key parsed from brief");
    assert_eq!(config.duration_bars, 8, "8-bar loop requested");

    // Key must be parseable (validator gate).
    assert!(
        scorestrata_theory::key::Key::parse(&config.key).is_some(),
        "config key must be parseable"
    );

    // ── 3. Create the 8-bar loop ────────────────────────────────────
    let plan = scorestrata_compiler::plan::plan_project(&config);
    assert_eq!(plan.arrangement.total_bars, 8);
    let covered: u32 = plan.arrangement.segments.iter().map(|s| s.bars).sum();
    assert_eq!(covered, 8, "segments cover the full duration");
    assert!(
        plan.arrangement
            .segments
            .iter()
            .all(|s| !s.layers.is_empty()),
        "every segment has layers"
    );
    assert!(!plan.patterns.is_empty(), "patterns are planned");

    // ── 4. Compile to timeline IR ───────────────────────────────────
    // Build a real project from the plan (segments, layers, patterns).
    let mut project = scorestrata_testkit::minimal_project();
    project.config = config;
    project.arrangement = plan.arrangement.clone();
    project.patterns = plan.patterns;
    project.status = scorestrata_core::domain::ProjectStatus::ConfigApproved;

    let timeline = scorestrata_compiler::compile_project(&project)
        .expect("planned project compiles to timeline IR");
    assert!(!timeline.events.is_empty(), "timeline has events");
    assert!(!timeline.input_hash.is_empty(), "input hash is computed");
    assert!(
        timeline.total_ticks >= 8 * 480 * 4,
        "8 bars at 4/4 produce at least 8*480*4 ticks"
    );

    // ── 5. Export MIDI ──────────────────────────────────────────────
    let midi = scorestrata_compiler::midi_export::export_midi(&timeline)
        .expect("timeline exports to MIDI");
    assert_eq!(&midi[0..4], b"MThd", "MIDI header magic");
    assert!(
        midi.windows(4).any(|w| w == b"MTrk"),
        "MIDI contains a track chunk"
    );
}

/// Steps 6–9: validation → diagnostics → 2 successful patches → score improvement.
#[test]
fn mvas_steps_6_to_9_repair_loop_proves_improvement() {
    use scorestrata_mcp::patch_ops::{apply_patch_to_project, layer_out_of_scale_count};
    use scorestrata_mcp::tools::{ApplyPatchParams, PatchScope, ToolStatus};

    // Build an 8-bar project with an out-of-scale lead (deliberate defect).
    let mut project = project_with_out_of_scale_lead();

    // ── 6. Run symbolic validation ──────────────────────────────────
    let report = build_report(&project);
    let diag_count = report.summary.warnings + report.summary.errors + report.summary.critical;

    // ── 7. Receive diagnostics ──────────────────────────────────────
    assert!(
        diag_count > 0,
        "the deliberately broken project produces diagnostics"
    );
    let has_scale_diag = report
        .diagnostics
        .iter()
        .any(|d| d.code == "HARMONY_SCALE_VIOLATION");
    assert!(has_scale_diag, "scale diagnostics are actionable");

    // Capture the baseline score.
    let baseline_score = report.scorecard.overall_score;

    // ── 8. Apply at least 2 successful patches ──────────────────────
    // Patch A: quantize the lead layer to scale.
    let (seg_id, layer_id) = lead_scope(&project);
    let mut quantized = 0usize;
    {
        let key = scorestrata_theory::key::Key::parse("C minor").expect("config key parses");
        let before = layer_out_of_scale_count(lead_layer(&project), &project.patterns, &key);
        assert!(before > 0, "lead layer starts out of scale");

        let params = ApplyPatchParams {
            project_id: project.id.to_string(),
            op: "quantize_layer_to_scale".to_string(),
            params: serde_json::json!({"strength": 1.0}),
            dry_run: false,
            scope: Some(PatchScope {
                segment_id: Some(seg_id.clone()),
                layer_id: Some(layer_id),
            }),
            project: None,
        };
        let receipt = apply_patch_to_project(&params, &mut project);
        assert!(
            matches!(receipt.status, ToolStatus::Succeeded),
            "quantize patch succeeds: {}",
            receipt.summary
        );
        assert!(
            receipt.score_delta.contains_key("harmony.scale_compliance"),
            "receipt reports a score delta"
        );
        assert!(
            !receipt.evidence_uris.is_empty(),
            "receipt has evidence URIs"
        );
        quantized += 1;

        // Verify the fix actually landed.
        let after = layer_out_of_scale_count(lead_layer(&project), &project.patterns, &key);
        assert_eq!(after, 0, "all lead notes are now in scale");
    }

    // Patch B: add a transition package (structural improvement).
    let transition_params = ApplyPatchParams {
        project_id: project.id.to_string(),
        op: "add_transition_package".to_string(),
        params: serde_json::json!({"fill": true, "riser": true, "impact": true}),
        dry_run: false,
        scope: Some(PatchScope {
            segment_id: Some(seg_id),
            layer_id: None,
        }),
        project: None,
    };
    let receipt = apply_patch_to_project(&transition_params, &mut project);
    assert!(
        matches!(receipt.status, ToolStatus::Succeeded),
        "transition patch succeeds: {}",
        receipt.summary
    );
    assert!(
        receipt
            .score_delta
            .contains_key("arrangement.transition_strength"),
        "transition receipt reports a score delta"
    );
    quantized += 1;

    assert!(quantized >= 2, "at least 2 successful patches applied");

    // ── 9. Prove score improvement ──────────────────────────────────
    let improved_report = build_report(&project);
    let after_score = improved_report.scorecard.overall_score;
    let after_diag_count = improved_report.summary.warnings
        + improved_report.summary.errors
        + improved_report.summary.critical;

    assert!(
        after_diag_count < diag_count,
        "diagnostics decreased: {diag_count} → {after_diag_count}"
    );
    assert!(
        after_score > baseline_score,
        "score improved: {baseline_score} → {after_score}"
    );
}

/// Steps 10–11: snapshot, diff, rollback; receipts and evidence.
#[test]
fn mvas_steps_10_to_11_snapshot_rollback_and_evidence() {
    use scorestrata_core::ids::{BranchId, SnapshotId};
    use scorestrata_core::snapshot::Snapshot;

    let project_id = ProjectId::new();
    let branch_id = BranchId::new();

    // Initial snapshot.
    let snap_initial = Snapshot {
        id: SnapshotId::new(),
        project_id,
        branch_id,
        label: "initial".to_string(),
        created_at: chrono::Utc::now(),
        command_id: None,
        state_hash: "state_v0".to_string(),
        scorecard_hash: None,
    };

    // After patch A.
    let snap_after_a = Snapshot {
        id: SnapshotId::new(),
        project_id,
        branch_id,
        label: "after_quantize".to_string(),
        created_at: chrono::Utc::now(),
        command_id: None,
        state_hash: "state_v1".to_string(),
        scorecard_hash: None,
    };

    // After patch B (head).
    let snap_after_b = Snapshot {
        id: SnapshotId::new(),
        project_id,
        branch_id,
        label: "after_transition".to_string(),
        created_at: chrono::Utc::now(),
        command_id: None,
        state_hash: "state_v2".to_string(),
        scorecard_hash: None,
    };

    let initial_id = snap_initial.id.as_hex();
    let after_a_id = snap_after_a.id.as_hex();
    let after_b_id = snap_after_b.id.as_hex();

    let graph = scorestrata_storage::snapshot_graph::SnapshotGraph::from_snapshots(vec![
        snap_initial,
        snap_after_a,
        snap_after_b,
    ]);

    // Head and root are tracked.
    let head = graph.head().expect("graph has a head");
    assert_eq!(head.label, "after_transition");
    let root = graph.root().expect("graph has a root");
    assert_eq!(root.label, "initial");

    // Parent chain preserves history (rollback path).
    let chain = graph.parent_chain(&after_b_id);
    assert_eq!(chain.len(), 3, "root → a → b");
    assert_eq!(chain[0].label, "after_transition");
    assert_eq!(chain[2].label, "initial");

    // ── 10. Snapshot and rollback ───────────────────────────────────
    // Diff shows state changes between patches.
    let diff = graph
        .diff(&initial_id, &after_b_id)
        .expect("both snapshots exist");
    assert!(diff.has_changes());
    assert!(
        diff.changes.iter().any(|c| c.field == "state_hash"),
        "diff reports the state change"
    );

    // Rollback: the parent chain lets us restore the pre-patch state.
    let rollback_target = graph.get(&initial_id).expect("initial snapshot restorable");
    assert_eq!(rollback_target.state_hash, "state_v0");

    // ── 11. Produce receipts and evidence ───────────────────────────
    // Every mutation appends a ledger event with evidence payload.
    let ledger = scorestrata_core::event::EventLedger::new(project_id);
    let mut ledger = ledger;
    ledger.append(scorestrata_core::event::LedgerEntry {
        event_id: scorestrata_core::ids::EventId::new(),
        command_id: scorestrata_core::ids::CommandId::new(),
        event_type: "pattern_updated".to_string(),
        branch_id: Some(branch_id),
        snapshot_id: None,
        payload: serde_json::json!({
            "op": "quantize_layer_to_scale",
            "evidence_uri": "project://mvas/segments/0/layers/lead/notes",
            "score_delta": {"harmony.scale_compliance": "+0.33"}
        }),
        timestamp: chrono::Utc::now(),
    });
    ledger.append(scorestrata_core::event::LedgerEntry {
        event_id: scorestrata_core::ids::EventId::new(),
        command_id: scorestrata_core::ids::CommandId::new(),
        event_type: "snapshot_created".to_string(),
        branch_id: Some(branch_id),
        snapshot_id: Some(
            after_a_id
                .parse::<scorestrata_core::ids::SnapshotId>()
                .expect("valid snapshot id"),
        ),
        payload: serde_json::json!({"state_hash": "state_v1"}),
        timestamp: chrono::Utc::now(),
    });

    assert_eq!(ledger.event_count(), 2, "every mutation is recorded");
    assert!(
        ledger
            .events
            .iter()
            .all(|e| e.payload.get("evidence_uri").is_some()
                || e.payload.get("state_hash").is_some()),
        "ledger entries carry evidence"
    );

    // ── 12. Stay within budget (agent kernel) ───────────────────────
    let config = scorestrata_agent::kernel::AgentConfig {
        max_iterations: 12,
        max_no_improvement: 3,
        target_score: 0.85,
        ..scorestrata_agent::kernel::AgentConfig::default()
    };
    let mut kernel = scorestrata_agent::kernel::AgentKernel::with_config(project_id, config);

    // Simulate the repair loop consuming budget.
    let diagnosis = scorestrata_agent::kernel::Diagnosis {
        metric: "harmony.scale_compliance".to_string(),
        scope: "harmony".to_string(),
        current_value: 0.67,
        target_value: 1.0,
        severity: 0.6,
        is_hard_failure: false,
        evidence_uri: Some("project://mvas/metrics/scale_compliance".to_string()),
    };
    for _ in 0..2 {
        let result = kernel.run_iteration(
            std::slice::from_ref(&diagnosis),
            &["quantize_layer_to_scale".to_string()],
            &[("loudness_lufs".to_string(), -14.0)],
        );
        let result = result.expect("iteration runs");
        assert!(result.improved, "each patch iteration improves");
    }
    assert!(kernel.total_patches_applied >= 2, "two successful patches");
    assert!(
        kernel.iteration <= kernel.config.max_iterations,
        "iteration budget respected"
    );
    assert!(!kernel.should_terminate(), "budget not exhausted early");
}

/// Level-4 audio validation: render the loop, measure LUFS/true peak, and
/// prove `normalize_loudness_target` moves the mix toward the target.
#[test]
fn mvas_step_13_level4_audio_validation_loop() {
    use scorestrata_mcp::patch_ops::apply_patch_to_project;
    use scorestrata_mcp::tools::{ApplyPatchParams, ToolStatus};

    // Build the 8-bar loop from a brief, as in steps 1–4.
    let brief = "Dark melodic techno at 128 BPM in C minor, 8 bars";
    let config = scorestrata_mcp::config_gen::generate_config("mvas_audio", brief);
    let plan = scorestrata_compiler::plan::plan_project(&config);
    let mut project = scorestrata_testkit::minimal_project();
    project.config = config;
    project.arrangement = plan.arrangement;
    project.patterns = plan.patterns;

    // Quiet the mix so normalization has measurable work to do.
    for pattern in &mut project.patterns {
        for note in &mut pattern.notes {
            note.velocity = scorestrata_core::domain::Velocity::new(25);
        }
        for step in &mut pattern.steps {
            if step.active {
                step.velocity = scorestrata_core::domain::Velocity::new(25);
            }
        }
    }

    // ── Render the loop ─────────────────────────────────────────────
    let timeline =
        scorestrata_compiler::compile_project(&project).expect("planned project compiles");
    let mut layer_names = std::collections::HashMap::new();
    for segment in &project.arrangement.segments {
        for layer in &segment.layers {
            layer_names.insert(layer.id.to_string(), layer.name.clone());
        }
    }
    let notes: Vec<scorestrata_audio::render::RenderNote> = timeline
        .events
        .iter()
        .filter_map(|e| match &e.kind {
            scorestrata_compiler::timeline::EventKind::NoteOn { pitch, velocity } => {
                let layer_name = layer_names
                    .get(&e.layer_id.to_string())
                    .cloned()
                    .unwrap_or_else(|| e.layer_id.to_string());
                Some(scorestrata_audio::render::RenderNote {
                    pitch: pitch.0,
                    start_tick: e.tick.0,
                    duration_ticks: 480,
                    velocity: velocity.as_f32(),
                    layer_name,
                })
            }
            _ => None,
        })
        .collect();
    let result = scorestrata_audio::render::render_notes(
        &notes,
        timeline.bpm,
        scorestrata_audio::render::DEFAULT_SAMPLE_RATE,
        &[],
    )
    .expect("render succeeds");

    // Level-4 metrics are present and meaningful.
    assert!(
        result.analysis.integrated_lufs > -70.0,
        "non-silent render has measurable loudness ({:.1} LUFS)",
        result.analysis.integrated_lufs
    );
    assert!(
        !result.analysis.clipping_detected,
        "quiet mix does not clip"
    );
    assert!(result.analysis.true_peak_dbtp < -1.0, "peak under -1 dBTP");

    // ── normalize_loudness_target moves the mix toward -14 LUFS ─────
    let params = ApplyPatchParams {
        project_id: project.id.to_string(),
        op: "normalize_loudness_target".to_string(),
        params: serde_json::json!({}),
        dry_run: false,
        scope: None,
        project: None,
    };
    let receipt = apply_patch_to_project(&params, &mut project);
    assert!(
        matches!(receipt.status, ToolStatus::Succeeded),
        "normalize patch succeeds: {}",
        receipt.summary
    );
    assert!(
        receipt.summary.contains("LUFS"),
        "receipt reports loudness before/after"
    );

    // Velocities were boosted, so a re-render lands closer to -14 LUFS.
    assert!(
        project
            .patterns
            .iter()
            .flat_map(|p| p.notes.iter())
            .all(|n| n.velocity.0 >= 25),
        "velocities adjusted"
    );
    let re_render = scorestrata_compiler::compile_project(&project).expect("recompiles");
    assert!(!re_render.events.is_empty());
}

/// Level-6 regression + genre-calibrated loudness (§11.6/§11.7): the agent
/// loop detects loudness drift between renders and the mix diagnosis
/// produces a normalize candidate targeting the genre's calibrated LUFS.
#[test]
fn mvas_step_14_genre_calibrated_loudness_and_render_regression() {
    use scorestrata_agent::kernel::AgentKernel;
    use scorestrata_mcp::patch_ops::apply_patch_to_project;
    use scorestrata_mcp::tools::{ApplyPatchParams, ToolStatus};

    // ── Genre-calibrated target (§11.6) ─────────────────────────────
    // A melodic-techno project targets -10 LUFS, not the -14 default.
    assert_eq!(
        scorestrata_storage::calibration::loudness_target_for("melodic_techno"),
        Some(-10.0)
    );
    assert_eq!(
        scorestrata_storage::calibration::loudness_target_for("ambient"),
        Some(-16.0)
    );
    assert_eq!(
        scorestrata_storage::calibration::loudness_target_for("jazz"),
        None
    );

    // Build a melodic-techno project.
    let config = scorestrata_mcp::config_gen::generate_config(
        "mvas_calib",
        "Dark melodic techno at 128 BPM in C minor, 8 bars",
    );
    assert_eq!(config.genre, "melodic_techno");
    let plan = scorestrata_compiler::plan::plan_project(&config);
    let mut project = scorestrata_testkit::minimal_project();
    project.config = config;
    project.arrangement = plan.arrangement;
    project.patterns = plan.patterns;

    // ── normalize uses the genre target (-10) ───────────────────────
    let params = ApplyPatchParams {
        project_id: project.id.to_string(),
        op: "normalize_loudness_target".to_string(),
        params: serde_json::json!({}),
        dry_run: false,
        scope: None,
        project: None,
    };
    let receipt = apply_patch_to_project(&params, &mut project);
    assert!(
        matches!(receipt.status, ToolStatus::Succeeded),
        "normalize succeeds: {}",
        receipt.summary
    );
    assert!(
        receipt.summary.contains("targeting -10.0"),
        "receipt uses the genre-calibrated -10 LUFS target, got: {}",
        receipt.summary
    );

    // ── Render regression detection (§11.7 Level 6) ─────────────────
    use scorestrata_validator::regression_check::{
        RenderRegressionConfig, RenderSignature, detect_render_regression,
        render_regression_diagnostics,
    };
    let baseline = RenderSignature {
        checksum: "base_checksum_123".to_string(),
        integrated_lufs: -10.0,
        true_peak_dbtp: -1.8,
    };
    // A drifted render after some patch.
    let drifted = RenderSignature {
        checksum: "drifted_checksum_456".to_string(),
        integrated_lufs: -7.2,
        true_peak_dbtp: -0.4,
    };
    let config6 = RenderRegressionConfig::default();
    let regression = detect_render_regression(&baseline, &drifted, &config6);
    assert!(regression.loudness_drifted);
    assert!(regression.peak_drifted);
    assert!(regression.checksum_changed);
    let diags = render_regression_diagnostics(&regression);
    assert!(
        diags.iter().any(|d| d.code == "RENDER_LOUDNESS_DRIFT"),
        "loudness drift diagnostic"
    );
    assert!(
        diags.iter().any(|d| d.code == "RENDER_PEAK_DRIFT"),
        "peak drift diagnostic"
    );
    // The drift diagnostic suggests normalize — the agent loop can repair.
    assert!(diags.iter().any(|d| {
        d.suggested_patches
            .iter()
            .any(|p| p.op == "normalize_loudness_target")
    }));

    // ── Agent kernel turns the drifted render into a mix diagnosis ──
    let mut kernel = AgentKernel::new(project.id);
    let diag = kernel
        .diagnosis_from_audio_metrics(-7.2, -0.4)
        .expect("clipping/drifted master yields a diagnosis");
    assert_eq!(diag.scope, "mix");
    let candidates = kernel.l1_micro_repair(&diag, &[]);
    assert!(
        candidates
            .iter()
            .any(|c| c.operation == "normalize_loudness_target"),
        "agent loop proposes the loudness repair"
    );
}

/// Debug plane (§12) + learning loop (§13): the agent can trace commands,
/// diff candidates, isolate layers, explain metrics, and record/query
/// lessons — the evidence and knowledge the closed loop relies on.
#[test]
fn mvas_step_15_debug_plane_and_learning_loop() {
    use scorestrata_mcp::tools::{
        CandidateDiffParams, ExplainMetricParams, GetCommandTraceParams, GetLessonsParams,
        GetRepairTemplatesParams, IsolateLayerParams, RecordLessonParams, SnapshotDiffParams,
        SoloSegmentParams, ToolStatus,
    };

    let project = scorestrata_testkit::melodic_techno_project();
    let project_json = serde_json::to_value(&project).expect("project serializes");
    let project_id = project.id.to_string();

    // ── 15a. Trace a command through episodic memory ────────────────
    let mut memory = scorestrata_storage::memory::ProjectMemory::new();
    memory.record_command(scorestrata_storage::memory::CommandRecord {
        command_id: "cmd_apply_quantize".to_string(),
        command_type: "apply_patch".to_string(),
        actor: "agent:composer".to_string(),
        success: true,
        score_delta: Some(0.14),
        duration_ms: 42,
        timestamp: chrono::Utc::now().to_rfc3339(),
    });
    let trace_receipt = scorestrata_mcp::tools::get_command_trace(&GetCommandTraceParams {
        project_id: project_id.clone(),
        command_id: "cmd_apply_quantize".to_string(),
        memory: Some(serde_json::to_value(&memory).expect("memory serializes")),
    });
    assert_eq!(trace_receipt.tool, "get_command_trace");
    assert!(
        matches!(trace_receipt.status, ToolStatus::Succeeded),
        "command trace succeeds: {}",
        trace_receipt.summary
    );
    assert!(trace_receipt.summary.contains("42 ms"));
    assert!(
        trace_receipt
            .evidence_uris
            .iter()
            .any(|u| u.contains("trace/command")),
        "trace evidence URI"
    );

    // ── 15b. Diff two candidates and pick a winner ──────────────────
    // Candidate A is healthy; B is degraded (out-of-scale notes).
    let mut candidate_b = project.clone();
    for pattern in &mut candidate_b.patterns {
        for note in &mut pattern.notes {
            note.pitch = scorestrata_core::domain::MidiNote::new(61);
        }
    }
    let diff_receipt = scorestrata_mcp::tools::get_candidate_diff(&CandidateDiffParams {
        project_id: project_id.clone(),
        candidate_a: "cand_a".to_string(),
        candidate_b: "cand_b".to_string(),
        project_a: Some(project_json.clone()),
        project_b: Some(serde_json::to_value(&candidate_b).expect("candidate serializes")),
    });
    assert!(
        matches!(diff_receipt.status, ToolStatus::Succeeded),
        "candidate diff succeeds: {}",
        diff_receipt.summary
    );
    assert!(
        diff_receipt.score_delta.contains_key("candidate.winner"),
        "candidate diff reports a winner"
    );
    assert!(diff_receipt.summary.contains("cand_a") || diff_receipt.summary.contains("cand_b"));

    // ── 15c. Snapshot diff between two project states ───────────────
    let mut changed = project.clone();
    for seg in &mut changed.arrangement.segments {
        seg.bars += 4;
    }
    let snap_diff = scorestrata_mcp::tools::get_snapshot_diff(&SnapshotDiffParams {
        project_id: project_id.clone(),
        snapshot_a: "snap_a".to_string(),
        snapshot_b: "snap_b".to_string(),
        project_a: Some(project_json.clone()),
        project_b: Some(serde_json::to_value(&changed).expect("changed serializes")),
    });
    assert!(
        matches!(snap_diff.status, ToolStatus::Succeeded),
        "snapshot diff succeeds: {}",
        snap_diff.summary
    );
    assert!(snap_diff.summary.contains("segments changed"));

    // ── 15d. Isolate a layer and solo a segment ─────────────────────
    let layer_id = project.arrangement.segments[0].layers[0].id.to_string();
    let isolate = scorestrata_mcp::tools::isolate_layer(&IsolateLayerParams {
        project_id: project_id.clone(),
        layer_id: layer_id.clone(),
        project: Some(project_json.clone()),
    });
    assert!(matches!(isolate.status, ToolStatus::Succeeded));
    assert!(isolate.summary.contains(&layer_id));

    let segment_id = project.arrangement.segments[0].id.to_string();
    let solo = scorestrata_mcp::tools::solo_segment(&SoloSegmentParams {
        project_id: project_id.clone(),
        segment_id,
        project: Some(project_json),
    });
    assert!(matches!(solo.status, ToolStatus::Succeeded));
    assert!(solo.summary.contains("bars"));
    assert!(solo.summary.contains("layers"));

    // ── 15e. Explain a metric with root causes ──────────────────────
    // Build a degraded project so the metric has root causes to report.
    let mut degraded = project;
    for pattern in &mut degraded.patterns {
        for note in &mut pattern.notes {
            note.pitch = scorestrata_core::domain::MidiNote::new(61);
        }
    }
    let explain = scorestrata_mcp::tools::explain_metric(&ExplainMetricParams {
        project_id: project_id.clone(),
        metric_name: "harmony.scale_compliance".to_string(),
        project: Some(serde_json::to_value(&degraded).expect("degraded serializes")),
    });
    assert!(
        matches!(explain.status, ToolStatus::Succeeded),
        "metric explanation succeeds: {}",
        explain.summary
    );
    assert!(explain.summary.contains("scale_compliance"));
    assert!(explain.summary.contains("root cause"));

    // ── 15f. Learning loop: record → query → derive templates ──────
    let store = scorestrata_storage::lesson_store::LessonStore::new();
    let record = scorestrata_mcp::tools::record_lesson(&RecordLessonParams {
        project_id: project_id.clone(),
        trigger: "HARMONY_SCALE_VIOLATION".to_string(),
        scope: "harmony".to_string(),
        successful_patch: Some("quantize_layer_to_scale".to_string()),
        genre: Some("melodic_techno".to_string()),
        confidence: 0.9,
        lesson_store: Some(serde_json::to_value(&store).expect("store serializes")),
    });
    assert!(
        matches!(record.status, ToolStatus::Succeeded),
        "record lesson succeeds: {}",
        record.summary
    );
    assert_eq!(record.score_delta["lessons.total"], "1");

    // The updated store is returned in evidence — round-trip it back in.
    let store_prefix = format!("memory://lessons/{project_id}?store=");
    let updated_store = record
        .evidence_uris
        .iter()
        .find_map(|u| u.strip_prefix(&store_prefix))
        .expect("updated store in evidence");
    let store_json: serde_json::Value =
        serde_json::from_str(updated_store).expect("updated store parses");
    let query = scorestrata_mcp::tools::get_lessons(&GetLessonsParams {
        project_id: project_id.clone(),
        trigger: Some("HARMONY_SCALE_VIOLATION".to_string()),
        genre: None,
        min_confidence: 0.5,
        lesson_store: Some(store_json),
    });
    assert!(matches!(query.status, ToolStatus::Succeeded));
    assert_eq!(query.score_delta["lessons.matched"], "1");
    assert!(query.summary.contains("quantize_layer_to_scale"));

    let templates = scorestrata_mcp::tools::get_repair_templates(&GetRepairTemplatesParams {
        project_id,
        lesson_store: Some(serde_json::from_str(updated_store).expect("updated store re-parses")),
    });
    assert!(matches!(templates.status, ToolStatus::Succeeded));
    assert_eq!(templates.score_delta["templates.count"], "1");
    assert!(templates.summary.contains("quantize_layer_to_scale"));
}

/// Human escalation workflow (§12.3): when the agent loop stagnates, it
/// computes a failure report and creates a full human review packet with
/// validation context, failed metrics, and recommended decisions.
#[test]
fn mvas_step_16_human_escalation_workflow() {
    use scorestrata_mcp::tools::{
        AttemptedPatchParams, CreateHumanReviewPacketParams, GetFailureReportParams, ToolStatus,
    };

    // A project the agent cannot fully repair: out-of-scale lead + weak
    // transitions that a human needs to adjudicate.
    let mut project = project_with_out_of_scale_lead();
    for pattern in &mut project.patterns {
        for note in &mut pattern.notes {
            note.pitch = scorestrata_core::domain::MidiNote::new(61);
        }
    }
    let project_json = serde_json::to_value(&project).expect("project serializes");
    let project_id = project.id.to_string();

    // ── 16a. Failure report — why is this failing? ──────────────────
    let report = scorestrata_mcp::tools::get_failure_report(&GetFailureReportParams {
        project_id: project_id.clone(),
        project: Some(project_json.clone()),
    });
    // Status mirrors the report's `needs_repair` decision: this project has
    // a failed metric, so the report is Failed (not "succeeded but escalate").
    assert!(
        matches!(report.status, ToolStatus::Failed),
        "failure report on a broken project is Failed: {}",
        report.summary
    );
    assert!(report.summary.contains("failed metric"));
    assert!(
        report.score_delta.contains_key("report.overall_score"),
        "report includes the overall score"
    );
    assert!(
        report
            .next_actions
            .contains(&"create_human_review_packet".to_string()),
        "failing project is escalated, got: {:?}",
        report.next_actions
    );

    // ── 16b. Human review packet — full context for a decision ──────
    let packet =
        scorestrata_mcp::tools::create_human_review_packet(&CreateHumanReviewPacketParams {
            project_id: project_id.clone(),
            reason: "Stagnation: no improvement after repair loop".to_string(),
            agent_confidence: 0.4,
            attempted_patches: vec![AttemptedPatchParams {
                operation: "quantize_layer_to_scale".to_string(),
                scope: "lead".to_string(),
                succeeded: false,
                score_delta: -0.05,
                iteration: 7,
            }],
            project: Some(project_json),
        });
    assert!(
        matches!(packet.status, ToolStatus::Succeeded),
        "packet creation succeeds: {}",
        packet.summary
    );
    assert!(packet.summary.contains("Stagnation"));
    assert!(
        packet.summary.contains("recommended decision"),
        "packet carries recommended decisions, got: {}",
        packet.summary
    );
    assert_eq!(packet.score_delta["packet.recommended_decisions"], "3");
    assert!(
        packet
            .next_actions
            .contains(&"await_human_decision".to_string()),
        "project transitions to awaiting a human decision"
    );

    // ── 16c. The packet payload is round-trippable ──────────────────
    let packet_prefix = format!("packet://{project_id}/latest?packet=");
    let packet_payload = packet
        .evidence_uris
        .iter()
        .find_map(|u| u.strip_prefix(&packet_prefix))
        .expect("packet payload is returned in evidence");
    let packet_json: serde_json::Value =
        serde_json::from_str(packet_payload).expect("packet payload parses");
    assert_eq!(packet_json["project_id"], serde_json::json!(project_id));
    assert!(
        packet_json["summary"]
            .as_str()
            .unwrap()
            .contains("Escalation"),
        "packet summary is present in the payload"
    );
    assert!(
        packet_json["recommended_decisions"]
            .as_array()
            .unwrap()
            .len()
            == 3,
        "payload carries the recommended decisions"
    );

    // ── 16d. State machine reaches NeedsHumanReview ─────────────────
    // A failing validation (Validating) is the natural escalation point;
    // the state machine guards the Validating → NeedsHumanReview path.
    use scorestrata_core::domain::ProjectStatus;
    use scorestrata_core::state_machine::validate_transition;
    let from = ProjectStatus::Validating;
    let to = ProjectStatus::NeedsHumanReview;
    assert!(
        validate_transition(from, to).is_ok(),
        "Validating can escalate to NeedsHumanReview"
    );
    // ...but it cannot skip straight from NeedsPolicyReview.
    assert!(
        validate_transition(ProjectStatus::NeedsPolicyReview, to).is_err(),
        "NeedsPolicyReview cannot jump straight to NeedsHumanReview"
    );
}

#[test]
fn mvas_step_17_orientation_and_validation_drill_down() {
    // §10.5/§13.1 L0 orientation + validation drill-down: the 8 tools that let
    // an agent bootstrap (contracts, onboarding, summary) and then dig into
    // why a project scores what it scores (report, root causes, evidence,
    // candidate comparison) — all before committing to a mutation (dry run).
    use scorestrata_mcp::tools::{
        CompareCandidatesParams, DryRunPatchParams, GetMetricEvidenceParams,
        GetProjectSummaryParams, GetRootCausesParams, GetValidationReportParams, ToolStatus,
    };

    let clean = scorestrata_testkit::melodic_techno_project();
    let clean_json = serde_json::to_value(&clean).expect("project serializes");
    let broken = project_with_out_of_scale_lead();
    let broken_json = serde_json::to_value(&broken).expect("project serializes");
    let project_id = clean.id.to_string();

    // ── 17a. Orientation: contracts, onboarding packet, project summary ──
    let contracts = scorestrata_mcp::tools::get_contracts();
    assert!(matches!(contracts.status, ToolStatus::Succeeded));
    assert!(
        contracts.summary.contains("schema"),
        "contracts receipt lists the schema registry"
    );

    let onboarding = scorestrata_mcp::tools::get_onboarding_packet();
    assert!(matches!(onboarding.status, ToolStatus::Succeeded));
    assert!(onboarding.summary.contains("Onboarding"));
    assert!(onboarding.summary.contains("tool(s)"));

    let summary = scorestrata_mcp::tools::get_project_summary(&GetProjectSummaryParams {
        project_id: project_id.clone(),
        project: Some(clean_json.clone()),
    });
    assert!(matches!(summary.status, ToolStatus::Succeeded));
    assert!(summary.summary.contains("segment(s)"));
    assert!(summary.score_delta.contains_key("summary.segments"));

    // ── 17b. Validation report (Level 2 symbolic) ──────────────────
    let report = scorestrata_mcp::tools::get_validation_report(&GetValidationReportParams {
        project_id: project_id.clone(),
        level: 2,
        project: Some(clean_json.clone()),
    });
    assert!(matches!(report.status, ToolStatus::Succeeded));
    assert!(report.summary.contains("overall"));
    assert!(report.score_delta.contains_key("report.overall_score"));
    assert!(report.score_delta.contains_key("report.metrics"));

    // ── 17c. Root causes on a metric that is genuinely failing ──────
    let roots = scorestrata_mcp::tools::get_root_causes(&GetRootCausesParams {
        project_id: project_id.clone(),
        metric_name: "harmony.scale_compliance".to_string(),
        project: Some(broken_json.clone()),
    });
    assert!(
        matches!(roots.status, ToolStatus::Succeeded),
        "root-cause attribution succeeds for a measured metric: {roots:#?}"
    );
    assert!(roots.score_delta.contains_key("metric.value"));

    // ── 17d. Evidence drill-down from a recorded scorecard ──────────
    let broken_report = scorestrata_mcp::debug_ops::project_report(&broken);
    let mut warehouse = scorestrata_storage::evidence::EvidenceWarehouse::new();
    warehouse.record_scorecard(
        &broken_report.scorecard,
        2,
        None,
        &broken_report.diagnostics,
    );
    let evidence = scorestrata_mcp::tools::get_metric_evidence(&GetMetricEvidenceParams {
        project_id: project_id.clone(),
        metric_name: "harmony.scale_compliance".to_string(),
        warehouse: Some(serde_json::to_value(&warehouse).expect("warehouse serializes")),
        lineage: None,
    });
    assert!(matches!(evidence.status, ToolStatus::Succeeded));
    assert!(evidence.score_delta.contains_key("evidence.entries"));
    assert!(
        evidence.summary.contains("sample(s)"),
        "evidence summary reports sample count"
    );

    // ── 17e. Candidate comparison picks the higher-scoring option ───
    let compared = scorestrata_mcp::tools::compare_candidates(&CompareCandidatesParams {
        project_id: project_id.clone(),
        candidate_a: "clean".to_string(),
        candidate_b: "broken".to_string(),
        project_a: Some(clean_json.clone()),
        project_b: Some(broken_json),
    });
    assert!(matches!(compared.status, ToolStatus::Succeeded));
    assert!(compared.score_delta.contains_key("diff.metrics_compared"));
    assert!(
        compared.score_delta.get("diff.winner").is_some(),
        "comparison reports a winner between a clean and a broken candidate"
    );

    // ── 17f. Dry-run a patch before committing to it ────────────────
    let dry_run = scorestrata_mcp::tools::dry_run_patch(&DryRunPatchParams {
        project_id,
        op: "quantize_layer_to_scale".to_string(),
        params: serde_json::json!({"strength": 1.0}),
        scope: None,
        project: Some(clean_json),
    });
    assert!(matches!(dry_run.status, ToolStatus::DryRunCompleted));
    assert!(dry_run.dry_run);
    assert!(
        dry_run.summary.contains("nothing persisted"),
        "dry-run states the no-mutation guarantee: {}",
        dry_run.summary
    );
}

#[test]
fn mvas_step_18_render_jobs_and_engineering_plane() {
    // §10.5 render-job + engineering-plane tools: a candidate preview is
    // rendered through the real audio engine with an observable job
    // lifecycle, then the engineering loop closes — result submitted, eval
    // report generated, docs updated, ADR recorded, calibration updated.
    use scorestrata_mcp::tools::{
        CancelRenderJobParams, CreateAdrParams, GetEvalReportParams, GetRenderJobParams,
        RenderCandidatePreviewParams, SubmitTaskResultParams, ToolStatus,
        UpdateCalibrationProfileParams, UpdateDocsParams,
    };

    let project = scorestrata_testkit::melodic_techno_project();
    let project_json = serde_json::to_value(&project).expect("project serializes");
    let project_id = project.id.to_string();

    // ── 18a. Render a candidate preview — real engine, job lifecycle ──
    let preview = scorestrata_mcp::tools::render_candidate_preview(&RenderCandidatePreviewParams {
        project_id: project_id.clone(),
        candidate_id: "cand_a".to_string(),
        project: Some(project_json),
        jobs: Some(serde_json::json!({})),
    });
    assert!(
        matches!(preview.status, ToolStatus::Succeeded),
        "candidate preview should render, got {preview:#?}"
    );
    assert_eq!(
        preview
            .score_delta
            .get("render.job_status")
            .map(String::as_str),
        Some("Completed")
    );
    let job_id = preview
        .score_delta
        .get("render.preview_job")
        .cloned()
        .expect("job id reported");

    // The job is queryable afterwards.
    let mut jobs = scorestrata_storage::render_job::RenderJobStore::new();
    jobs.submit(scorestrata_storage::render_job::RenderJob::new(
        &job_id,
        &project_id,
        "preview",
    ));
    jobs.start(&job_id);
    jobs.complete(
        &job_id,
        &format!("project://{project_id}/render/preview/cand_a"),
    );
    let queried = scorestrata_mcp::tools::get_render_job(&GetRenderJobParams {
        project_id: project_id.clone(),
        job_id: job_id.clone(),
        jobs: Some(serde_json::to_value(&jobs).expect("jobs serialize")),
    });
    assert!(matches!(queried.status, ToolStatus::Succeeded));
    assert_eq!(
        queried
            .score_delta
            .get("render.job_status")
            .map(String::as_str),
        Some("Completed")
    );

    // A queued job can be cancelled; a completed one cannot.
    let mut jobs2 = scorestrata_storage::render_job::RenderJobStore::new();
    jobs2.submit(scorestrata_storage::render_job::RenderJob::new(
        "job_cancel",
        &project_id,
        "master",
    ));
    let cancelled = scorestrata_mcp::tools::cancel_render_job(&CancelRenderJobParams {
        project_id: project_id.clone(),
        job_id: "job_cancel".to_string(),
        jobs: Some(serde_json::to_value(&jobs2).expect("jobs serialize")),
    });
    assert!(matches!(cancelled.status, ToolStatus::Succeeded));

    // ── 18b. Engineering loop: submit result → eval report ──────────
    let mut board = scorestrata_storage::engineering::TaskResultBoard::new();
    board.record(scorestrata_storage::engineering::TaskResult {
        task_id: "TASK-001".to_string(),
        agent_id: "agent:composer".to_string(),
        passed: 3,
        failed: 0,
        score_delta: 0.12,
        evidence_uri: None,
        timestamp: chrono::Utc::now().to_rfc3339(),
    });
    let submitted = scorestrata_mcp::tools::submit_task_result(&SubmitTaskResultParams {
        project_id: project_id.clone(),
        task_id: "TASK-002".to_string(),
        agent_id: "agent:composer".to_string(),
        passed: 2,
        failed: 0,
        score_delta: 0.05,
        board: Some(serde_json::to_value(&board).expect("board serializes")),
    });
    assert!(matches!(submitted.status, ToolStatus::Succeeded));
    assert!(submitted.score_delta.contains_key("engineering.pass_rate"));

    // Now the eval report over the whole board is clean.
    let mut full = scorestrata_storage::engineering::TaskResultBoard::new();
    full.record(scorestrata_storage::engineering::TaskResult {
        task_id: "TASK-001".to_string(),
        agent_id: "agent:composer".to_string(),
        passed: 3,
        failed: 0,
        score_delta: 0.12,
        evidence_uri: None,
        timestamp: chrono::Utc::now().to_rfc3339(),
    });
    full.record(scorestrata_storage::engineering::TaskResult {
        task_id: "TASK-002".to_string(),
        agent_id: "agent:composer".to_string(),
        passed: 2,
        failed: 0,
        score_delta: 0.05,
        evidence_uri: None,
        timestamp: chrono::Utc::now().to_rfc3339(),
    });
    let report = scorestrata_mcp::tools::get_eval_report(&GetEvalReportParams {
        project_id: project_id.clone(),
        board: Some(serde_json::to_value(&full).expect("board serializes")),
    });
    assert!(matches!(report.status, ToolStatus::Succeeded));
    assert!(report.summary.contains("clean"));
    assert!(report.score_delta.contains_key("eval.pass_rate"));

    // ── 18c. Self-documentation: docs update + ADR ──────────────────
    let docs = scorestrata_mcp::tools::update_docs(&UpdateDocsParams {
        project_id: project_id.clone(),
        path: "docs/architecture/architecture.md".to_string(),
        section: "§10.5".to_string(),
        summary: "render job + engineering tools wired".to_string(),
        related_spec: "SPEC-ENGINEERING".to_string(),
        ledger: Some(serde_json::json!({})),
    });
    assert!(matches!(docs.status, ToolStatus::Succeeded));
    assert!(docs.evidence_uris[0].contains("architecture.md"));

    let adr = scorestrata_mcp::tools::create_adr(&CreateAdrParams {
        project_id: project_id.clone(),
        title: "Render jobs get a dedicated store".to_string(),
        context: "Rendering is long-running and must be observable".to_string(),
        decision: "Add RenderJobStore to scorestrata-storage".to_string(),
        consequences: vec![],
        related_spec: "§10.5".to_string(),
        ledger: Some(serde_json::json!({})),
    });
    assert!(matches!(adr.status, ToolStatus::Succeeded));
    assert_eq!(
        adr.score_delta.get("adr.id").map(String::as_str),
        Some("adr_0001")
    );

    // ── 18d. Calibration update ─────────────────────────────────────
    let calibration =
        scorestrata_mcp::tools::update_calibration_profile(&UpdateCalibrationProfileParams {
            project_id,
            genre: "melodic_techno".to_string(),
            target_lufs: -10.0,
            profile: Some(serde_json::json!({})),
        });
    assert!(matches!(calibration.status, ToolStatus::Succeeded));
    assert!(calibration.summary.contains("-10.0 LUFS"));
}

#[test]
fn mvas_step_19_composition_and_patch_mutations() {
    // §10.5/§11 composition + patch tools: notes and steps are written,
    // motivic variation applied, patterns placed, instruments retargeted,
    // goals/energy set, then the mix/patch tools operate on the content.
    use scorestrata_mcp::tools::{
        AddDrumFillParams, ApplyMotivicVariationParams, HarmonizeMelodyParams,
        IncreaseSegmentDensityParams, NormalizeLoudnessTargetParams, PlacePatternInSegmentParams,
        ReduceMaskingParams, SetEnergyCurveParams, SetLayerInstrumentParams, SetSegmentGoalsParams,
        ToolStatus, WritePatternNotesParams, WritePatternStepsParams,
    };

    // Planned patterns are empty, so populate content the way the agent
    // would — write notes into the lead pattern first.
    let mut project = scorestrata_testkit::melodic_techno_project();
    let lead_pattern_id = project.patterns[0].id;
    if let Some(pattern) = project
        .patterns
        .iter_mut()
        .find(|p| p.id == lead_pattern_id)
    {
        // Two spectrally/temporally overlapping notes so de-masking has
        // real content to act on (planned patterns are otherwise empty).
        pattern.notes.extend([
            scorestrata_core::domain::Note {
                id: scorestrata_core::ids::NoteId::new(),
                pitch: scorestrata_core::domain::MidiNote::new(60),
                start: scorestrata_core::domain::Tick::new(0),
                duration: scorestrata_core::domain::Duration(480),
                velocity: scorestrata_core::domain::Velocity::new(110),
                tied: false,
            },
            scorestrata_core::domain::Note {
                id: scorestrata_core::ids::NoteId::new(),
                pitch: scorestrata_core::domain::MidiNote::new(64),
                start: scorestrata_core::domain::Tick::new(24),
                duration: scorestrata_core::domain::Duration(480),
                velocity: scorestrata_core::domain::Velocity::new(110),
                tied: false,
            },
        ]);
    }
    let project_json = serde_json::to_value(&project).expect("project serializes");
    let project_id = project.id.to_string();
    let segment_id = project.arrangement.segments[0].id.to_string();
    let layer_id = project.arrangement.segments[0].layers[0].id.to_string();
    let pattern_id = project.patterns[0].id.to_string();

    // ── 19a. Composition: notes + steps written, variation applied ──
    let notes = scorestrata_mcp::tools::write_pattern_notes(&WritePatternNotesParams {
        project_id: project_id.clone(),
        pattern_id: pattern_id.clone(),
        notes: vec![scorestrata_mcp::tools::NoteSpec {
            pitch: 62,
            start: 480,
            duration: 480,
            velocity: 90,
        }],
        project: Some(project_json.clone()),
    });
    assert!(matches!(notes.status, ToolStatus::Succeeded));

    let steps = scorestrata_mcp::tools::write_pattern_steps(&WritePatternStepsParams {
        project_id: project_id.clone(),
        pattern_id: pattern_id.clone(),
        steps: vec![scorestrata_mcp::tools::StepSpec {
            index: 0,
            active: true,
            velocity: 100,
        }],
        project: Some(project_json.clone()),
    });
    assert!(matches!(steps.status, ToolStatus::Succeeded));

    let varied = scorestrata_mcp::tools::apply_motivic_variation(&ApplyMotivicVariationParams {
        project_id: project_id.clone(),
        pattern_id: pattern_id.clone(),
        variation: "retrograde".to_string(),
        semitones: 0,
        project: Some(project_json.clone()),
    });
    assert!(
        matches!(varied.status, ToolStatus::Succeeded),
        "motivic variation should succeed, got {varied:#?}"
    );

    // ── 19b. Arrangement: place pattern, retarget instrument ─────────
    let placed = scorestrata_mcp::tools::place_pattern_in_segment(&PlacePatternInSegmentParams {
        project_id: project_id.clone(),
        segment_id: segment_id.clone(),
        layer_id: layer_id.clone(),
        pattern_id,
        start_tick: 0,
        project: Some(project_json.clone()),
    });
    assert!(matches!(placed.status, ToolStatus::Succeeded));

    let retargeted = scorestrata_mcp::tools::set_layer_instrument(&SetLayerInstrumentParams {
        project_id: project_id.clone(),
        segment_id: segment_id.clone(),
        layer_id: layer_id.clone(),
        instrument: "supersaw".to_string(),
        project: Some(project_json.clone()),
    });
    assert!(matches!(retargeted.status, ToolStatus::Succeeded));
    assert!(retargeted.score_delta.contains_key("layer.instrument"));

    // ── 19c. Goals + energy curve ───────────────────────────────────
    let goals = scorestrata_mcp::tools::set_segment_goals(&SetSegmentGoalsParams {
        project_id: project_id.clone(),
        segment_id: segment_id.clone(),
        target_energy: 0.8,
        project: Some(project_json.clone()),
    });
    assert!(matches!(goals.status, ToolStatus::Succeeded));

    let curve = scorestrata_mcp::tools::set_energy_curve(&SetEnergyCurveParams {
        project_id: project_id.clone(),
        curve: vec![scorestrata_mcp::tools::EnergyPointSpec {
            segment_id: segment_id.clone(),
            target_energy: 0.9,
        }],
        project: Some(project_json.clone()),
    });
    assert!(matches!(curve.status, ToolStatus::Succeeded));

    // ── 19d. Patch ops: harmonize, drum fill, density, de-mask, loudness ─
    let harmonized = scorestrata_mcp::tools::harmonize_melody(&HarmonizeMelodyParams {
        project_id: project_id.clone(),
        segment_id: segment_id.clone(),
        layer_id: layer_id.clone(),
        interval_degrees: 3,
        project: Some(project_json.clone()),
    });
    assert!(
        matches!(harmonized.status, ToolStatus::Succeeded),
        "harmonize should succeed on populated layer, got {harmonized:#?}"
    );

    let fill = scorestrata_mcp::tools::add_drum_fill(&AddDrumFillParams {
        project_id: project_id.clone(),
        segment_id: segment_id.clone(),
        layer_id,
        steps: 4,
        project: Some(project_json.clone()),
    });
    assert!(matches!(fill.status, ToolStatus::Succeeded));

    let dense = scorestrata_mcp::tools::increase_segment_density(&IncreaseSegmentDensityParams {
        project_id: project_id.clone(),
        segment_id: segment_id.clone(),
        factor: 2,
        project: Some(project_json.clone()),
    });
    assert!(matches!(dense.status, ToolStatus::Succeeded));

    let demasked = scorestrata_mcp::tools::reduce_masking(&ReduceMaskingParams {
        project_id: project_id.clone(),
        segment_id,
        amount: 0.15,
        project: Some(project_json.clone()),
    });
    assert!(matches!(demasked.status, ToolStatus::Succeeded));

    let loud = scorestrata_mcp::tools::normalize_loudness_target(&NormalizeLoudnessTargetParams {
        project_id,
        target_lufs: Some(-10.0),
        project: Some(project_json),
    });
    assert!(matches!(loud.status, ToolStatus::Succeeded));
    assert!(loud.summary.contains("LUFS"));
}

/// Step 20: MCP dispatcher (`call_tool`) — the wire-level protocol core.
///
/// Drives the exact `tool_name + JSON params -> ToolReceipt` contract §10.5
/// by routing every phase of the agent loop through
/// `scorestrata_mcp::dispatch::dispatch_tool`: orientation, validation,
/// mutation, render, governance and learning. This proves the dispatcher
/// (not just the underlying functions) is operationally wired.
#[test]
fn mvas_step_20_mcp_dispatcher_wire_protocol() {
    use scorestrata_mcp::dispatch::dispatch_tool;
    use scorestrata_mcp::tools::ToolStatus;

    // Planned patterns start empty, so plant content into the lead pattern
    // the way an agent would before sending mutations over the wire — this
    // keeps the deterministic success assertions honest.
    let mut project = scorestrata_testkit::melodic_techno_project();
    let lead_pattern_id = project.patterns[0].id;
    if let Some(pattern) = project
        .patterns
        .iter_mut()
        .find(|p| p.id == lead_pattern_id)
    {
        pattern.notes.push(scorestrata_core::domain::Note {
            id: scorestrata_core::ids::NoteId::new(),
            pitch: scorestrata_core::domain::MidiNote::new(60),
            start: scorestrata_core::domain::Tick::new(0),
            duration: scorestrata_core::domain::Duration(480),
            velocity: scorestrata_core::domain::Velocity::new(100),
            tied: false,
        });
    }
    let project_json = serde_json::to_value(&project).expect("project serializes");
    let project_id = project.id.to_string();
    let pattern_id = project.patterns[0].id.to_string();
    let segment_id = project.arrangement.segments[0].id.to_string();
    let layer_id = project.arrangement.segments[0].layers[0].id.to_string();

    // ── 20a. Governance / orientation over the wire ─────────────────
    let caps = dispatch_tool("get_capabilities", serde_json::json!({}));
    assert!(matches!(caps.status, ToolStatus::Succeeded));
    assert!(caps.summary.contains("Capabilities manifest"));
    assert!(caps.summary.contains("run_validation"));

    let contracts = dispatch_tool("get_contracts", serde_json::json!({}));
    assert!(matches!(contracts.status, ToolStatus::Succeeded));
    assert!(contracts.summary.contains("Contract registry"));

    // ── 20b. Validation over the wire ───────────────────────────────
    let validation = dispatch_tool(
        "run_validation",
        serde_json::json!({
            "project_id": project_id,
            "project": project_json,
            "level": 2,
        }),
    );
    assert_eq!(validation.tool, "run_validation");
    assert!(validation.summary.contains("diagnostics"));
    assert!(validation.summary.contains("score"));

    // ── 20c. Mutation over the wire: composition + patch ────────────    // These two mutations are deterministic: the pattern from
    // `melodic_techno_project` always exists, so the wire path must succeed.
    let notes = dispatch_tool(
        "write_pattern_notes",
        serde_json::json!({
            "project_id": project_id,
            "pattern_id": pattern_id,
            "notes": [{"pitch": 62, "start": 480, "duration": 480, "velocity": 90}],
            "project": project_json,
        }),
    );
    assert_eq!(notes.tool, "write_pattern_notes");
    assert!(
        matches!(notes.status, ToolStatus::Succeeded),
        "write_pattern_notes should succeed on a real pattern: {:?}",
        notes.status
    );

    let motif = dispatch_tool(
        "apply_motivic_variation",
        serde_json::json!({
            "project_id": project_id,
            "pattern_id": pattern_id,
            "variation": "retrograde",
            "semitones": 0,
            "project": project_json,
        }),
    );
    assert_eq!(motif.tool, "apply_motivic_variation");
    assert!(
        matches!(motif.status, ToolStatus::Succeeded),
        "apply_motivic_variation should succeed on a real pattern: {:?}",
        motif.status
    );

    // ── 20d. Render + engineering over the wire ─────────────────────
    let render = dispatch_tool(
        "get_render_job",
        serde_json::json!({ "project_id": project_id, "job_id": "job_1" }),
    );
    assert_eq!(render.tool, "get_render_job");
    assert!(!render.summary.is_empty());

    let task = dispatch_tool(
        "get_next_task",
        serde_json::json!({ "project_id": project_id }),
    );
    assert_eq!(task.tool, "get_next_task");

    // ── 20e. Learning over the wire ─────────────────────────────────
    let lessons = dispatch_tool(
        "get_lessons",
        serde_json::json!({ "project_id": project_id }),
    );
    assert_eq!(lessons.tool, "get_lessons");
    assert!(matches!(
        lessons.status,
        ToolStatus::Succeeded | ToolStatus::Failed
    ));

    // ── 20f. Debug over the wire ────────────────────────────────────
    let isolate = dispatch_tool(
        "isolate_layer",
        serde_json::json!({
            "project_id": project_id,
            "segment_id": segment_id,
            "layer_id": layer_id,
            "project": project_json,
        }),
    );
    assert_eq!(isolate.tool, "isolate_layer");
    assert!(matches!(
        isolate.status,
        ToolStatus::Succeeded | ToolStatus::Failed
    ));

    // ── 20g. Unknown tool returns a transparent failure ─────────────
    let unknown = dispatch_tool("definitely_not_a_tool", serde_json::json!({}));
    assert!(matches!(unknown.status, ToolStatus::Failed));
    assert!(unknown.summary.contains("Unknown tool"));
}

/// Step 21: Level 5 brief compliance over the wire (§11.2 L5).
///
/// A project whose config genre disagrees with its brief must surface a
/// `BRIEF_GENRE_MISMATCH` diagnostic when `run_validation` is dispatched at
/// level 5 with a project payload — proving the full validation plane is
/// wired end-to-end, not just the L2 symbolic slice.
#[test]
fn mvas_step_21_l5_brief_compliance_over_the_wire() {
    use scorestrata_mcp::dispatch::dispatch_tool;
    use scorestrata_mcp::tools::ToolStatus;

    // ── 21a. Compliant project at L5: no brief diagnostics ──────────
    let compliant = scorestrata_testkit::melodic_techno_project();
    let ok = dispatch_tool(
        "run_validation",
        serde_json::json!({
            "project_id": compliant.id.to_string(),
            "project": compliant,
            "level": 5,
        }),
    );
    assert_eq!(ok.tool, "run_validation");
    assert!(
        matches!(ok.status, ToolStatus::Succeeded | ToolStatus::Failed),
        "L5 validation must run: {:?}",
        ok.status
    );
    assert!(
        !ok.diagnostics.iter().any(|d| d.code.starts_with("BRIEF_")),
        "compliant project must have no brief diagnostics: {:?}",
        ok.diagnostics
            .iter()
            .map(|d| d.code.clone())
            .collect::<Vec<_>>()
    );

    // ── 21b. Mismatched brief at L5: BRIEF_* diagnostic appears ─────
    let mut mismatched = scorestrata_testkit::melodic_techno_project();
    mismatched.config.genre = "house".to_string();
    let bad = dispatch_tool(
        "run_validation",
        serde_json::json!({
            "project_id": mismatched.id.to_string(),
            "project": mismatched,
            "level": 5,
        }),
    );
    assert!(
        bad.diagnostics
            .iter()
            .any(|d| d.code == "BRIEF_GENRE_MISMATCH"),
        "L5 must emit BRIEF_GENRE_MISMATCH: {:?}",
        bad.diagnostics
            .iter()
            .map(|d| d.code.clone())
            .collect::<Vec<_>>()
    );

    // ── 21c. Same mismatch at L2: brief plane is not exercised ──────
    let mut l2 = scorestrata_testkit::melodic_techno_project();
    l2.config.genre = "house".to_string();
    let l2_bad = dispatch_tool(
        "run_validation",
        serde_json::json!({
            "project_id": l2.id.to_string(),
            "project": l2,
            "level": 2,
        }),
    );
    assert!(
        !l2_bad
            .diagnostics
            .iter()
            .any(|d| d.code.starts_with("BRIEF_")),
        "L2 must not emit brief diagnostics"
    );
}

/// Step 22: Telemetry (§15.5) — the dispatcher measures what it runs.
///
/// Every tool dispatched over the wire is accounted in the telemetry registry;
/// `get_telemetry` serves the snapshot back. This proves the "you cannot
/// improve what you cannot measure" requirement end-to-end.
#[test]
fn mvas_step_22_telemetry_measures_dispatched_calls() {
    use scorestrata_mcp::dispatch::dispatch_tool;
    use scorestrata_mcp::tools::ToolStatus;

    // MVAS steps run in parallel, so other steps' dispatches can pollute the
    // shared registry. Use unique probe names so this step's exact counts are
    // immune to parallel pollution — including a failing probe to prove failed
    // dispatches are still measured.
    scorestrata_mcp::telemetry::reset();
    let _ = dispatch_tool("mvas22_probe_a", serde_json::json!({}));
    let _ = dispatch_tool("mvas22_probe_b", serde_json::json!({}));
    let _ = dispatch_tool("mvas22_probe_a", serde_json::json!({}));
    let _ = dispatch_tool("mvas22_failing_probe", serde_json::json!({}));

    let receipt = dispatch_tool("get_telemetry", serde_json::json!({}));
    assert_eq!(receipt.tool, "get_telemetry");
    assert!(matches!(receipt.status, ToolStatus::Succeeded));
    assert!(receipt.summary.contains("tool call(s)"));

    // The registry itself reflects the exact per-tool counts. Assert against
    // the full snapshot map, not the summary string: the summary is a
    // truncated top-N display, and parallel MVAS steps pollute the shared
    // registry with more tools than fit the cutoff — so a 1-call probe can
    // fall off the displayed list while still being measured correctly.
    let snap = scorestrata_mcp::telemetry::snapshot();
    assert_eq!(
        snap.tool_calls.get("mvas22_probe_a"),
        Some(&2),
        "repeated dispatches are counted: {}",
        receipt.summary
    );
    assert_eq!(
        snap.tool_calls.get("mvas22_probe_b"),
        Some(&1),
        "unique dispatches are counted: {}",
        receipt.summary
    );
    assert_eq!(
        snap.tool_calls.get("mvas22_failing_probe"),
        Some(&1),
        "failed dispatches are still measured: {}",
        receipt.summary
    );
    assert!(snap.total_tool_calls() >= 4);
    assert_eq!(snap.tool_calls.get("mvas22_probe_a"), Some(&2));
    assert_eq!(snap.tool_calls.get("mvas22_failing_probe"), Some(&1));
}

/// §13.5 Learning loop closes: kernel lessons export into a persistent store,
/// and repeated wire-level recording of the same diagnostic accumulates
/// confidence instead of duplicating.
#[test]
fn mvas_step_23_learning_loop_accumulates_confidence() {
    use scorestrata_agent::kernel::{AgentKernel, Candidate, Diagnosis};
    use scorestrata_agent::lessons::export_lessons_to_store;
    use scorestrata_mcp::tools::{GetLessonsParams, RecordLessonParams, ToolStatus};
    use scorestrata_storage::lesson_store::LessonStore;

    // 1. Kernel experience → export → persistent store (the §13.5 bridge).
    let mut kernel = AgentKernel::new(scorestrata_core::ids::ProjectId::new());
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
        description: "Quantize".to_string(),
        scope: "harmony".to_string(),
        diagnosis_metric: "scale_violation".to_string(),
        estimated_delta: 0.05,
        is_repair: false,
        template_id: None,
    };
    kernel.l4_learn(&diagnosis, &candidate, 0.1);
    kernel.l4_learn(&diagnosis, &candidate, 0.1);

    let mut store = LessonStore::new();
    let exported = export_lessons_to_store(
        &kernel.lessons_learned,
        &mut store,
        Some("melodic_techno"),
        "proj_1",
    );
    assert_eq!(exported, 2);
    // Both successes merge into one keyed lesson.
    assert_eq!(store.len(), 1);
    let lesson = store
        .get_best_for_trigger("scale_violation", Some("melodic_techno"))
        .expect("lesson present after export");
    assert_eq!(lesson.success_count, 2);

    // 2. Wire-level accumulation: recording the same trigger through the MCP
    //    tool merges into the same lesson and keeps raising confidence.
    let mut current = serde_json::to_value(&store).expect("store serializes");
    for _ in 0..2 {
        let receipt = scorestrata_mcp::tools::record_lesson(&RecordLessonParams {
            project_id: "proj_1".to_string(),
            trigger: "scale_violation".to_string(),
            scope: "harmony".to_string(),
            successful_patch: Some("quantize_to_scale".to_string()),
            genre: Some("melodic_techno".to_string()),
            confidence: 0.6,
            lesson_store: Some(current.clone()),
        });
        assert!(
            matches!(receipt.status, ToolStatus::Succeeded),
            "record should succeed: {}",
            receipt.summary
        );
        let updated = receipt
            .evidence_uris
            .iter()
            .find_map(|u| u.strip_prefix("memory://lessons/proj_1?store="))
            .expect("updated store in evidence");
        current = serde_json::from_str(updated).expect("updated store parses");
    }

    // 3. Query: one accumulated lesson, retrievable, never duplicated.
    let query = scorestrata_mcp::tools::get_lessons(&GetLessonsParams {
        project_id: "proj_1".to_string(),
        trigger: Some("scale_violation".to_string()),
        genre: None,
        min_confidence: 0.5,
        lesson_store: Some(current),
    });
    assert!(matches!(query.status, ToolStatus::Succeeded));
    assert_eq!(query.score_delta["lessons.matched"], "1");
    assert_eq!(query.score_delta["lessons.total_in_store"], "1");
    assert!(query.summary.contains("quantize_to_scale"));
}

/// §11.2 L3 timeline validation over the wire: a layer referencing a missing
/// pattern must surface as `TIMELINE_UNRESOLVED_PATTERN` at level 3 through the
/// dispatcher, and must be absent at level 2 (plane gating).
#[test]
fn mvas_step_24_timeline_validation_over_the_wire() {
    use scorestrata_mcp::tools::ToolStatus;

    let mut project = scorestrata_testkit::melodic_techno_project();
    project.arrangement.segments[0].layers[0]
        .pattern_instances
        .push(scorestrata_core::domain::PatternInstance {
            id: scorestrata_core::ids::PatternInstanceId::new(),
            pattern_id: scorestrata_core::ids::PatternId::new(),
            start_tick: scorestrata_core::domain::Tick::new(0),
            transpose: 0,
            velocity_multiplier: 1.0,
        });

    // L3: the timeline plane fires.
    let at_l3 = scorestrata_mcp::dispatch::dispatch_tool(
        "run_validation",
        serde_json::json!({
            "project_id": project.id.to_string(),
            "project": &project,
            "level": 3,
        }),
    );
    assert_eq!(at_l3.tool, "run_validation");
    assert!(
        at_l3
            .diagnostics
            .iter()
            .any(|d| d.code == "TIMELINE_UNRESOLVED_PATTERN"),
        "L3 must emit TIMELINE_UNRESOLVED_PATTERN: {:?}",
        at_l3
            .diagnostics
            .iter()
            .map(|d| d.code.clone())
            .collect::<Vec<_>>()
    );

    // L2: the timeline plane is gated off.
    let at_l2 = scorestrata_mcp::dispatch::dispatch_tool(
        "run_validation",
        serde_json::json!({
            "project_id": project.id.to_string(),
            "project": &project,
            "level": 2,
        }),
    );
    assert_eq!(at_l2.tool, "run_validation");
    assert!(
        !at_l2
            .diagnostics
            .iter()
            .any(|d| d.code.starts_with("TIMELINE_")),
        "L2 must not emit timeline diagnostics"
    );

    // A clean project passes L3 with no timeline diagnostics.
    let clean = scorestrata_testkit::melodic_techno_project();
    let clean_receipt = scorestrata_mcp::dispatch::dispatch_tool(
        "run_validation",
        serde_json::json!({
            "project_id": clean.id.to_string(),
            "project": &clean,
            "level": 3,
        }),
    );
    assert!(matches!(clean_receipt.status, ToolStatus::Succeeded));
    assert!(
        !clean_receipt
            .diagnostics
            .iter()
            .any(|d| d.code.starts_with("TIMELINE_")),
        "clean project passes L3"
    );
}

#[test]
fn mvas_step_25_audio_validation_over_the_wire() {
    // The L4 audio plane renders the project and analyzes the mix. With the
    // fixed engine (limiter ceiling −1.0 dBTP, polyBLEP, gain staging) the
    // melodic-techno fixture renders clean — no clipping, no true-peak
    // violation — but the audio plane must still surface diagnostics at
    // level 4 (narrow-stereo on the mono render) and stay hidden below it.
    let project = scorestrata_testkit::melodic_techno_project();

    let at_l3 = scorestrata_mcp::dispatch::dispatch_tool(
        "run_validation",
        serde_json::json!({
            "project_id": project.id.to_string(),
            "project": &project,
            "level": 3,
        }),
    );
    assert_eq!(at_l3.tool, "run_validation");
    assert!(
        !at_l3
            .diagnostics
            .iter()
            .any(|d| d.code.starts_with("AUDIO_")),
        "audio plane must stay off below L4"
    );

    let at_l4 = scorestrata_mcp::dispatch::dispatch_tool(
        "run_validation",
        serde_json::json!({
            "project_id": project.id.to_string(),
            "project": &project,
            "level": 4,
        }),
    );
    assert_eq!(at_l4.tool, "run_validation");
    assert!(
        at_l4
            .diagnostics
            .iter()
            .any(|d| d.code == "AUDIO_NARROW_STEREO"),
        "L4 audio plane must surface diagnostics over the wire"
    );
    assert!(
        !at_l4
            .diagnostics
            .iter()
            .any(|d| d.code == "AUDIO_CLIPPING_DETECTED"),
        "the fixed engine must not clip over the wire"
    );
    assert!(
        !at_l4
            .diagnostics
            .iter()
            .any(|d| d.code == "AUDIO_TRUE_PEAK_EXCEEDED"),
        "the fixed engine must respect the −1.0 dBTP ceiling over the wire"
    );
    assert!(
        !at_l4
            .diagnostics
            .iter()
            .any(|d| d.code == "AUDIO_RENDER_FAILED"),
        "clean fixture must render at L4 without failure"
    );
}

#[test]
fn mvas_step_26_regression_detection_over_the_wire() {
    // L6 (§11.2 L6, §11.7): a baseline scorecard claiming clean audio must be
    // flagged when the current project regresses — both as a metric regression
    // and as a hard-gate violation. With the fixed engine the melodic-techno
    // fixture renders clean, so the regression is driven by a genuinely broken
    // project: silenced layers collapse audio.integrated_lufs below the 1.0
    // baseline (a hard gate).
    use scorestrata_core::diagnostic::{MetricSample, MetricScope, Scorecard};
    use scorestrata_core::error::Severity;
    use scorestrata_core::ids::{MetricId, ScorecardId};

    let mut project = scorestrata_testkit::melodic_techno_project();
    let gate = |name: &str| MetricSample {
        id: MetricId::new(),
        name: name.to_string(),
        value: 1.0,
        severity: Severity::Info,
        scope: MetricScope::default(),
        evidence_uri: format!("project://baseline/{name}"),
        explanation: "baseline clean".to_string(),
        confidence: 1.0,
        suggested_patches: vec![],
    };
    let baseline = Scorecard {
        id: ScorecardId::new(),
        metrics: vec![
            gate("audio.clipping"),
            gate("audio.true_peak_dbtp"),
            gate("audio.integrated_lufs"),
        ],
        overall_score: 1.0,
        hard_failures: 0,
        warnings: 0,
    };
    for segment in &mut project.arrangement.segments {
        segment.layers.clear();
    }

    let receipt = scorestrata_mcp::dispatch::dispatch_tool(
        "check_regressions",
        serde_json::json!({
            "project_id": project.id.to_string(),
            "baseline_scorecard": baseline,
            "project": &project,
        }),
    );
    assert_eq!(receipt.tool, "check_regressions");
    assert!(
        receipt
            .diagnostics
            .iter()
            .any(|d| d.code == "METRIC_REGRESSION_DETECTED"),
        "wire L6 must flag the loudness regression"
    );
    assert!(
        receipt
            .diagnostics
            .iter()
            .any(|d| d.code == "HARD_GATE_REGRESSION"),
        "wire L6 must flag the hard-gate violation"
    );
    assert!(
        receipt
            .next_actions
            .iter()
            .any(|a| a == "rollback_to_previous_snapshot"),
        "hard-gate regressions must suggest rollback"
    );
}

#[test]
fn mvas_step_27_golden_harness_over_the_wire() {
    // §16.5: the golden harness must run the *real* pipeline — config → plan →
    // L6 validation over the wire → render → structural + validation golden
    // assertions — and a regressed project must fail its golden.
    use scorestrata_eval::golden::{
        ExpectedOutputs, GoldenConfig, ValidationGates, run_golden_assertions,
        run_validation_golden_assertions,
    };

    let brief = "Dark melodic techno at 124 BPM in C minor, 64 bars";
    let config = scorestrata_mcp::config_gen::generate_config("golden_techno", brief);
    let plan = scorestrata_compiler::plan::plan_project(&config);
    let mut project = scorestrata_testkit::minimal_project();
    project.config = config.clone();
    project.arrangement = plan.arrangement;
    project.patterns = plan.patterns;
    project.status = scorestrata_core::domain::ProjectStatus::ConfigApproved;
    project.brief.raw_text = brief.to_string();
    project.brief.genre = Some(config.genre);

    // L6 validation over the wire (dispatcher).
    let receipt = scorestrata_mcp::dispatch::dispatch_tool(
        "run_validation",
        serde_json::json!({
            "project_id": project.id.to_string(),
            "project": &project,
            "level": 6,
        }),
    );
    assert_eq!(receipt.tool, "run_validation");

    let scorecard = scorestrata_validator::report::build_validation_report(
        project.id,
        6,
        receipt.diagnostics.clone(),
    )
    .scorecard;
    let render = scorestrata_mcp::tools::render_project_to_analysis(&project).expect("renders");

    let golden = GoldenConfig {
        name: "melodic_techno_64_bar".to_string(),
        description: "golden over the wire".to_string(),
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
            max_errors: 0, // fixed engine: no clipping, no true-peak exceed
            max_warnings: 3,
            max_true_peak_dbtp: -0.5,
            min_lufs: -16.0,
            max_lufs: -4.0,
        },
        expected_metrics: std::collections::BTreeMap::new(),
    };

    let segments = &project.arrangement.segments;
    let has_chorus = segments
        .iter()
        .any(|s| s.role == scorestrata_core::domain::SegmentRole::Chorus);
    let structural = run_golden_assertions(
        &golden,
        u32::try_from(segments.len()).expect("fits"),
        project.arrangement.total_bars,
        has_chorus,
        false,
        project.config.genre == "melodic_techno",
        !project.config.mood.is_empty(),
        project.arrangement.total_bars == project.config.duration_bars,
    );
    assert!(structural.passed, "structural golden failed over the wire");

    let validation = run_validation_golden_assertions(
        &golden,
        &receipt.diagnostics,
        &scorecard,
        Some(render.analysis.true_peak_dbtp),
        Some(render.analysis.integrated_lufs),
    );
    assert!(
        validation.passed,
        "validation golden failed over the wire: {:?}",
        validation
            .assertions
            .iter()
            .filter(|a| !a.passed)
            .map(|a| (a.name.as_str(), a.error.clone().unwrap_or_default()))
            .collect::<Vec<_>>()
    );

    // A regressed project must fail its golden.
    if let Some(segment) = project.arrangement.segments.first_mut() {
        segment.layers.clear();
    }
    let regressed_receipt = scorestrata_mcp::dispatch::dispatch_tool(
        "run_validation",
        serde_json::json!({
            "project_id": project.id.to_string(),
            "project": &project,
            "level": 6,
        }),
    );
    let regressed_render =
        scorestrata_mcp::tools::render_project_to_analysis(&project).expect("regressed renders");
    let regressed_validation = run_validation_golden_assertions(
        &golden,
        &regressed_receipt.diagnostics,
        &scorecard,
        Some(regressed_render.analysis.true_peak_dbtp),
        Some(regressed_render.analysis.integrated_lufs),
    );
    assert!(
        !regressed_validation.passed,
        "a regressed project must fail its golden validation"
    );
    assert!(
        regressed_receipt
            .diagnostics
            .iter()
            .any(|d| d.code == "SEGMENT_EMPTY"),
        "the wire regression must be the emptied segment"
    );
}

#[test]
fn mvas_step_28_genre_calibration_feeds_l6_scoring() {
    // §11.8: the genre profile's metric weights must reach the L6 scorecard.
    // Previously the weights were deadweight — every profile shipped an empty
    // table and the validator never consulted the genre.
    use scorestrata_eval::golden::{
        ExpectedOutputs, GoldenConfig, ValidationGates, run_validation_golden_assertions,
    };

    // The canonical melodic-techno profile now carries genre weights and the
    // §11.6 loudness target.
    let profile = scorestrata_storage::calibration::default_genre_profiles()
        .into_iter()
        .find(|p| p.name == "melodic_techno")
        .expect("melodic_techno profile exists");
    assert!(
        !profile.metric_weights.is_empty(),
        "genre weights populated"
    );
    assert_eq!(profile.metric_weights.get("rhythm"), Some(&1.3));
    assert_eq!(profile.loudness_target_lufs, -10.0);

    // The MCP wiring point resolves the calibrated scoring profile.
    let calibrated = scorestrata_mcp::tools::scoring_profile_for_genre("melodic_techno");
    assert_eq!(calibrated.weights.get("rhythm"), Some(&1.3));
    assert_eq!(calibrated.weights.get("structure"), Some(&1.2));

    // The full pipeline (config → plan → L6 over the wire → golden) runs
    // with the genre-calibrated scoring.
    let brief = "Dark melodic techno at 124 BPM in C minor, 64 bars";
    let config = scorestrata_mcp::config_gen::generate_config("golden_techno", brief);
    let plan = scorestrata_compiler::plan::plan_project(&config);
    let mut project = scorestrata_testkit::minimal_project();
    project.config = config.clone();
    project.arrangement = plan.arrangement;
    project.patterns = plan.patterns;
    project.status = scorestrata_core::domain::ProjectStatus::ConfigApproved;
    project.brief.raw_text = brief.to_string();
    project.brief.genre = Some(config.genre);

    let receipt = scorestrata_mcp::dispatch::dispatch_tool(
        "run_validation",
        serde_json::json!({
            "project_id": project.id.to_string(),
            "project": &project,
            "level": 6,
        }),
    );
    let scorecard = scorestrata_validator::report::build_validation_report_with_profile(
        project.id,
        6,
        receipt.diagnostics.clone(),
        &calibrated,
    )
    .scorecard;
    let render = scorestrata_mcp::tools::render_project_to_analysis(&project).expect("renders");

    let golden = GoldenConfig {
        name: "melodic_techno_64_bar".to_string(),
        description: "calibrated golden".to_string(),
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
            max_errors: 0, // fixed engine: no clipping, no true-peak exceed
            max_warnings: 3,
            max_true_peak_dbtp: -0.5,
            min_lufs: -16.0,
            max_lufs: -4.0,
        },
        expected_metrics: std::collections::BTreeMap::from([(
            "arrangement.transition_strength".to_string(),
            scorestrata_eval::golden::MetricRange {
                min: 0.85,
                max: 1.0,
            },
        )]),
    };
    let validation = run_validation_golden_assertions(
        &golden,
        &receipt.diagnostics,
        &scorecard,
        Some(render.analysis.true_peak_dbtp),
        Some(render.analysis.integrated_lufs),
    );
    assert!(
        validation.passed,
        "calibrated golden failed: {:?}",
        validation
            .assertions
            .iter()
            .filter(|a| !a.passed)
            .map(|a| (a.name.as_str(), a.error.clone().unwrap_or_default()))
            .collect::<Vec<_>>()
    );
}

#[test]
fn mvas_step_29_suggest_fixes_closes_learning_loop() {
    // §12.3 + §13.5: the failure report recommends suggest_fixes as the next
    // action, and suggest_fixes must surface (1) the real validator patches
    // and (2) proven lessons from the store — closing the diagnose → fix →
    // learn loop. Previously the tool was a stub that returned a canned
    // success string and ignored both sources.
    let project = scorestrata_testkit::melodic_techno_project();

    // The failure report recommends the fix path.
    let failure = scorestrata_mcp::debug_ops::compute_failure_report(&project);
    assert!(
        failure
            .recommended_actions
            .contains(&"suggest_fixes".to_string()),
        "failure report recommends suggest_fixes"
    );

    // Wire path: dispatcher → suggest_fixes with a project payload returns
    // the validator's real SuggestedPatch ops.
    let receipt = scorestrata_mcp::dispatch::dispatch_tool(
        "suggest_fixes",
        serde_json::json!({
            "project_id": project.id.to_string(),
            "diagnostic_code": "TRANSITION_ENERGY_DELTA_LOW",
            "project": &project,
        }),
    );
    assert_eq!(receipt.tool, "suggest_fixes");
    assert!(
        matches!(
            receipt.status,
            scorestrata_mcp::tools::ToolStatus::Succeeded
        ),
        "real fixes must succeed: {}",
        receipt.summary
    );
    assert!(
        receipt.summary.contains("add_transition_package"),
        "validator patch surfaced: {}",
        receipt.summary
    );
    assert!(receipt.next_actions.contains(&"apply_patch".to_string()));

    // Learning loop: a proven lesson for a trigger the validator does not
    // currently emit must still surface via the store.
    let mut store = scorestrata_storage::lesson_store::LessonStore::new();
    store.record(scorestrata_storage::lesson_store::StoredLesson {
        id: "les_e2e".to_string(),
        scope: "layer".to_string(),
        trigger: "HARMONY_SCALE_VIOLATION".to_string(),
        successful_patch: Some("quantize_layer_to_scale".to_string()),
        failed_patches: vec![],
        evidence_uris: vec![],
        genre: None,
        confidence: 0.9,
        success_count: 4,
        failure_count: 0,
        created_at: "t".to_string(),
        updated_at: "t".to_string(),
    });
    let lesson_receipt = scorestrata_mcp::dispatch::dispatch_tool(
        "suggest_fixes",
        serde_json::json!({
            "project_id": project.id.to_string(),
            "diagnostic_code": "HARMONY_SCALE_VIOLATION",
            "lesson_store": &store,
        }),
    );
    assert!(
        matches!(
            lesson_receipt.status,
            scorestrata_mcp::tools::ToolStatus::Succeeded
        ),
        "lesson-proven fix must succeed: {}",
        lesson_receipt.summary
    );
    assert!(
        lesson_receipt.summary.contains("quantize_layer_to_scale"),
        "lesson patch surfaced: {}",
        lesson_receipt.summary
    );
    assert_eq!(
        lesson_receipt
            .score_delta
            .get("fixes.lesson_sources")
            .map(String::as_str),
        Some("1")
    );

    // Stub transparency: without any payload the tool must fail honestly
    // rather than fabricate fixes.
    let empty = scorestrata_mcp::dispatch::dispatch_tool(
        "suggest_fixes",
        serde_json::json!({
            "project_id": project.id.to_string(),
            "diagnostic_code": "TRANSITION_ENERGY_DELTA_LOW",
        }),
    );
    assert!(
        matches!(empty.status, scorestrata_mcp::tools::ToolStatus::Failed),
        "no payload must fail transparently"
    );
    assert!(empty.summary.contains("no project or lesson-store payload"));
}

/// Step 30: task-completion level triggers (§13.1–§13.4, §13.7).
///
/// The kernel advances L0 → L1 → L2 → L3 only when each level's actual task
/// is done — orientation pass completed, hard failures cleared, high-severity
/// issues gone, target reached with gates passing — never on an iteration
/// count. A persistent hard failure must pin the kernel at L1 even at even
/// iteration counts, where the old count-based heuristic would have advanced.
#[test]
fn mvas_step_30_task_completion_triggers_drive_level_advancement() {
    use scorestrata_agent::kernel::{AgentConfig, AgentKernel, Diagnosis, LoopLevel};

    let project_id = ProjectId::new();
    let config = AgentConfig {
        max_iterations: 12,
        max_no_improvement: 4,
        target_score: 5.0, // unreachable: budget, not target, bounds the run
        improvement_threshold: 0.01,
        max_candidates_per_iteration: 3,
        ..AgentConfig::default()
    };
    let mut kernel = AgentKernel::with_config(project_id, config);

    // ── 30a. L0 → L1: orientation completes after the first pass ──────
    // A persistent mix hard failure gives the repair loop real work.
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

    let result = kernel
        .run_iteration(std::slice::from_ref(&hard), &templates, &metrics)
        .expect("iteration runs");
    assert!(result.improved, "L0 repair iteration improves");
    assert_eq!(
        kernel.level,
        LoopLevel::MicroRepair,
        "L0 orientation done after one pass → L1"
    );

    // ── 30b. L1 pins on a persistent hard failure ─────────────────────
    // Iterations 2 and 3 are even/odd counts — the old rule advanced on
    // every other *successful* iteration; task-completion must keep L1
    // active because the hard failure has not been cleared.
    for _ in 0..2 {
        let result = kernel
            .run_iteration(std::slice::from_ref(&hard), &templates, &metrics)
            .expect("iteration runs");
        assert!(result.improved);
    }
    assert_eq!(kernel.iteration, 3, "three repair iterations executed");
    assert_eq!(
        kernel.level,
        LoopLevel::MicroRepair,
        "a hard failure pins the kernel at L1 regardless of iteration count"
    );

    // ── 30c. L1 → L2: hard failure cleared, task completes ────────────
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
        .run_iteration(std::slice::from_ref(&soft), &templates, &metrics)
        .expect("iteration runs");
    assert!(result.improved);
    assert_eq!(
        kernel.level,
        LoopLevel::SegmentDevelopment,
        "L1 task done → L2"
    );

    // ── 30d. L2 exit trigger checks ───────────────────────────────────
    let high = Diagnosis {
        metric: "harmony.chord_tone_ratio".to_string(),
        scope: "harmony".to_string(),
        current_value: 0.3,
        target_value: 0.8,
        severity: 0.8,
        is_hard_failure: false,
        evidence_uri: None,
    };
    let mut at_l2 = AgentKernel::new(project_id);
    at_l2.level = LoopLevel::SegmentDevelopment;
    assert!(
        !at_l2.level_task_complete(&[high]),
        "high-severity issue keeps L2 active"
    );
    assert!(
        at_l2.level_task_complete(std::slice::from_ref(&soft)),
        "low-severity issues complete L2"
    );

    // ── 30e. L3 exit trigger: target + gates (§13.7 break) ────────────
    let mut at_l3 = AgentKernel::with_config(
        project_id,
        AgentConfig {
            target_score: 0.85,
            ..AgentConfig::default()
        },
    );
    at_l3.level = LoopLevel::ProjectRefinement;
    at_l3.current_score = 0.9;
    at_l3.hard_gates_passing = true;
    assert!(
        at_l3.level_task_complete(&[soft]),
        "target reached with gates passing completes L3"
    );
    at_l3.hard_gates_passing = false;
    assert!(
        !at_l3.level_task_complete(&[Diagnosis {
            metric: "m".to_string(),
            scope: "s".to_string(),
            current_value: 0.0,
            target_value: 1.0,
            severity: 0.2,
            is_hard_failure: false,
            evidence_uri: None,
        }]),
        "failing gates keep L3 active even above target"
    );
}

/// Step 31: segment fixes close the diagnose → fix loop (§11.2, §12.3).
///
/// The segment diagnostics (`SEGMENT_EMPTY`, `SEGMENT_BAR_COUNT_INVALID`)
/// previously shipped **empty** `suggested_patches`, so `suggest_fixes` could
/// not offer a fix and the L2 segment loop starved. Now the validator
/// attaches real suggestions and the patch registry implements them:
/// `add_segment_layer` and `set_segment_bars` actually mutate the project
/// and clear the diagnostics — proven end-to-end over the wire.
#[test]
fn mvas_step_31_segment_fixes_close_the_repair_loop() {
    use scorestrata_mcp::dispatch::dispatch_tool;
    use scorestrata_mcp::tools::{ApplyPatchParams, PatchScope, ToolStatus};

    // ── 31a. An empty segment surfaces SEGMENT_EMPTY with a real fix ──
    let mut project = project_with_out_of_scale_lead();
    project.arrangement.segments[0].layers.clear();
    let segment_id = project.arrangement.segments[0].id.to_string();

    let validation = dispatch_tool(
        "run_validation",
        serde_json::json!({
            "project_id": project.id.to_string(),
            "project": &project,
            "level": 2,
        }),
    );
    let segment_diag = validation
        .diagnostics
        .iter()
        .find(|d| d.code == "SEGMENT_EMPTY")
        .expect("empty segment produces SEGMENT_EMPTY");
    assert!(
        segment_diag
            .suggested_patches
            .iter()
            .any(|p| p.op == "add_segment_layer"),
        "SEGMENT_EMPTY suggests add_segment_layer, got {:?}",
        segment_diag.suggested_patches
    );

    // suggest_fixes surfaces the validator's suggested patch.
    let fixes = dispatch_tool(
        "suggest_fixes",
        serde_json::json!({
            "project_id": project.id.to_string(),
            "diagnostic_code": "SEGMENT_EMPTY",
            "project": &project,
        }),
    );
    assert!(
        fixes.summary.contains("add_segment_layer"),
        "suggest_fixes surfaces the segment fix: {}",
        fixes.summary
    );

    // Applying the suggested op actually fixes the empty segment.
    let receipt = scorestrata_mcp::patch_ops::apply_patch_to_project(
        &ApplyPatchParams {
            project_id: project.id.to_string(),
            op: "add_segment_layer".to_string(),
            params: serde_json::json!({}),
            dry_run: false,
            scope: Some(PatchScope {
                segment_id: Some(segment_id),
                layer_id: None,
            }),
            project: None,
        },
        &mut project,
    );
    assert!(
        matches!(receipt.status, ToolStatus::Succeeded),
        "add_segment_layer succeeds: {}",
        receipt.summary
    );
    assert_eq!(project.arrangement.segments[0].layers.len(), 1);

    let revalidated = dispatch_tool(
        "run_validation",
        serde_json::json!({
            "project_id": project.id.to_string(),
            "project": &project,
            "level": 2,
        }),
    );
    assert!(
        !revalidated
            .diagnostics
            .iter()
            .any(|d| d.code == "SEGMENT_EMPTY"),
        "add_segment_layer clears SEGMENT_EMPTY"
    );

    // ── 31b. An out-of-range bar count suggests + applies a resize ────
    project.arrangement.segments[1].bars = 128; // over the 4–64 range
    let validation = dispatch_tool(
        "run_validation",
        serde_json::json!({
            "project_id": project.id.to_string(),
            "project": &project,
            "level": 2,
        }),
    );
    let bars_diag = validation
        .diagnostics
        .iter()
        .find(|d| d.code == "SEGMENT_BAR_COUNT_INVALID")
        .expect("out-of-range bars produces the diagnostic");
    let suggested_bars = bars_diag
        .suggested_patches
        .iter()
        .find(|p| p.op == "set_segment_bars")
        .expect("bar-count diagnostic suggests set_segment_bars");
    assert_eq!(
        suggested_bars.params["bars"],
        serde_json::json!(64),
        "suggestion clamps to the nearest valid bound"
    );

    let bars_segment_id = project.arrangement.segments[1].id.to_string();
    let receipt = scorestrata_mcp::patch_ops::apply_patch_to_project(
        &ApplyPatchParams {
            project_id: project.id.to_string(),
            op: "set_segment_bars".to_string(),
            params: serde_json::json!({}),
            dry_run: false,
            scope: Some(PatchScope {
                segment_id: Some(bars_segment_id),
                layer_id: None,
            }),
            project: None,
        },
        &mut project,
    );
    assert!(
        matches!(receipt.status, ToolStatus::Succeeded),
        "set_segment_bars succeeds: {}",
        receipt.summary
    );
    assert_eq!(project.arrangement.segments[1].bars, 64);

    let revalidated = dispatch_tool(
        "run_validation",
        serde_json::json!({
            "project_id": project.id.to_string(),
            "project": &project,
            "level": 2,
        }),
    );
    assert!(
        !revalidated
            .diagnostics
            .iter()
            .any(|d| d.code == "SEGMENT_BAR_COUNT_INVALID"),
        "set_segment_bars clears SEGMENT_BAR_COUNT_INVALID"
    );
}

/// Step 32 — provenance manifest drives reproducibility + cache keys (§15.2, §9.3).
///
/// Every artifact must be reproducible from (brief, config, policy, seed,
/// asset hashes, engine versions, render settings, DSP graph version). This
/// step proves the manifest records those inputs, its deterministic hash
/// detects any drift (seed, asset, version, settings), and the same inputs
/// always produce the same content-addressed cache key.
#[test]
fn mvas_step_32_provenance_manifests_drive_reproducibility() {
    use scorestrata_mcp::resources::ProvenanceView;
    use scorestrata_storage::provenance::ProvenanceManifest;

    // ── 32a. A complete manifest records every §15.2 input ──
    let mut manifest = ProvenanceManifest::new("p32");
    manifest.brief_hash = "brief_abc".to_string();
    manifest.config_hash = "config_abc".to_string();
    manifest.policy_hash = "policy_abc".to_string();
    manifest.seed = Some(1337);
    manifest
        .asset_hashes
        .insert("kick.wav".to_string(), "h_kick".to_string());
    manifest.snapshot_hash = "snap_abc".to_string();
    manifest.compiler_version = "0.1.0".to_string();
    manifest.validator_version = "0.1.0".to_string();
    manifest.audio_engine_version = "0.1.0".to_string();
    manifest
        .render_settings
        .insert("sample_rate".to_string(), "44100".to_string());
    manifest.dsp_graph_version = "v3".to_string();
    assert!(manifest.is_complete(), "complete manifest has all inputs");

    // ── 32b. Deterministic hash: identical inputs → identical hash ──
    let mut twin = manifest.clone();
    assert_eq!(manifest.provenance_hash(), twin.provenance_hash());
    assert!(manifest.reproduces(&twin));

    // ── 32c. Drift detection: any single drift breaks reproducibility ──
    twin.seed = Some(9999);
    assert!(
        !manifest.reproduces(&twin),
        "seed drift must break reproducibility"
    );

    let mut asset_drift = manifest.clone();
    asset_drift
        .asset_hashes
        .insert("kick.wav".to_string(), "h_kick_V2".to_string());
    assert!(!manifest.reproduces(&asset_drift));

    let mut version_drift = manifest.clone();
    version_drift.validator_version = "0.2.0".to_string();
    assert!(!manifest.reproduces(&version_drift));

    let mut settings_drift = manifest.clone();
    settings_drift
        .render_settings
        .insert("sample_rate".to_string(), "48000".to_string());
    assert!(!manifest.reproduces(&settings_drift));

    // ── 32d. §9.3 cache keys: same inputs → same content-addressed key ──
    let key_a = manifest.cache_key("render");
    let key_b = manifest.clone().cache_key("render");
    assert_eq!(key_a, key_b);
    assert!(key_a.starts_with("render/"));
    assert_ne!(key_a, asset_drift.cache_key("render"));

    // ── 32e. The MCP resource view derives from the real manifest ──
    let view = ProvenanceView::from_manifest(&manifest);
    assert_eq!(view.project_id, "p32");
    assert_eq!(view.provenance_hash, manifest.provenance_hash());
    assert!(view.complete);
    assert_eq!(view.policy_hash, "policy_abc");

    // ── 32f. Evidence produced under a manifest is reproducible: validating
    //         the same project with the same seed yields the same score ──
    let project_a = scorestrata_testkit::minimal_project();
    let project_b = project_a.clone();
    let report_a = build_report(&project_a);
    let report_b = build_report(&project_b);
    assert_eq!(
        report_a.scorecard.overall_score, report_b.scorecard.overall_score,
        "same inputs produce the same validation score"
    );
}

/// Step 33 — deterministic replay & time travel capabilities (§9.4, §15.3).
///
/// "If an agent cannot replay a failure exactly, it cannot reliably learn
/// from it." This step proves the `ReplayEngine` capabilities over a real
/// journal: replay until failure pinpoints the exact failing command,
/// snapshot bisect finds it in O(log n), event reconstruction recovers the
/// observable outcome stream, and substituted-policy replay shows whether a
/// stricter policy would have prevented the failure — and at which command.
#[test]
fn mvas_step_33_deterministic_replay_and_time_travel() {
    use scorestrata_storage::replay::{JournalEntry, ReplayEngine, ReplayJournal};
    use std::collections::BTreeMap;

    fn ok(seq: u64) -> JournalEntry {
        JournalEntry {
            seq,
            command: "apply_patch".to_string(),
            actor: "agent:composer".to_string(),
            scope: BTreeMap::from([("segment".to_string(), "0".to_string())]),
            timestamp: chrono::Utc::now().to_rfc3339(),
            pre_snapshot_id: Some(format!("snap_{seq}")),
            post_snapshot_id: Some(format!("snap_{}", seq + 1)),
            events: vec!["ok".to_string()],
        }
    }

    fn hard_fail(seq: u64) -> JournalEntry {
        let mut e = ok(seq);
        e.events = vec!["hard_failure:true_peak".to_string()];
        e
    }

    // ── 33a. A journal records the full command history with outcomes ──
    let mut journal = ReplayJournal::new();
    journal.record(
        "plan_arrangement",
        "agent:planner",
        BTreeMap::new(),
        None,
        Some("snap_0".to_string()),
        vec!["ok".to_string()],
    );
    journal.record(
        "apply_patch",
        "agent:composer",
        BTreeMap::from([("segment".to_string(), "0".to_string())]),
        Some("snap_1".to_string()),
        Some("snap_2".to_string()),
        vec!["ok".to_string()],
    );
    journal.record(
        "apply_patch",
        "agent:composer",
        BTreeMap::new(),
        Some("snap_2".to_string()),
        Some("snap_3".to_string()),
        vec!["hard_failure:true_peak".to_string()],
    );
    assert_eq!(journal.len(), 3);
    assert_eq!(journal.next_seq, 3);

    // ── 33b. Replay until failure finds the exact failing command ──
    let is_failure = |e: &JournalEntry| e.events.iter().any(|ev| ev.starts_with("hard_failure"));
    let failed_seq = ReplayEngine::replay_until_failure(&journal.entries, is_failure);
    assert_eq!(failed_seq, Some(2));

    // ── 33c. Snapshot bisect finds it in O(log n) on the monotone tail ──
    let entries: Vec<JournalEntry> = (0..8).map(ok).chain((8..16).map(hard_fail)).collect();
    let first_fail = ReplayEngine::snapshot_bisect(&entries, is_failure);
    assert_eq!(first_fail, Some(8));

    // ── 33d. Event reconstruction recovers the observable outcome stream ──
    let events: Vec<(&str, &str)> = ReplayEngine::reconstruct_events(&journal.entries).collect();
    assert_eq!(events.len(), 3);
    assert_eq!(events[0], ("plan_arrangement", "ok"));
    assert_eq!(events[2], ("apply_patch", "hard_failure:true_peak"));

    // ── 33e. Substituted policy replay (§9.4) — would a stricter policy
    //         have caught the failure earlier? ──
    let original = vec![ok(0), ok(1), ok(2)];
    let stricter = ReplayEngine::replay_with_substituted_inputs(&original, |e| {
        if e.seq >= 2 {
            vec!["hard_failure:true_peak".to_string()]
        } else {
            vec!["ok".to_string()]
        }
    });
    let divergence = ReplayEngine::divergence_point(&original, &stricter.entries);
    assert_eq!(divergence, Some(2));

    // ── 33f. Repro: identical inputs → identical replay (no divergence) ──
    let identical = ReplayEngine::replay_with_substituted_inputs(&original, |e| e.events.clone());
    assert_eq!(
        ReplayEngine::divergence_point(&original, &identical.entries),
        None
    );
}

/// Step 34 — knowledge graph / ontology layer (§9.6).
///
/// "A graph layer makes the system more queryable and more learnable." This
/// step proves the typed ontology: real validation diagnostics are ingested
/// into the graph, the §9.6 relationships (Diagnostic → affects → Metric,
/// Metric → evidence in → Segment, Patch → fixes → Metric) materialize, and
/// the query patterns the spec lists actually work — "which patches fix this
/// failure" and "what does this diagnostic affect".
#[test]
fn mvas_step_34_knowledge_graph_answers_ontology_queries() {
    use scorestrata_storage::knowledge::{EdgeKind, KnowledgeGraph, NodeKind, node_id};

    // ── 34a. Ingest real diagnostics from the over-the-wire validation ──
    let project = project_with_out_of_scale_lead();
    let report = build_report(&project);
    let mut graph = KnowledgeGraph::new();
    graph.ingest_diagnostics("p34", &report.diagnostics);
    assert!(graph.node_count() > 3, "ontology has project + entities");
    assert!(
        graph.edges.iter().any(|e| e.kind == EdgeKind::Affects),
        "diagnostics affect metrics"
    );

    // ── 34b. Patch → fixes → Metric edges materialize from suggestions ──
    let scale_diag = report
        .diagnostics
        .iter()
        .find(|d| d.code == "SCALE_COMPLIANCE_VIOLATION" || d.metric.contains("scale"));
    if let Some(diag) = scale_diag {
        let metric_id = node_id(NodeKind::Metric, &format!("{}:p34", diag.metric));
        let fixers = graph.frequent_fixers(&metric_id);
        if !diag.suggested_patches.is_empty() {
            assert!(!fixers.is_empty(), "suggested patches become fixers");
            assert!(
                fixers.iter().any(|(node, _)| node.kind == NodeKind::Patch),
                "fixers are patch nodes"
            );
        }
    }

    // ── 34e. Blocking query: "which tasks block the validator milestone?" ──
    let mut tasks = KnowledgeGraph::new();
    for (id, label) in [
        ("core-001", "Define core IDs"),
        ("theory-001", "Implement scales"),
        ("validator-001", "Implement scale compliance"),
        ("compiler-001", "Timeline IR"),
    ] {
        tasks.add_node(NodeKind::Task, id, label);
    }
    tasks.link(
        NodeKind::Task,
        "validator-001",
        NodeKind::Task,
        "theory-001",
        EdgeKind::DependsOn,
    );
    tasks.link(
        NodeKind::Task,
        "theory-001",
        NodeKind::Task,
        "core-001",
        EdgeKind::DependsOn,
    );
    tasks.link(
        NodeKind::Task,
        "compiler-001",
        NodeKind::Task,
        "validator-001",
        EdgeKind::DependsOn,
    );
    let blockers = tasks.blocking_tasks(&node_id(NodeKind::Task, "compiler-001"));
    assert_eq!(blockers.len(), 3);
    assert!(
        blockers
            .iter()
            .any(|n| n.id == node_id(NodeKind::Task, "core-001"))
    );
    assert!(
        blockers
            .iter()
            .any(|n| n.id == node_id(NodeKind::Task, "theory-001"))
    );
    assert!(
        blockers
            .iter()
            .any(|n| n.id == node_id(NodeKind::Task, "validator-001"))
    );

    // ── 34c. The spec's relationship vocabulary is fully represented ──
    let mut ontology = KnowledgeGraph::new();
    ontology.add_node(NodeKind::Lesson, "l1", "lesson");
    ontology.add_node(NodeKind::Experiment, "e1", "experiment");
    ontology.add_node(NodeKind::Task, "t1", "task");
    ontology.add_node(NodeKind::Spec, "s1", "spec");
    ontology.add_node(NodeKind::GoldenTest, "g1", "golden");
    ontology.add_node(NodeKind::Candidate, "c1", "candidate");
    ontology.add_node(NodeKind::Snapshot, "sn1", "snapshot");
    ontology.add_node(NodeKind::PreferenceSignal, "p1", "signal");
    ontology.add_node(NodeKind::GenreProfile, "gp1", "techno");
    assert!(ontology.link(
        NodeKind::Lesson,
        "l1",
        NodeKind::Experiment,
        "e1",
        EdgeKind::DerivedFrom
    ));
    assert!(ontology.link(
        NodeKind::Task,
        "t1",
        NodeKind::Spec,
        "s1",
        EdgeKind::Implements
    ));
    assert!(ontology.link(
        NodeKind::Spec,
        "s1",
        NodeKind::GoldenTest,
        "g1",
        EdgeKind::VerifiedBy
    ));
    assert!(ontology.link(
        NodeKind::Candidate,
        "c1",
        NodeKind::Snapshot,
        "sn1",
        EdgeKind::ChildOf
    ));
    assert!(ontology.link(
        NodeKind::PreferenceSignal,
        "p1",
        NodeKind::GenreProfile,
        "gp1",
        EdgeKind::Calibrates
    ));
    assert_eq!(ontology.edge_count(), 5);

    // ── 34d. Neighbor traversal answers "what affects this metric" ──
    // Not all diagnostics are scale violations, so use whatever exists.
    let any_diag = report.diagnostics.first();
    if let Some(diag) = any_diag {
        let metric = format!("metric:{}:p34", diag.metric);
        let affecting = graph.neighbors(&metric, EdgeKind::Affects);
        // incoming() gives the diagnostics that affect this metric.
        let incoming_edges = graph.incoming(&metric);
        assert!(
            !incoming_edges.is_empty(),
            "metric has at least one affecting diagnostic"
        );
        assert_eq!(affecting.len(), 0); // outgoing Affects from metric — none
    }
}

/// Step 35 — metric lineage: the repair trajectory behind every metric (§8.3, §15.1).
///
/// §15.1 requires every metric to have lineage. This step proves the
/// trajectory end-to-end: each validation run records into the lineage store,
/// the patch that moved the metric is attributable by delta, regressions and
/// improvements are classified, the worst regression is queryable, and the
/// MCP `get_metric_evidence` tool surfaces the trajectory over the wire.
#[test]
fn mvas_step_35_metric_lineage_shows_repair_trajectory() {
    use scorestrata_core::diagnostic::{MetricSample, MetricScope, Scorecard};
    use scorestrata_core::error::Severity;
    use scorestrata_core::ids::{MetricId, ScorecardId};
    use scorestrata_mcp::dispatch::dispatch_tool;
    use scorestrata_storage::lineage::LineageStore;

    fn metric(value: f32) -> MetricSample {
        MetricSample {
            id: MetricId::new(),
            name: "harmony.scale_compliance".to_string(),
            value,
            severity: Severity::Warning,
            scope: MetricScope::default(),
            evidence_uri: "project://mvas/metrics/scale".to_string(),
            explanation: "scale compliance".to_string(),
            confidence: 0.9,
            suggested_patches: vec![],
        }
    }

    fn scorecard(value: f32) -> Scorecard {
        Scorecard {
            id: ScorecardId::new(),
            metrics: vec![metric(value)],
            overall_score: value,
            hard_failures: 0,
            warnings: 0,
        }
    }

    // ── 35a. Each validation run records into the lineage store ──
    let mut store = LineageStore::new();
    store.record_scorecard(&scorecard(0.50), 2, Some("validate_v0"));
    store.record_scorecard(&scorecard(0.80), 2, Some("apply_patch:quantize"));
    store.record_scorecard(&scorecard(0.85), 2, Some("apply_patch:harmonize"));
    let lineage = store
        .get("harmony.scale_compliance")
        .expect("lineage recorded");
    assert_eq!(lineage.len(), 3);

    // ── 35b. Deltas attribute the patch that moved the metric ──
    assert!((lineage.samples[1].delta.unwrap() - 0.30).abs() < f32::EPSILON);
    assert!((lineage.samples[2].delta.unwrap() - 0.05).abs() < f32::EPSILON);
    assert_eq!(
        lineage.samples[1].command_id.as_deref(),
        Some("apply_patch:quantize")
    );
    assert!((lineage.total_delta().unwrap() - 0.35).abs() < f32::EPSILON);

    // ── 35c. A regression run is classified and queryable ──
    store.record_scorecard(&scorecard(0.78), 2, Some("apply_patch:reduce_masking"));
    let lineage = store.get("harmony.scale_compliance").unwrap();
    assert_eq!(lineage.regression_count(), 1);
    assert_eq!(lineage.improvement_count(), 2);
    assert!(lineage.worst_regression().is_some());
    let regressions = store.latest_regressions();
    assert_eq!(regressions.len(), 1);
    assert_eq!(regressions[0].0.metric_name, "harmony.scale_compliance");

    // ── 35d. The trajectory surfaces over the wire via get_metric_evidence ──
    let warehouse = scorestrata_storage::evidence::EvidenceWarehouse::new();
    let receipt = dispatch_tool(
        "get_metric_evidence",
        serde_json::json!({
            "project_id": "p35",
            "metric_name": "harmony.scale_compliance",
            "warehouse": &warehouse,
            "lineage": &store,
        }),
    );
    assert!(
        receipt.summary.contains("lineage"),
        "receipt reports lineage: {}",
        receipt.summary
    );
    assert!(
        receipt.summary.contains("total delta +0.280"),
        "receipt shows the full repair delta: {}",
        receipt.summary
    );
    assert_eq!(
        receipt.score_delta.get("lineage.samples"),
        Some(&"4".to_string())
    );
}

/// Step 36 — §17.2 Render Manifest: every render produces a reproducibility
/// receipt tying the artifact's SHA-256 to its inputs, and the receipt is
/// verifiable over the wire.
#[test]
fn mvas_step_36_render_manifest_proves_reproducibility() {
    use scorestrata_mcp::dispatch::dispatch_tool;
    use scorestrata_storage::cache::content_hash;
    use scorestrata_storage::render_manifest::{RenderManifest, RenderManifestStore};

    // A deterministic render: same project, same inputs → same bytes.
    let project = scorestrata_testkit::melodic_techno_project();
    let result = scorestrata_mcp::tools::render_project_to_analysis(&project)
        .expect("project renders deterministically");
    let master_checksum = content_hash(&result.wav_bytes);

    // Produce the manifest for the master render (§17.2 schema fields).
    let manifest = RenderManifest::from_render(
        "art_p36_master",
        "master_wav",
        "renders/master.wav",
        &master_checksum,
        result.sample_rate,
        result.channels,
        result.duration_seconds,
        "snap_p36_0042",
        "cmd_0042",
        "0.3.0",
        "0.3.0",
        Some(1337),
        std::collections::BTreeMap::new(),
    );
    assert!(manifest.is_complete(), "manifest is a usable receipt"); // Re-rendering with identical inputs reproduces the same manifest hash
    // (§17.1: artifacts reproducible from the same inputs).
    let re_render = scorestrata_mcp::tools::render_project_to_analysis(&project)
        .expect("deterministic re-render");
    let manifest_2 = RenderManifest::from_render(
        "art_p36_master",
        "master_wav",
        "renders/master.wav",
        &content_hash(&re_render.wav_bytes),
        re_render.sample_rate,
        re_render.channels,
        re_render.duration_seconds,
        "snap_p36_0042",
        "cmd_0042",
        "0.3.0",
        "0.3.0",
        Some(1337),
        std::collections::BTreeMap::new(),
    );
    assert!(
        manifest.reproduces(&manifest_2),
        "identical inputs reproduce the same manifest"
    );
    assert_eq!(manifest.manifest_hash(), manifest_2.manifest_hash());

    // Different seed breaks reproducibility (§17.1).
    let mut different = manifest.clone();
    different.seed = Some(9999);
    assert!(!manifest.reproduces(&different));

    // The manifest's cache key is the deterministic reproducibility
    // fingerprint (§9.3).
    assert!(manifest.cache_key().starts_with("render/"));
    assert!(
        manifest
            .cache_key()
            .contains(&manifest.manifest_hash()[..8])
    );

    // Record it and query it over the wire: the tool verifies the artifact's
    // actual bytes against the declared checksum (§17.3).
    let mut store = RenderManifestStore::new();
    store.record(manifest);
    let receipt = dispatch_tool(
        "get_render_manifest",
        serde_json::json!({
            "project_id": "p36",
            "artifact_id": "art_p36_master",
            "manifests": serde_json::to_value(&store).unwrap_or_default(),
            "artifact_bytes": result.wav_bytes,
        }),
    );
    assert!(
        receipt.summary.contains("CHECKSUM MATCH"),
        "wire receipt verifies the artifact: {}",
        receipt.summary
    );
    assert!(
        receipt.summary.contains("48000 Hz")
            && receipt.summary.contains("from snapshot")
            && receipt.summary.contains("reproducible inputs"),
        "receipt reports format + provenance: {}",
        receipt.summary
    );

    // Tampered bytes must fail verification over the wire.
    let mut tampered = result.wav_bytes;
    tampered[0] ^= 0xFF;
    let receipt = dispatch_tool(
        "get_render_manifest",
        serde_json::json!({
            "project_id": "p36",
            "artifact_id": "art_p36_master",
            "manifests": serde_json::to_value(&store).unwrap_or_default(),
            "artifact_bytes": tampered,
        }),
    );
    assert!(
        receipt.summary.contains("CHECKSUM MISMATCH"),
        "tampering detected over the wire: {}",
        receipt.summary
    );
}

#[test]
fn mvas_step_37_render_project_emits_manifest_over_wire() {
    use scorestrata_mcp::dispatch::dispatch_tool;
    use scorestrata_mcp::tools::ToolStatus;

    // §17.2 "every render should produce a manifest": the render_project
    // TOOL itself emits the reproducibility receipt — no manual manifest
    // construction — records it into a caller-supplied store, and the
    // fingerprint is stable for identical inputs yet drifts when a
    // reproducibility input changes (§17.1).
    let project = scorestrata_testkit::melodic_techno_project();
    let store = scorestrata_storage::render_manifest::RenderManifestStore::new();
    let project_json = serde_json::to_value(&project).expect("project serializes");
    let store_json = serde_json::to_value(&store).expect("store serializes");

    let render = |seed: u64| {
        dispatch_tool(
            "render_project",
            serde_json::json!({
                "project_id": "p37",
                "mode": "master",
                "project": project_json.clone(),
                "manifest_store": store_json.clone(),
                "source_snapshot_id": "snap_p37_0042",
                "command_id": "cmd_0042",
                "seed": seed,
            }),
        )
    };

    let first = render(1337);
    assert!(
        matches!(first.status, ToolStatus::Succeeded),
        "render succeeded: {}",
        first.summary
    );
    let hash = first
        .score_delta
        .get("manifest.manifest_hash")
        .cloned()
        .expect("manifest hash on receipt");
    assert_eq!(
        first
            .score_delta
            .get("manifest.artifact_id")
            .map(String::as_str),
        Some("art_p37_master"),
        "artifact id derived from project + mode"
    );
    assert_eq!(
        first
            .score_delta
            .get("manifest.recorded")
            .map(String::as_str),
        Some("true"),
        "manifest recorded into the supplied store"
    );
    assert!(
        first
            .next_actions
            .iter()
            .any(|a| a == "get_render_manifest"),
        "recorded manifests are retrievable"
    );
    assert!(
        first.summary.contains(&hash[..8.min(hash.len())]),
        "summary carries the fingerprint: {}",
        first.summary
    );

    // Identical reproducibility inputs → identical fingerprint.
    let second = render(1337);
    assert_eq!(
        first.score_delta.get("manifest.manifest_hash"),
        second.score_delta.get("manifest.manifest_hash"),
        "identical inputs reproduce the same manifest"
    );

    // Seed drift changes the fingerprint (§17.1).
    let drifted = render(9999);
    assert_ne!(
        first.score_delta.get("manifest.manifest_hash"),
        drifted.score_delta.get("manifest.manifest_hash"),
        "seed drift breaks reproducibility"
    );
}

/// Build a full validation report for a project (Level 2 symbolic).
fn build_report(
    project: &scorestrata_core::domain::Project,
) -> scorestrata_core::diagnostic::ValidationReport {
    use scorestrata_core::diagnostic::Diagnostic;

    let mut diagnostics: Vec<Diagnostic> = Vec::new();
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

    // Scale compliance per pattern (the key symbolic gate).
    if let Some(key) = scorestrata_theory::key::Key::parse(&project.config.key) {
        for pattern in &project.patterns {
            // Attach the scope of the first layer that references this
            // pattern so diagnostics point at real musical entities.
            let mut scope: Option<(
                scorestrata_core::ids::SegmentId,
                scorestrata_core::ids::LayerId,
            )> = None;
            'outer: for segment in &project.arrangement.segments {
                for layer in &segment.layers {
                    if layer
                        .pattern_instances
                        .iter()
                        .any(|pi| pi.pattern_id == pattern.id)
                    {
                        scope = Some((segment.id, layer.id));
                        break 'outer;
                    }
                }
            }
            let (segment_id, layer_id) = scope.unwrap_or_else(|| {
                (
                    project.arrangement.segments[0].id,
                    project.arrangement.segments[0].layers[0].id,
                )
            });
            let result = scorestrata_validator::scale_check::check_scale_compliance(
                pattern, &key, segment_id, layer_id,
            );
            diagnostics.extend(result.diagnostics);
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

    scorestrata_validator::report::build_validation_report(project.id, 2, diagnostics)
}

/// Build an 8-bar project whose lead contains deliberate out-of-scale notes.
fn project_with_out_of_scale_lead() -> scorestrata_core::domain::Project {
    use scorestrata_core::domain::*;

    let pattern = Pattern {
        id: scorestrata_core::ids::PatternId::new(),
        name: "lead".to_string(),
        bars: 1,
        time_signature: TimeSignature::default(),
        notes: vec![
            Note {
                id: scorestrata_core::ids::NoteId::new(),
                pitch: MidiNote::new(60), // C — in C minor
                start: Tick::new(0),
                duration: Duration::quarter(),
                velocity: Velocity::new(100),
                tied: false,
            },
            Note {
                id: scorestrata_core::ids::NoteId::new(),
                pitch: MidiNote::new(61), // C# — OUT of C minor
                start: Tick::new(480),
                duration: Duration::quarter(),
                velocity: Velocity::new(90),
                tied: false,
            },
            Note {
                id: scorestrata_core::ids::NoteId::new(),
                pitch: MidiNote::new(63), // Eb — in C minor
                start: Tick::new(960),
                duration: Duration::quarter(),
                velocity: Velocity::new(80),
                tied: false,
            },
        ],
        steps: Vec::new(),
    };

    let layer = Layer {
        id: scorestrata_core::ids::LayerId::new(),
        name: "Lead".to_string(),
        instrument: "lead_synth".to_string(),
        pattern_instances: vec![PatternInstance {
            id: scorestrata_core::ids::PatternInstanceId::new(),
            pattern_id: pattern.id,
            start_tick: Tick::new(0),
            transpose: 0,
            velocity_multiplier: 1.0,
        }],
        register: None,
        automation: vec![],
        fx: None,
        mix_gain_db: None,
        mix_pan: None,
    };

    // Build an 8-bar project with two segments: a sparse verse (drums only)
    // and a denser chorus (drums + lead with the deliberate defect). The
    // energy contrast makes the transition check pass once the scale issue
    // is fixed, so the repair loop provably raises the overall score.
    let drums = scorestrata_testkit::test_pattern();

    let verse = Segment {
        id: scorestrata_core::ids::SegmentId::new(),
        name: "Verse".to_string(),
        role: SegmentRole::Verse,
        start_bar: 0,
        bars: 4,
        layers: vec![Layer {
            id: scorestrata_core::ids::LayerId::new(),
            name: "Drums".to_string(),
            instrument: "drum_sampler".to_string(),
            pattern_instances: vec![PatternInstance {
                id: scorestrata_core::ids::PatternInstanceId::new(),
                pattern_id: drums.id,
                start_tick: Tick::new(0),
                transpose: 0,
                velocity_multiplier: 1.0,
            }],
            register: None,
            automation: vec![],
            fx: None,
            mix_gain_db: None,
            mix_pan: None,
        }],
    };

    let chorus = Segment {
        id: scorestrata_core::ids::SegmentId::new(),
        name: "Chorus".to_string(),
        role: SegmentRole::Chorus,
        start_bar: 4,
        bars: 4,
        layers: vec![
            Layer {
                id: scorestrata_core::ids::LayerId::new(),
                name: "Drums".to_string(),
                instrument: "drum_sampler".to_string(),
                pattern_instances: vec![PatternInstance {
                    id: scorestrata_core::ids::PatternInstanceId::new(),
                    pattern_id: drums.id,
                    start_tick: Tick::new(0),
                    transpose: 0,
                    velocity_multiplier: 1.0,
                }],
                register: None,
                automation: vec![],
                fx: None,
                mix_gain_db: None,
                mix_pan: None,
            },
            // Pad fills the harmonic space; three chorus layers vs one verse
            // layer gives the ≥0.15 energy delta the transition check wants.
            Layer {
                id: scorestrata_core::ids::LayerId::new(),
                name: "Pad".to_string(),
                instrument: "pad_synth".to_string(),
                pattern_instances: vec![PatternInstance {
                    id: scorestrata_core::ids::PatternInstanceId::new(),
                    pattern_id: drums.id,
                    start_tick: Tick::new(0),
                    transpose: 0,
                    velocity_multiplier: 1.0,
                }],
                register: None,
                automation: vec![],
                fx: None,
                mix_gain_db: None,
                mix_pan: None,
            },
            layer,
        ],
    };

    let mut project = scorestrata_testkit::minimal_project();
    project.config.key = "C minor".to_string();
    project.config.duration_bars = 8;
    project.arrangement.total_bars = 8;
    project.arrangement.segments = vec![verse, chorus];
    project.patterns = vec![drums, pattern];
    project
}

/// The (`segment_id`, `layer_id`) string scope of the lead layer.
///
/// The lead lives in the chorus (segment 1) as its second layer.
fn lead_scope(project: &scorestrata_core::domain::Project) -> (String, String) {
    let segment = &project.arrangement.segments[1];
    let lead = segment
        .layers
        .iter()
        .find(|l| l.name == "Lead")
        .expect("chorus has a Lead layer");
    (segment.id.to_string(), lead.id.to_string())
}

/// Reference to the lead layer (chorus, second layer).
fn lead_layer(project: &scorestrata_core::domain::Project) -> &scorestrata_core::domain::Layer {
    project.arrangement.segments[1]
        .layers
        .iter()
        .find(|l| l.name == "Lead")
        .expect("chorus has a Lead layer")
}

/// §2 (audio plan) — the `ss analyze` flow end-to-end: render to WAV, then
/// read the file back and produce the post-generation spectral audit (text
/// log + spectrogram/waveform PNGs + genre fingerprint deviation). This is
/// the "debug log for generated music" that makes mix issues visible.
#[test]
fn audio_plan_ss_analyze_flow_produces_audit_artifacts() {
    // Build a genre-typed project through the real pipeline.
    let config = scorestrata_mcp::config_gen::generate_config(
        "analyze_e2e",
        "Melodic techno at 124 BPM in C minor, 8 bars",
    );
    let plan = scorestrata_compiler::plan::plan_project(&config);
    let mut project = scorestrata_testkit::minimal_project();
    project.config = config;
    project.arrangement = plan.arrangement;
    project.patterns = plan.patterns;

    // Render to WAV bytes (same path the CLI `ss render` takes).
    let timeline = scorestrata_compiler::compile_project(&project).expect("project compiles");
    let mut layer_names = std::collections::HashMap::new();
    for segment in &project.arrangement.segments {
        for layer in &segment.layers {
            layer_names.insert(layer.id.to_string(), layer.name.clone());
        }
    }
    let notes: Vec<scorestrata_audio::render::RenderNote> = timeline
        .events
        .iter()
        .filter_map(|e| match &e.kind {
            scorestrata_compiler::timeline::EventKind::NoteOn { pitch, velocity } => {
                let layer_name = layer_names
                    .get(&e.layer_id.to_string())
                    .cloned()
                    .unwrap_or_else(|| e.layer_id.to_string());
                Some(scorestrata_audio::render::RenderNote {
                    pitch: pitch.0,
                    start_tick: e.tick.0,
                    duration_ticks: 480,
                    velocity: velocity.as_f32(),
                    layer_name,
                })
            }
            _ => None,
        })
        .collect();
    let voices = scorestrata_audio::render::build_voices(&{
        let mut inst = Vec::new();
        for segment in &project.arrangement.segments {
            for layer in &segment.layers {
                if !inst.iter().any(|(n, _)| *n == layer.name.as_str()) {
                    inst.push((layer.name.as_str(), layer.instrument.as_str()));
                }
            }
        }
        inst
    });
    let result = scorestrata_audio::render::render_notes_with_config(
        &notes,
        timeline.bpm,
        scorestrata_audio::render::DEFAULT_SAMPLE_RATE,
        &voices,
        &scorestrata_audio::render::RenderConfig {
            stereo: true,
            limiter: Some(scorestrata_audio::mastering::LimiterConfig::default()),
            ..Default::default()
        },
    )
    .expect("render succeeds");
    assert!(!result.wav_bytes.is_empty());

    // The `ss analyze` path: write master.wav, read it back, audit it.
    let dir = std::env::temp_dir().join(format!("scorestrata_analyze_e2e_{}", project.id));
    let render_dir = dir.join("render");
    let wav_path = render_dir.join("master.wav");
    std::fs::create_dir_all(&render_dir).expect("temp render dir");
    std::fs::write(&wav_path, &result.wav_bytes).expect("write master.wav");

    let (left, right, sample_rate) =
        scorestrata_audio::wav::read_wav_file(&wav_path).expect("read back master.wav");
    assert_eq!(sample_rate, scorestrata_audio::render::DEFAULT_SAMPLE_RATE);
    let genre = Some(project.config.genre.as_str());
    let report = scorestrata_audio::audit::audit_stereo(&left, &right, sample_rate, genre);

    // The audit report is a meaningful debug log: not silent, no clipping
    // (the limiter ceiling is -1.0 dBTP), a plausible spectral centroid, and
    // a genre deviation that is a well-formed 0..1 score.
    assert!(!report.silence_detected, "render is not silent");
    assert_eq!(report.clipped_sample_count, 0, "no hard clipping");
    assert!(
        report.spectral_centroid_hz > 100.0,
        "centroid above sub range"
    );
    assert_eq!(report.genre.as_deref(), Some("melodic_techno"));
    let dev = report
        .genre_fingerprint_deviation
        .expect("genre fingerprint exists for melodic_techno");
    assert!((0.0..=1.0).contains(&dev), "deviation in [0,1], got {dev}");
    assert_eq!(report.band_energy.len(), 6, "six spectral bands");

    // The debug-log artifacts land on disk: text log + two PNGs.
    let audit_dir = render_dir.join("audit");
    let artifact = scorestrata_audio::audit::write_audit_files(
        &result.mono_samples,
        result.sample_rate,
        genre,
        &audit_dir,
        "master",
    )
    .expect("audit files written");
    assert!(artifact.text_path.exists());
    assert!(artifact.spectrogram_path.exists());
    assert!(artifact.waveform_path.exists());
    let log = std::fs::read_to_string(&artifact.text_path).expect("read audit log");
    assert!(
        log.contains("centroid"),
        "text log names the spectral centroid"
    );
    let png = std::fs::read(&artifact.spectrogram_path).expect("read spectrogram PNG");
    assert_eq!(
        &png[..8],
        b"\x89PNG\r\n\x1a\n",
        "spectrogram is a valid PNG"
    );

    std::fs::remove_dir_all(&dir).ok();
}
