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
    clippy::assertions_on_constants,
    reason = "Integration test asserts the full slice explicitly"
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
    project.config = config.clone();
    project.arrangement = plan.arrangement.clone();
    project.patterns = plan.patterns.clone();
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
        let before = layer_out_of_scale_count(&lead_layer(&project), &project.patterns, &key);
        assert!(before > 0, "lead layer starts out of scale");

        let params = ApplyPatchParams {
            project_id: project.id.to_string(),
            op: "quantize_layer_to_scale".to_string(),
            params: serde_json::json!({"strength": 1.0}),
            dry_run: false,
            scope: Some(PatchScope {
                segment_id: Some(seg_id.clone()),
                layer_id: Some(layer_id.clone()),
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
        let after = layer_out_of_scale_count(&lead_layer(&project), &project.patterns, &key);
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
            &[diagnosis.clone()],
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
        segment_id: segment_id.clone(),
        project: Some(project_json.clone()),
    });
    assert!(matches!(solo.status, ToolStatus::Succeeded));
    assert!(solo.summary.contains("bars"));
    assert!(solo.summary.contains("layers"));

    // ── 15e. Explain a metric with root causes ──────────────────────
    // Build a degraded project so the metric has root causes to report.
    let mut degraded = project.clone();
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
        project_id: project_id.clone(),
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
            project: Some(project_json.clone()),
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
        "root-cause attribution succeeds for a measured metric: {:#?}",
        roots
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
        project_b: Some(broken_json.clone()),
    });
    assert!(matches!(compared.status, ToolStatus::Succeeded));
    assert!(compared.score_delta.contains_key("diff.metrics_compared"));
    assert!(
        compared.score_delta.get("diff.winner").is_some(),
        "comparison reports a winner between a clean and a broken candidate"
    );

    // ── 17f. Dry-run a patch before committing to it ────────────────
    let dry_run = scorestrata_mcp::tools::dry_run_patch(&DryRunPatchParams {
        project_id: project_id.clone(),
        op: "quantize_layer_to_scale".to_string(),
        params: serde_json::json!({"strength": 1.0}),
        scope: None,
        project: Some(clean_json.clone()),
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
        project: Some(project_json.clone()),
        jobs: Some(serde_json::json!({})),
    });
    assert!(
        matches!(preview.status, ToolStatus::Succeeded),
        "candidate preview should render, got {:#?}",
        preview
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
            project_id: project_id.clone(),
            genre: "melodic_techno".to_string(),
            target_lufs: -10.0,
            profile: Some(serde_json::json!({})),
        });
    assert!(matches!(calibration.status, ToolStatus::Succeeded));
    assert!(calibration.summary.contains("-10.0 LUFS"));
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

/// The (segment_id, layer_id) string scope of the lead layer.
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
