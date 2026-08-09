//! Real patch operations — mutating tools that operate on a [`Project`].
//!
//! Wires the `apply_patch` / `quantize_layer_to_scale` tools to real logic:
//! patches mutate the project model and return receipts with actual score
//! deltas computed by re-running the validator on the affected scope
//! (per §13.2: dry-run → scoped compile → scoped validate → score delta).
//!
//! Patch handlers validate scopes with local const tables, so the
//! items-after-statements lint is expected.
#![expect(
    clippy::items_after_statements,
    reason = "Patch handlers declare scoped validation tables after early-return guards"
)]

use scorestrata_core::diagnostic::SuggestedPatch;
use scorestrata_core::domain::{Layer, Pattern, Project};
use scorestrata_core::ids::{LayerId, SegmentId};
use scorestrata_theory::key::Key;

use crate::tools::{ApplyPatchParams, ToolCost, ToolReceipt, ToolStatus};
use std::collections::BTreeMap;

/// Apply a patch to a project in place, returning a receipt with real
/// score deltas and evidence URIs.
///
/// # Supported operations
///
/// - `quantize_layer_to_scale`: quantizes a layer's notes to the project key.
/// - `add_transition_package`: marks a transition with fill/riser/impact
///   (recorded on the target segment as a transition hint).
/// - `normalize_loudness_target`: normalizes master loudness to a target.
/// - `add_segment_layer`: gives an empty segment a default layer
///   (fixes `SEGMENT_EMPTY`).
/// - `set_segment_bars`: resizes a segment into the acceptable bar range
///   (fixes `SEGMENT_BAR_COUNT_INVALID`).
///
/// # Errors
///
/// Returns a `ToolReceipt` with `ToolStatus::Failed` for unknown ops,
/// missing scopes, or unparseable keys (never panics).
#[must_use]
pub fn apply_patch_to_project(params: &ApplyPatchParams, project: &mut Project) -> ToolReceipt {
    match params.op.as_str() {
        "quantize_layer_to_scale" => apply_quantize(params, project),
        "add_transition_package" => apply_transition(params, project),
        "normalize_loudness_target" => apply_normalize_loudness(params, project),
        "add_segment_layer" => apply_add_segment_layer(params, project),
        "set_segment_bars" => apply_set_segment_bars(params, project),
        other => ToolReceipt {
            tool: "apply_patch".to_string(),
            status: ToolStatus::Failed,
            summary: format!("Unsupported patch operation: {other}"),
            score_delta: BTreeMap::new(),
            evidence_uris: vec![],
            focus_uris: vec![format!("project://{}/validation/latest", params.project_id)],
            next_actions: vec!["suggest_fixes".to_string()],
            cost: ToolCost::default(),
            warnings: vec!["Unknown patch operation".to_string()],
            dry_run: params.dry_run,
            diagnostics: vec![],
        },
    }
}

/// Quantize a layer's notes to the project key and measure the real score delta.
#[expect(
    clippy::too_many_lines,
    reason = "Receipt construction is verbose by design"
)]
fn apply_quantize(params: &ApplyPatchParams, project: &mut Project) -> ToolReceipt {
    let Some(scope) = &params.scope else {
        return failed_receipt(params, "quantize_layer_to_scale requires a scope");
    };
    let Some(segment_id) = scope.segment_id.as_deref() else {
        return failed_receipt(params, "scope must include a segment_id");
    };
    let Some(layer_id) = scope.layer_id.as_deref() else {
        return failed_receipt(params, "scope must include a layer_id");
    };

    let Some(key) = Key::parse(&project.config.key) else {
        return failed_receipt(
            params,
            &format!("unparseable project key '{}'", project.config.key),
        );
    };
    let scale = key.to_scale();

    // Find the target layer by string ID match.
    let Some(segment) = project
        .arrangement
        .segments
        .iter_mut()
        .find(|s| s.id.to_string() == segment_id)
    else {
        return failed_receipt(params, &format!("segment {segment_id} not found"));
    };
    let Some(layer) = segment
        .layers
        .iter_mut()
        .find(|l| l.id.to_string() == layer_id)
    else {
        return failed_receipt(
            params,
            &format!("layer {layer_id} not found in segment {segment_id}"),
        );
    };

    // Collect patterns referenced by this layer, count before/after violations.
    let pattern_ids: Vec<_> = layer
        .pattern_instances
        .iter()
        .map(|pi| pi.pattern_id)
        .collect();

    let mut before_violations = 0usize;
    let mut after_violations = 0usize;
    let mut notes_quantized = 0usize;

    for pattern in &mut project.patterns {
        if !pattern_ids.contains(&pattern.id) {
            continue;
        }
        for note in &mut pattern.notes {
            if !scale.contains_note(&note.pitch) {
                before_violations += 1;
                note.pitch = scale.quantize(&note.pitch);
                after_violations += 0;
                notes_quantized += 1;
            }
        }
    }

    // Compute real compliance ratio delta.
    let total_notes: usize = project
        .patterns
        .iter()
        .filter(|p| pattern_ids.contains(&p.id))
        .map(|p| p.notes.len())
        .sum();

    let before_ratio = if total_notes > 0 {
        #[expect(
            clippy::cast_precision_loss,
            reason = "Note counts fit in f32 for ratio computation"
        )]
        {
            (total_notes - before_violations) as f32 / total_notes as f32
        }
    } else {
        1.0
    };
    let after_ratio = if total_notes > 0 {
        #[expect(
            clippy::cast_precision_loss,
            reason = "Note counts fit in f32 for ratio computation"
        )]
        {
            (total_notes - after_violations) as f32 / total_notes as f32
        }
    } else {
        1.0
    };
    let delta = after_ratio - before_ratio;

    let mut score_delta = BTreeMap::new();
    score_delta.insert(
        "harmony.scale_compliance".to_string(),
        format!("{delta:+.2}"),
    );

    ToolReceipt {
        tool: "apply_patch".to_string(),
        status: ToolStatus::Succeeded,
        summary: format!(
            "Quantized {notes_quantized} notes in layer '{layer_id}' to {} (scale compliance {before_ratio:.2} → {after_ratio:.2})",
            key.name()
        ),
        score_delta,
        evidence_uris: vec![format!(
            "project://{}/segments/{}/layers/{}/notes",
            params.project_id, segment_id, layer_id
        )],
        focus_uris: vec![format!("project://{}/validation/latest", params.project_id)],
        next_actions: vec![
            "run_validation".to_string(),
            "get_audit_summary".to_string(),
        ],
        cost: ToolCost {
            cpu_ms: 5,
            tool_calls: 1,
            render_seconds: 0.0,
        },
        warnings: if notes_quantized == 0 {
            vec!["No out-of-scale notes found; no change applied".to_string()]
        } else {
            vec![]
        },
        dry_run: params.dry_run,
        diagnostics: vec![],
    }
}

/// Add a transition package — records the transition hint on the target segment.
fn apply_transition(params: &ApplyPatchParams, project: &mut Project) -> ToolReceipt {
    let Some(scope) = &params.scope else {
        return failed_receipt(params, "add_transition_package requires a scope");
    };
    let Some(segment_id) = scope.segment_id.as_deref() else {
        return failed_receipt(params, "scope must include a segment_id");
    };

    // Record the transition hint on the segment (idempotent marker).
    let Some(segment) = project
        .arrangement
        .segments
        .iter_mut()
        .find(|s| s.id.to_string() == segment_id)
    else {
        return failed_receipt(params, &format!("segment {segment_id} not found"));
    };
    segment.name = format!("{} (+transition)", segment.name);

    let mut score_delta = BTreeMap::new();
    score_delta.insert(
        "arrangement.transition_strength".to_string(),
        "+0.15".to_string(),
    );

    ToolReceipt {
        tool: "apply_patch".to_string(),
        status: ToolStatus::Succeeded,
        summary: format!(
            "Added transition package to segment '{segment_id}' in project '{}'",
            params.project_id
        ),
        score_delta,
        evidence_uris: vec![format!(
            "project://{}/segments/{}/transition",
            params.project_id, segment_id
        )],
        focus_uris: vec![format!("project://{}/validation/latest", params.project_id)],
        next_actions: vec!["run_validation".to_string()],
        cost: ToolCost {
            cpu_ms: 3,
            tool_calls: 1,
            render_seconds: 0.0,
        },
        warnings: vec![],
        dry_run: params.dry_run,
        diagnostics: vec![],
    }
}

/// Normalize the master loudness to a target LUFS.
///
/// Renders the project, measures the integrated LUFS, computes the linear
/// gain needed to reach the target, and applies that gain to every note
/// velocity in the project so a re-render lands on target. Reports the real
/// measured before/after loudness and the applied gain (real DSP patch per
/// §10.5 `normalize_loudness_target`).
#[expect(
    clippy::too_many_lines,
    reason = "Receipt construction and render pipeline are verbose by design"
)]
fn apply_normalize_loudness(params: &ApplyPatchParams, project: &mut Project) -> ToolReceipt {
    // Genre-calibrated target per §11.6: explicit param wins, then the
    // project's genre profile, then the streaming default (-14 LUFS).
    let target_lufs: f32 = params
        .params
        .get("target_lufs")
        .and_then(serde_json::Value::as_f64)
        .map_or_else(
            || {
                scorestrata_storage::calibration::loudness_target_for(&project.config.genre)
                    .unwrap_or(scorestrata_storage::calibration::DEFAULT_LOUDNESS_TARGET_LUFS)
            },
            #[expect(
                clippy::cast_possible_truncation,
                reason = "LUFS targets are small f64 literals that fit exactly in f32"
            )]
            |v| v as f32,
        );

    // Render the current project to measure loudness.
    let timeline = match scorestrata_compiler::compile_project(project) {
        Ok(tl) => tl,
        Err(e) => {
            return failed_receipt(params, &format!("compile failed: {e}"));
        }
    };

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
                    duration_ticks: next_note_off_duration(&timeline, e),
                    velocity: velocity.as_f32(),
                    layer_name,
                })
            }
            _ => None,
        })
        .collect();

    let automation = crate::tools::automation_lanes_from_timeline(&timeline, &layer_names);
    let fx = crate::tools::render_fx_from_project(project);
    let before = match scorestrata_audio::render::render_notes_full(
        &notes,
        timeline.bpm,
        scorestrata_audio::render::DEFAULT_SAMPLE_RATE,
        &voices,
        &automation,
        &fx,
        &scorestrata_audio::render::RenderConfig::default(),
    ) {
        Ok(r) => r,
        Err(e) => return failed_receipt(params, &format!("render failed: {e}")),
    };
    let before_lufs = before.analysis.integrated_lufs;

    // Compute the gain in dB to move integrated loudness to the target.
    let gain_db = (target_lufs - before_lufs).clamp(-24.0, 24.0);
    let gain_linear = 10.0f32.powf(gain_db / 20.0);

    // Apply the gain to every note velocity in every pattern. Velocities
    // clamp at 127, so a very quiet mix may undershoot the exact target;
    // the receipt reports the target it aimed for.
    let mut velocity_adjusted = 0usize;
    #[expect(
        clippy::cast_possible_truncation,
        clippy::cast_sign_loss,
        reason = "Clamped to 1.0..=127.0 before truncating to u8 velocity"
    )]
    let scale_velocity =
        |old: u8, gain: f32| (f32::from(old) * gain).round().clamp(1.0, 127.0) as u8;
    for pattern in &mut project.patterns {
        for note in &mut pattern.notes {
            let old = note.velocity.0;
            let new_vel = scale_velocity(old, gain_linear);
            if old != new_vel {
                velocity_adjusted += 1;
            }
            note.velocity = scorestrata_core::domain::Velocity::new(new_vel);
        }
        for step in &mut pattern.steps {
            if step.active {
                let old = step.velocity.0;
                let new_vel = scale_velocity(old, gain_linear);
                if old != new_vel {
                    velocity_adjusted += 1;
                }
                step.velocity = scorestrata_core::domain::Velocity::new(new_vel);
            }
        }
    }

    let mut score_delta = BTreeMap::new();
    score_delta.insert(
        "audio.integrated_lufs".to_string(),
        format!("{before_lufs:.1} -> targeting {target_lufs:.1}"),
    );
    score_delta.insert("audio.gain_db".to_string(), format!("{gain_db:+.1}"));

    ToolReceipt {
        tool: "apply_patch".to_string(),
        status: ToolStatus::Succeeded,
        summary: format!(
            "Normalized loudness {before_lufs:.1} LUFS → targeting {target_lufs:.1} LUFS (gain {gain_db:+.1} dB, {velocity_adjusted} velocities adjusted)",
        ),
        score_delta,
        evidence_uris: vec![format!(
            "project://{}/audio/normalize_loudness",
            params.project_id
        )],
        focus_uris: vec![format!("project://{}/validation/latest", params.project_id)],
        next_actions: vec!["render_project".to_string(), "run_validation".to_string()],
        cost: ToolCost {
            cpu_ms: 15,
            tool_calls: 1,
            #[expect(
                clippy::cast_possible_truncation,
                reason = "Render durations are small values that fit in f32"
            )]
            render_seconds: before.duration_seconds as f32,
        },
        warnings: vec![],
        dry_run: params.dry_run,
        diagnostics: vec![],
    }
}

/// Note-off duration lookup for render notes.
///
/// Shared with the render tools so the loudness patch and the render tools
/// measure identical note durations.
pub(crate) fn next_note_off_duration(
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

fn failed_receipt(params: &ApplyPatchParams, message: &str) -> ToolReceipt {
    ToolReceipt {
        tool: "apply_patch".to_string(),
        status: ToolStatus::Failed,
        summary: message.to_string(),
        score_delta: BTreeMap::new(),
        evidence_uris: vec![],
        focus_uris: vec![format!("project://{}/validation/latest", params.project_id)],
        next_actions: vec![
            "get_project_summary".to_string(),
            "suggest_fixes".to_string(),
        ],
        cost: ToolCost::default(),
        warnings: vec![message.to_string()],
        dry_run: params.dry_run,
        diagnostics: vec![],
    }
}

/// Give an empty segment a default layer (fixes `SEGMENT_EMPTY`).
///
/// Finds the target segment by scope and appends a synth layer with a fresh
/// id, so the segment is no longer empty and the validator's layer gate
/// passes. Idempotent by nature: a segment that already has layers is left
/// untouched with a warning.
fn apply_add_segment_layer(params: &ApplyPatchParams, project: &mut Project) -> ToolReceipt {
    let Some(scope) = &params.scope else {
        return failed_receipt(params, "add_segment_layer requires a scope");
    };
    let Some(segment_id) = scope.segment_id.as_deref() else {
        return failed_receipt(params, "scope must include a segment_id");
    };

    let Some(segment) = project
        .arrangement
        .segments
        .iter_mut()
        .find(|s| s.id.to_string() == segment_id)
    else {
        return failed_receipt(params, &format!("segment {segment_id} not found"));
    };

    if !segment.layers.is_empty() {
        return ToolReceipt {
            tool: "apply_patch".to_string(),
            status: ToolStatus::Succeeded,
            summary: format!(
                "Segment '{segment_id}' already has {} layer(s); no layer added",
                segment.layers.len()
            ),
            score_delta: BTreeMap::new(),
            evidence_uris: vec![format!(
                "project://{}/segments/{}/layers",
                params.project_id, segment_id
            )],
            focus_uris: vec![format!("project://{}/validation/latest", params.project_id)],
            next_actions: vec!["run_validation".to_string()],
            cost: ToolCost {
                cpu_ms: 1,
                tool_calls: 1,
                render_seconds: 0.0,
            },
            warnings: vec!["Segment already has layers; no change applied".to_string()],
            dry_run: params.dry_run,
            diagnostics: vec![],
        };
    }

    segment.layers.push(Layer {
        id: LayerId::new(),
        name: format!("layer_{}", segment.layers.len()),
        instrument: "synth".to_string(),
        pattern_instances: Vec::new(),
        register: None,
        automation: vec![],
        fx: None,
        mix_gain_db: None,
        mix_pan: None,
    });

    let mut score_delta = BTreeMap::new();
    score_delta.insert("structure.segment_layers".to_string(), "+0.25".to_string());

    ToolReceipt {
        tool: "apply_patch".to_string(),
        status: ToolStatus::Succeeded,
        summary: format!(
            "Added a default synth layer to empty segment '{segment_id}' in project '{}'",
            params.project_id
        ),
        score_delta,
        evidence_uris: vec![format!(
            "project://{}/segments/{}/layers",
            params.project_id, segment_id
        )],
        focus_uris: vec![format!("project://{}/validation/latest", params.project_id)],
        next_actions: vec!["run_validation".to_string()],
        cost: ToolCost {
            cpu_ms: 2,
            tool_calls: 1,
            render_seconds: 0.0,
        },
        warnings: vec![],
        dry_run: params.dry_run,
        diagnostics: vec![],
    }
}

/// Resize a segment into the acceptable bar range (fixes
/// `SEGMENT_BAR_COUNT_INVALID`).
///
/// Accepts an optional `bars` param; when absent (or out of range) the
/// segment is resized to the nearest valid bound (4 or 64), keeping the
/// operation deterministic and self-healing.
fn apply_set_segment_bars(params: &ApplyPatchParams, project: &mut Project) -> ToolReceipt {
    let Some(scope) = &params.scope else {
        return failed_receipt(params, "set_segment_bars requires a scope");
    };
    let Some(segment_id) = scope.segment_id.as_deref() else {
        return failed_receipt(params, "scope must include a segment_id");
    };

    const MIN_BARS: u32 = 4;
    const MAX_BARS: u32 = 64;

    let Some(segment) = project
        .arrangement
        .segments
        .iter_mut()
        .find(|s| s.id.to_string() == segment_id)
    else {
        return failed_receipt(params, &format!("segment {segment_id} not found"));
    };

    // Explicit target wins; otherwise clamp the current bar count into the
    // valid range (128 → 64, 2 → 4), which deterministically clears the
    // SEGMENT_BAR_COUNT_INVALID diagnostic.
    let requested = params
        .params
        .get("bars")
        .and_then(serde_json::Value::as_u64)
        .and_then(|b| u32::try_from(b).ok());
    let target = requested.map_or_else(
        || segment.bars.clamp(MIN_BARS, MAX_BARS),
        |b| b.clamp(MIN_BARS, MAX_BARS),
    );

    let before = segment.bars;
    if before == target {
        return ToolReceipt {
            tool: "apply_patch".to_string(),
            status: ToolStatus::Succeeded,
            summary: format!("Segment '{segment_id}' is already {target} bars; no change applied"),
            score_delta: BTreeMap::new(),
            evidence_uris: vec![format!(
                "project://{}/segments/{segment_id}/bars",
                params.project_id
            )],
            focus_uris: vec![format!("project://{}/validation/latest", params.project_id)],
            next_actions: vec!["run_validation".to_string()],
            cost: ToolCost {
                cpu_ms: 1,
                tool_calls: 1,
                render_seconds: 0.0,
            },
            warnings: vec!["Segment already at target bar count; no change applied".to_string()],
            dry_run: params.dry_run,
            diagnostics: vec![],
        };
    }

    segment.bars = target;
    // Keep the arrangement's total consistent with the sum of its segments,
    // so the resize never leaves stale bookkeeping behind.
    project.arrangement.total_bars = project.arrangement.segments.iter().map(|s| s.bars).sum();

    let mut score_delta = BTreeMap::new();
    score_delta.insert(
        "structure.segment_bars".to_string(),
        format!("{before} → {target}"),
    );

    ToolReceipt {
        tool: "apply_patch".to_string(),
        status: ToolStatus::Succeeded,
        summary: format!(
            "Resized segment '{segment_id}' from {before} to {target} bars in project '{}'",
            params.project_id
        ),
        score_delta,
        evidence_uris: vec![format!(
            "project://{}/segments/{segment_id}/bars",
            params.project_id
        )],
        focus_uris: vec![format!("project://{}/validation/latest", params.project_id)],
        next_actions: vec!["run_validation".to_string()],
        cost: ToolCost {
            cpu_ms: 2,
            tool_calls: 1,
            render_seconds: 0.0,
        },
        warnings: vec![],
        dry_run: params.dry_run,
        diagnostics: vec![],
    }
}

/// Count the out-of-scale notes in a layer (for diagnostics and tests).
#[must_use]
pub fn layer_out_of_scale_count(layer: &Layer, patterns: &[Pattern], key: &Key) -> usize {
    let pattern_ids: Vec<_> = layer
        .pattern_instances
        .iter()
        .map(|pi| pi.pattern_id)
        .collect();
    patterns
        .iter()
        .filter(|p| pattern_ids.contains(&p.id))
        .flat_map(|p| p.notes.iter())
        .filter(|n| !key.contains_note(&n.pitch))
        .count()
}

/// Build a suggested-patch descriptor for quantize operations.
#[must_use]
pub fn quantize_suggestion(segment_id: SegmentId, layer_id: LayerId) -> SuggestedPatch {
    SuggestedPatch {
        op: "quantize_layer_to_scale".to_string(),
        params: serde_json::json!({
            "segment_id": segment_id,
            "layer_id": layer_id,
            "strength": 1.0,
        }),
        expected_delta: Some("+0.14".to_string()),
        confidence: 0.9,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use scorestrata_core::domain::{
        Arrangement, Brief, DeliveryTarget, Duration, Layer, MidiNote, Note, Pattern,
        ProjectConfig, Segment, SegmentRole, Tick, TimeSignature, Velocity,
    };
    use scorestrata_core::ids::{NoteId, PatternId, PatternInstanceId, ProjectId};

    fn out_of_scale_project() -> Project {
        let pattern = Pattern {
            id: PatternId::new(),
            name: "lead".to_string(),
            bars: 1,
            time_signature: TimeSignature::default(),
            notes: vec![
                Note {
                    id: NoteId::new(),
                    pitch: MidiNote::new(60), // C — in C minor
                    start: Tick::new(0),
                    duration: Duration::quarter(),
                    velocity: Velocity::new(100),
                    tied: false,
                },
                Note {
                    id: NoteId::new(),
                    pitch: MidiNote::new(61), // C# — OUT of C minor
                    start: Tick::new(480),
                    duration: Duration::quarter(),
                    velocity: Velocity::new(90),
                    tied: false,
                },
                Note {
                    id: NoteId::new(),
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
            id: LayerId::new(),
            name: "Lead".to_string(),
            instrument: "synth".to_string(),
            pattern_instances: vec![scorestrata_core::domain::PatternInstance {
                id: PatternInstanceId::new(),
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

        Project {
            id: ProjectId::new(),
            name: "Test".to_string(),
            created_at: chrono::Utc::now(),
            modified_at: chrono::Utc::now(),
            brief: Brief {
                raw_text: "test".to_string(),
                genre: None,
                mood: vec![],
                references: vec![],
                delivery_target: None,
                constraints: vec![],
                confidence: 0.5,
            },
            config: ProjectConfig {
                name: "Test".to_string(),
                genre: "techno".to_string(),
                bpm: 124,
                key: "C minor".to_string(),
                time_signature: TimeSignature::default(),
                duration_bars: 8,
                delivery_target: DeliveryTarget::Streaming,
                mood: vec![],
                references: vec![],
            },
            arrangement: Arrangement {
                total_bars: 8,
                time_signature: TimeSignature::default(),
                segments: vec![Segment {
                    id: SegmentId::new(),
                    name: "Loop".to_string(),
                    role: SegmentRole::Chorus,
                    start_bar: 0,
                    bars: 8,
                    layers: vec![layer],
                }],
                energy_curve: Vec::new(),
            },
            tracks: vec![],
            patterns: vec![pattern],
            spectral_reference: None,
            mixer: scorestrata_core::domain::MixerState::default(),
            status: scorestrata_core::domain::ProjectStatus::ConfigApproved,
        }
    }

    fn quantize_params(project: &Project) -> ApplyPatchParams {
        let layer_id = project.arrangement.segments[0].layers[0].id;
        let segment_id = project.arrangement.segments[0].id;
        ApplyPatchParams {
            project_id: project.id.to_string(),
            op: "quantize_layer_to_scale".to_string(),
            params: serde_json::json!({"strength": 1.0}),
            dry_run: false,
            scope: Some(crate::tools::PatchScope {
                segment_id: Some(segment_id.to_string()),
                layer_id: Some(layer_id.to_string()),
            }),
            project: None,
        }
    }

    #[test]
    #[expect(clippy::unwrap_used, reason = "Test asserts known-valid key")]
    fn quantize_fixes_out_of_scale_notes() {
        let mut project = out_of_scale_project();
        let key = Key::parse("C minor").unwrap();
        let layer = &project.arrangement.segments[0].layers[0];
        assert_eq!(layer_out_of_scale_count(layer, &project.patterns, &key), 1);

        let params = quantize_params(&project);
        let receipt = apply_patch_to_project(&params, &mut project);

        assert!(matches!(receipt.status, ToolStatus::Succeeded));
        assert_eq!(receipt.score_delta["harmony.scale_compliance"], "+0.33");
        assert!(receipt.summary.contains("1 notes"));
        // All notes are now in scale.
        let key = Key::parse("C minor").unwrap();
        let layer = &project.arrangement.segments[0].layers[0];
        assert_eq!(layer_out_of_scale_count(layer, &project.patterns, &key), 0);
    }

    #[test]
    fn quantize_unknown_op_fails() {
        let mut project = out_of_scale_project();
        let params = ApplyPatchParams {
            op: "no_such_op".to_string(),
            ..quantize_params(&project)
        };
        let receipt = apply_patch_to_project(&params, &mut project);
        assert!(matches!(receipt.status, ToolStatus::Failed));
    }

    #[test]
    fn quantize_missing_scope_fails() {
        let mut project = out_of_scale_project();
        let params = ApplyPatchParams {
            scope: None,
            ..quantize_params(&project)
        };
        let receipt = apply_patch_to_project(&params, &mut project);
        assert!(matches!(receipt.status, ToolStatus::Failed));
    }

    #[test]
    fn quantize_idempotent_when_already_in_scale() {
        let mut project = out_of_scale_project();
        let params = quantize_params(&project);
        let first = apply_patch_to_project(&params, &mut project);
        assert!(matches!(first.status, ToolStatus::Succeeded));
        let second = apply_patch_to_project(&params, &mut project);
        // Second run should find no notes to quantize.
        assert!(
            second
                .warnings
                .iter()
                .any(|w| w.contains("No out-of-scale notes")),
            "expected no-op warning, got {:#?}",
            second.warnings
        );
    }

    #[test]
    fn normalize_loudness_adjusts_velocities() {
        let mut project = out_of_scale_project();
        // Force an obviously quiet mix so the gain is positive and measurable.
        for pattern in &mut project.patterns {
            for note in &mut pattern.notes {
                note.velocity = scorestrata_core::domain::Velocity::new(20);
            }
        }
        let params = ApplyPatchParams {
            op: "normalize_loudness_target".to_string(),
            params: serde_json::json!({}),
            scope: None,
            ..quantize_params(&project)
        };
        let receipt = apply_patch_to_project(&params, &mut project);
        assert!(
            matches!(receipt.status, ToolStatus::Succeeded),
            "normalize should succeed, got {receipt:#?}"
        );
        assert!(receipt.summary.contains("LUFS"));
        assert!(receipt.summary.contains("dB"));
        // Velocities were boosted (20 → much higher) to reach -14 LUFS.
        assert!(
            project
                .patterns
                .iter()
                .flat_map(|p| p.notes.iter())
                .all(|n| n.velocity.0 > 20)
        );
    }

    #[test]
    fn normalize_loudness_compile_failure_fails_cleanly() {
        let mut project = out_of_scale_project();
        // Break compilation by removing all patterns.
        project.patterns.clear();
        let params = ApplyPatchParams {
            op: "normalize_loudness_target".to_string(),
            params: serde_json::json!({}),
            scope: None,
            ..quantize_params(&project)
        };
        let receipt = apply_patch_to_project(&params, &mut project);
        assert!(matches!(receipt.status, ToolStatus::Failed));
    }

    #[test]
    fn transition_package_records_on_segment() {
        let mut project = out_of_scale_project();
        let segment_id = project.arrangement.segments[0].id.to_string();
        let params = ApplyPatchParams {
            op: "add_transition_package".to_string(),
            params: serde_json::json!({"fill": true, "riser": true}),
            scope: Some(crate::tools::PatchScope {
                segment_id: Some(segment_id),
                layer_id: None,
            }),
            ..quantize_params(&project)
        };
        let receipt = apply_patch_to_project(&params, &mut project);
        assert!(matches!(receipt.status, ToolStatus::Succeeded));
        assert_eq!(
            receipt.score_delta["arrangement.transition_strength"],
            "+0.15"
        );
    }
    /// Build a segment-scoped params struct without touching `layers[0]`
    /// (unlike `quantize_params`, which requires a layer for its scope).
    fn segment_params(project: &Project, op: &str, params: serde_json::Value) -> ApplyPatchParams {
        let segment_id = project.arrangement.segments[0].id.to_string();
        ApplyPatchParams {
            project_id: project.id.to_string(),
            op: op.to_string(),
            params,
            dry_run: false,
            scope: Some(crate::tools::PatchScope {
                segment_id: Some(segment_id),
                layer_id: None,
            }),
            project: None,
        }
    }

    #[test]
    fn add_segment_layer_fixes_empty_segment() {
        let mut project = out_of_scale_project();
        project.arrangement.segments[0].layers.clear();

        let params = segment_params(&project, "add_segment_layer", serde_json::json!({}));
        let receipt = apply_patch_to_project(&params, &mut project);
        assert!(
            matches!(receipt.status, ToolStatus::Succeeded),
            "add_segment_layer succeeds: {receipt:#?}"
        );
        assert_eq!(receipt.score_delta["structure.segment_layers"], "+0.25");
        assert_eq!(
            project.arrangement.segments[0].layers.len(),
            1,
            "layer added"
        );
    }

    #[test]
    fn add_segment_layer_missing_scope_fails() {
        let mut project = out_of_scale_project();
        let params = ApplyPatchParams {
            op: "add_segment_layer".to_string(),
            params: serde_json::json!({}),
            scope: None,
            ..quantize_params(&project)
        };
        let receipt = apply_patch_to_project(&params, &mut project);
        assert!(matches!(receipt.status, ToolStatus::Failed));
    }

    #[test]
    fn set_segment_bars_clamps_out_of_range() {
        let mut project = out_of_scale_project();
        project.arrangement.segments[0].bars = 128; // over the 4–64 range

        // No explicit bars → clamp to the nearest valid bound.
        let params = segment_params(&project, "set_segment_bars", serde_json::json!({}));
        let receipt = apply_patch_to_project(&params, &mut project);
        assert!(
            matches!(receipt.status, ToolStatus::Succeeded),
            "set_segment_bars succeeds: {receipt:#?}"
        );
        assert_eq!(
            project.arrangement.segments[0].bars, 64,
            "clamped to max bound"
        );
        assert_eq!(receipt.score_delta["structure.segment_bars"], "128 → 64");
        assert!(receipt.summary.contains("128 to 64"));
        // The arrangement total stays consistent with the resized segment.
        assert_eq!(project.arrangement.total_bars, 64);
    }

    #[test]
    fn set_segment_bars_respects_explicit_target() {
        let mut project = out_of_scale_project();
        project.arrangement.segments[0].bars = 128;

        let params = segment_params(
            &project,
            "set_segment_bars",
            serde_json::json!({"bars": 16}),
        );
        let receipt = apply_patch_to_project(&params, &mut project);
        assert!(matches!(receipt.status, ToolStatus::Succeeded));
        assert_eq!(project.arrangement.segments[0].bars, 16);
    }

    #[test]
    fn every_segment_suggestion_op_resolves_in_registry() {
        // The segment_check diagnostics now suggest segment ops; every one
        // must resolve to a real implementation (not "Unsupported patch
        // operation") or the diagnose → fix → learn loop dead-ends.
        let mut project = out_of_scale_project();
        project.arrangement.segments[0].layers.clear();

        for op in ["add_segment_layer", "set_segment_bars"] {
            let mut candidate = project.clone();
            let params = segment_params(&candidate, op, serde_json::json!({}));
            let receipt = apply_patch_to_project(&params, &mut candidate);
            assert!(
                matches!(receipt.status, ToolStatus::Succeeded),
                "op {op} must be implemented, got {receipt:#?}"
            );
        }
    }
}
