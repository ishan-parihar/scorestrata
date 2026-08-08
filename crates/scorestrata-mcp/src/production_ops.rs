//! In-place production-pipeline mutations (G2–G6).
//!
//! Single source of truth for the mutation logic that the MCP tools wrap in
//! [`ToolReceipt`](crate::tools::ToolReceipt)s and the `ss` CLI persists
//! (load → mutate → save). Every function takes `&mut Project`, mutates it in
//! place, and returns a typed outcome — or a [`ProductionError`] describing
//! exactly which lookup or argument was rejected. Keeping the logic here
//! instead of inside each surface guarantees the CLI and the tools can never
//! drift apart.
//!
//! §11 of the production-pipeline-ux-plan: `set_layer_register` (G2),
//! `repeat_pattern_in_segment` (G3), `write_automation` (G4), `set_track_fx`
//! / `set_track_mix` / `apply_genre_mix_preset` (G5), `apply_arrangement_template`
//! (G6).

use scorestrata_core::domain::{
    Automation, AutomationParam, AutomationPoint, AutomationPointJson, CompressorCfg, FreqBand,
    FxChain, PatternInstance, Project, Register, SendLevels, Tick,
};

/// Why an in-place production mutation was rejected.
///
/// Carries enough detail for each caller to render its own failure surface
/// (receipt warnings for the MCP tools, a plain message for the CLI) without
/// re-running the lookup.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ProductionError {
    /// `Segment '{id}' not found`.
    SegmentNotFound(String),
    /// `Layer '{layer}' not found in segment '{segment}'`.
    LayerNotFound {
        /// The segment id that was searched.
        segment: String,
        /// The layer id that was searched.
        layer: String,
    },
    /// `Pattern '{id}' not found`.
    PatternNotFound(String),
    /// `Unsupported automation param '{param}' (expected cutoff|volume|pan|send)`.
    UnsupportedParam(String),
    /// `total_bars must be > 0`.
    InvalidTotalBars,
}

impl std::fmt::Display for ProductionError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::SegmentNotFound(id) => write!(f, "Segment '{id}' not found"),
            Self::LayerNotFound { segment, layer } => {
                write!(f, "Layer '{layer}' not found in segment '{segment}'")
            }
            Self::PatternNotFound(id) => write!(f, "Pattern '{id}' not found"),
            Self::UnsupportedParam(p) => write!(
                f,
                "Unsupported automation param '{p}' (expected cutoff|volume|pan|send)"
            ),
            Self::InvalidTotalBars => write!(f, "total_bars must be > 0"),
        }
    }
}

impl std::error::Error for ProductionError {}

/// Outcome of a [`set_layer_register`] call.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RegisterOutcome {
    /// The register that was applied.
    pub register: Register,
}

/// Outcome of a [`write_automation`] call.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AutomationOutcome {
    /// The parameter the lane drives.
    pub param: AutomationParam,
    /// Total lanes on the layer after the write.
    pub lanes: usize,
    /// Points written (the caller's input length).
    pub point_count: usize,
}

/// Outcome of an [`apply_arrangement_template`] call.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TemplateOutcome {
    /// `"name bars"` lines, in section order.
    pub layout: Vec<String>,
    /// Number of archetype sections in the template.
    pub section_count: usize,
    /// Total bars the arrangement now spans.
    pub total_bars: u32,
}

/// Outcome of a [`repeat_pattern_in_segment`] call.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RepeatOutcome {
    /// Instances placed.
    pub placed: u32,
    /// Effective spacing between instances in ticks.
    pub step: u64,
    /// Transpose ramp applied per instance (semitones).
    pub transpose_step: i32,
}

/// Set a layer's octave register (G2).
///
/// The compiler clamps out-of-band notes and the validator flags them with
/// `LAYER_NOTE_OUTSIDE_REGISTER`; the timeline view renders the band.
pub fn set_layer_register(
    project: &mut Project,
    segment_id: &str,
    layer_id: &str,
    min_octave: i8,
    max_octave: i8,
) -> Result<RegisterOutcome, ProductionError> {
    let register = Register::new(min_octave, max_octave);
    let segment = project
        .arrangement
        .segments
        .iter_mut()
        .find(|s| s.id.to_string() == segment_id)
        .ok_or_else(|| ProductionError::SegmentNotFound(segment_id.to_string()))?;
    let layer = segment
        .layers
        .iter_mut()
        .find(|l| l.id.to_string() == layer_id)
        .ok_or_else(|| ProductionError::LayerNotFound {
            segment: segment_id.to_string(),
            layer: layer_id.to_string(),
        })?;
    layer.register = Some(register);
    Ok(RegisterOutcome { register })
}

/// Write an automation lane on a layer (G4).
///
/// Appends a new lane or replaces an existing lane for the same parameter.
/// Points are clamped to 0–1 and sorted by tick. The compiler turns the lane
/// into `ControlChange` events; the renderer sweeps the parameter.
pub fn write_automation(
    project: &mut Project,
    segment_id: &str,
    layer_id: &str,
    param: &str,
    points: &[AutomationPointJson],
) -> Result<AutomationOutcome, ProductionError> {
    let param = match param {
        "cutoff" => AutomationParam::Cutoff,
        "volume" => AutomationParam::Volume,
        "pan" => AutomationParam::Pan,
        "send" => AutomationParam::Send,
        other => return Err(ProductionError::UnsupportedParam(other.to_string())),
    };
    let segment = project
        .arrangement
        .segments
        .iter_mut()
        .find(|s| s.id.to_string() == segment_id)
        .ok_or_else(|| ProductionError::SegmentNotFound(segment_id.to_string()))?;
    let layer = segment
        .layers
        .iter_mut()
        .find(|l| l.id.to_string() == layer_id)
        .ok_or_else(|| ProductionError::LayerNotFound {
            segment: segment_id.to_string(),
            layer: layer_id.to_string(),
        })?;
    let mut points: Vec<AutomationPoint> = points
        .iter()
        .map(|p| AutomationPoint {
            tick: Tick::new(p.tick),
            value: p.value.clamp(0.0, 1.0),
        })
        .collect();
    points.sort_by_key(|p| p.tick.0);
    let point_count = points.len();
    // Replace an existing lane for the same param, or append a new one.
    if let Some(existing) = layer.automation.iter_mut().find(|a| a.param == param) {
        existing.points = points;
    } else {
        layer.automation.push(Automation {
            layer_id: layer.id,
            param,
            points,
        });
    }
    Ok(AutomationOutcome {
        param,
        lanes: layer.automation.len(),
        point_count,
    })
}

/// Set a layer's insert FX chain (G5).
///
/// The renderer applies the chain (HPF → EQ → compressor → sends) to the
/// layer's stem.
pub fn set_track_fx(
    project: &mut Project,
    segment_id: &str,
    layer_id: &str,
    fx: &FxChain,
) -> Result<(), ProductionError> {
    let segment = project
        .arrangement
        .segments
        .iter_mut()
        .find(|s| s.id.to_string() == segment_id)
        .ok_or_else(|| ProductionError::SegmentNotFound(segment_id.to_string()))?;
    let layer = segment
        .layers
        .iter_mut()
        .find(|l| l.id.to_string() == layer_id)
        .ok_or_else(|| ProductionError::LayerNotFound {
            segment: segment_id.to_string(),
            layer: layer_id.to_string(),
        })?;
    layer.fx = Some(fx.clone());
    Ok(())
}

/// Set a layer's mixer faders (G5) — the mixer-fader action.
pub fn set_track_mix(
    project: &mut Project,
    segment_id: &str,
    layer_id: &str,
    gain_db: Option<f32>,
    pan: Option<f32>,
) -> Result<(), ProductionError> {
    let segment = project
        .arrangement
        .segments
        .iter_mut()
        .find(|s| s.id.to_string() == segment_id)
        .ok_or_else(|| ProductionError::SegmentNotFound(segment_id.to_string()))?;
    let layer = segment
        .layers
        .iter_mut()
        .find(|l| l.id.to_string() == layer_id)
        .ok_or_else(|| ProductionError::LayerNotFound {
            segment: segment_id.to_string(),
            layer: layer_id.to_string(),
        })?;
    layer.mix_gain_db = gain_db;
    layer.mix_pan = pan;
    Ok(())
}

/// Apply a one-shot production-ready mix preset for a genre (G5).
///
/// Sets per-instrument HPF / EQ / sidechain / sends defaults from the genre
/// table. Returns how many layers received a chain.
pub fn apply_genre_mix_preset(project: &mut Project, genre: &str) -> usize {
    let mut applied = 0usize;
    for segment in &mut project.arrangement.segments {
        for layer in &mut segment.layers {
            let Some(chain) = genre_mix_chain(genre, &layer.instrument) else {
                continue;
            };
            layer.fx = Some(chain);
            applied += 1;
        }
    }
    applied
}

/// Apply a per-genre arrangement template (G6) — build the section skeleton
/// (intro → build → drop → breakdown → outro) with production-grade bar
/// counts and an energy arc.
///
/// Distributes `total_bars` across the archetype sections proportionally:
/// floor each section, then hand the leftover to the largest section so the
/// segments always sum to *exactly* `total_bars` (the drop stays the longest
/// — the DAW convention). u64 math avoids overflow on the multiply.
pub fn apply_arrangement_template(
    project: &mut Project,
    genre: &str,
    total_bars: u32,
) -> Result<TemplateOutcome, ProductionError> {
    if total_bars == 0 {
        return Err(ProductionError::InvalidTotalBars);
    }
    let template = crate::resources::arrangement_template_for(genre);
    let archetype_total: u64 = template.sections.iter().map(|s| u64::from(s.bars)).sum();
    let total = u64::from(total_bars);
    let n_sections = template.sections.len().max(1) as u64;
    let mut bars: Vec<u64> = template
        .sections
        .iter()
        .map(|s| {
            if archetype_total > 0 {
                (u64::from(s.bars) * total / archetype_total).max(1)
            } else {
                (total / n_sections).max(1)
            }
        })
        .collect();
    let allocated: u64 = bars.iter().sum();
    let leftover = total.saturating_sub(allocated);
    // Hand the remainder to the section with the largest archetype share (the
    // drop), preserving proportions and hitting exactly total_bars.
    if leftover > 0 {
        if let Some((idx, _)) = template
            .sections
            .iter()
            .enumerate()
            .max_by_key(|(_, s)| s.bars)
        {
            bars[idx] += leftover;
        }
    }

    let mut segments: Vec<scorestrata_core::domain::Segment> = Vec::new();
    let mut energy_curve: Vec<scorestrata_core::domain::EnergyPoint> = Vec::new();
    let mut start_bar = 0u32;
    for (section, &section_bars) in template.sections.iter().zip(&bars) {
        let role = scorestrata_core::domain::SegmentRole::from_template_role(&section.role);
        // Pre-populate each section with the template's instrument hints as
        // empty stub layers carrying the production octave-register discipline
        // (G2), so the planner and agent see exactly which voices a section
        // needs and in which octave band they belong.
        let layers = section
            .layers
            .iter()
            .map(|instrument| {
                let register = register_for_instrument(instrument);
                scorestrata_core::domain::Layer {
                    id: scorestrata_core::ids::LayerId::new(),
                    name: instrument.clone(),
                    instrument: instrument.clone(),
                    pattern_instances: Vec::new(),
                    register,
                    automation: Vec::new(),
                    fx: None,
                    mix_gain_db: None,
                    mix_pan: None,
                }
            })
            .collect();
        #[expect(
            clippy::cast_possible_truncation,
            reason = "section_bars is a share of u32 total_bars, so it always fits"
        )]
        let section_bars_u32 = section_bars as u32;
        let segment = scorestrata_core::domain::Segment {
            id: scorestrata_core::ids::SegmentId::new(),
            name: section.role.clone(),
            role,
            start_bar,
            bars: section_bars_u32,
            layers,
        };
        energy_curve.push(scorestrata_core::domain::EnergyPoint {
            segment_id: segment.id,
            target_energy: section.energy,
        });
        segments.push(segment);
        start_bar += section_bars_u32;
    }
    // The distribution above guarantees the sections sum to total_bars; verify
    // rather than trust the math (defensive — catches future archetype edits).
    let layout: Vec<String> = segments
        .iter()
        .map(|s| format!("{} {}", s.name, s.bars))
        .collect();
    project.arrangement.segments = segments;
    project.arrangement.energy_curve = energy_curve;
    project.arrangement.total_bars = total_bars;
    Ok(TemplateOutcome {
        layout,
        section_count: template.sections.len(),
        total_bars,
    })
}

/// Repeat a pattern across a segment's layer (G3) — the DAW "paint the
/// pattern across the timeline" action.
///
/// Places `count` instances spaced `step_ticks` apart (defaults to the
/// pattern's own bar length), with an optional per-instance transpose ramp so
/// an 8-bar loop can climb an octave toward the drop.
pub fn repeat_pattern_in_segment(
    project: &mut Project,
    segment_id: &str,
    layer_id: &str,
    pattern_id: &str,
    count: u32,
    step_ticks: u64,
    transpose_step: i32,
) -> Result<RepeatOutcome, ProductionError> {
    // Resolve the pattern once (id + bar length) before mutating the layer,
    // so we never hold two borrows of the project.
    let resolved_pattern = project
        .patterns
        .iter()
        .find(|p| p.id.to_string() == pattern_id)
        .cloned();
    let (resolved_id, pattern_bars) = resolved_pattern
        .as_ref()
        .map_or((scorestrata_core::ids::PatternId::new(), 1u32), |p| {
            (p.id, p.bars.max(1))
        });
    if resolved_pattern.is_none() {
        return Err(ProductionError::PatternNotFound(pattern_id.to_string()));
    }
    let ticks_per_bar = u64::from(scorestrata_core::domain::PPQ)
        * u64::from(project.arrangement.time_signature.numerator);
    let step = if step_ticks > 0 {
        step_ticks
    } else {
        u64::from(pattern_bars) * ticks_per_bar
    };
    let count = count.clamp(1, 256);

    let segment = project
        .arrangement
        .segments
        .iter_mut()
        .find(|s| s.id.to_string() == segment_id)
        .ok_or_else(|| ProductionError::SegmentNotFound(segment_id.to_string()))?;
    let layer = segment
        .layers
        .iter_mut()
        .find(|l| l.id.to_string() == layer_id)
        .ok_or_else(|| ProductionError::LayerNotFound {
            segment: segment_id.to_string(),
            layer: layer_id.to_string(),
        })?;

    // Replace existing instances of this pattern with the repeat sequence
    // (the DAW "paint pattern" overwrites the lane's placements).
    layer
        .pattern_instances
        .retain(|i| i.pattern_id.to_string() != pattern_id);
    let mut placed = 0u32;
    for i in 0..count {
        layer.pattern_instances.push(PatternInstance {
            id: scorestrata_core::ids::PatternInstanceId::new(),
            pattern_id: resolved_id,
            start_tick: Tick::new(step * u64::from(i)),
            transpose: transpose_step.saturating_mul(i as i32),
            velocity_multiplier: 1.0,
        });
        placed += 1;
    }
    Ok(RepeatOutcome {
        placed,
        step,
        transpose_step,
    })
}

/// Genre mix defaults: per-instrument insert chains for a genre.
///
/// Mirrors a producer's starting session: kick clean + sub, bass HPF'd + EQ
/// scooped + sidechained to the kick, pads HPF'd with air, hats cleaned.
/// `pub` so the eval harness can build a preset-applied project and prove
/// the render moves toward the genre reference profile (§12 test 37) — the
/// exact chain the `apply_genre_mix_preset` tool applies.
#[must_use]
pub fn genre_mix_chain(genre: &str, instrument: &str) -> Option<FxChain> {
    let g = genre.to_lowercase();
    let i = instrument.to_lowercase();
    let sidechain = matches!(
        g.as_str(),
        "techno" | "house" | "melodic_techno" | "dark_techno"
    );
    let chain = match i.as_str() {
        "drum_sampler" | "drums" | "kick" => FxChain {
            high_pass_hz: Some(30.0),
            compressor: Some(CompressorCfg {
                threshold_db: -12.0,
                ratio: 3.0,
                attack_ms: 2.0,
                release_ms: 80.0,
                makeup_db: 4.0,
            }),
            ..Default::default()
        },
        "bass_synth" | "bass" => FxChain {
            high_pass_hz: Some(40.0),
            eq: vec![FreqBand {
                freq_hz: 200.0,
                gain_db: -2.0,
                q: 1.0,
            }],
            sidechain_from_kick: sidechain,
            ..Default::default()
        },
        "pad_synth" | "pad" | "chord" => FxChain {
            high_pass_hz: Some(120.0),
            eq: vec![FreqBand {
                freq_hz: 350.0,
                gain_db: -2.0,
                q: 1.0,
            }],
            sends: SendLevels {
                reverb: 0.3,
                delay: 0.0,
            },
            ..Default::default()
        },
        "lead_synth" | "lead" | "bell" => FxChain {
            high_pass_hz: Some(150.0),
            eq: vec![FreqBand {
                freq_hz: 2500.0,
                gain_db: 2.0,
                q: 1.2,
            }],
            sends: SendLevels {
                reverb: 0.15,
                delay: 0.2,
            },
            ..Default::default()
        },
        "hat" | "percussion" | "clap" | "snare" => FxChain {
            high_pass_hz: Some(500.0),
            ..Default::default()
        },
        _ => return None,
    };
    Some(chain)
}

/// The production octave-register defaults (G2) for an instrument family.
///
/// Mirrors the DAW discipline: bass lives in octaves 1–2, chords/pads 3–5,
/// leads 5–7; drums are unpitched and get no register. Used to pre-seed stub
/// layers from an arrangement template so the agent sees the register contract
/// before writing any notes.
#[must_use]
pub fn register_for_instrument(instrument: &str) -> Option<Register> {
    let lower = instrument.to_lowercase();
    if lower.contains("drum") || lower.contains("perc") || lower.contains("hat") {
        None
    } else if lower.contains("bass") || lower.contains("sub") {
        Some(Register::new(1, 2))
    } else if lower.contains("pad")
        || lower.contains("chord")
        || lower.contains("string")
        || lower.contains("key")
    {
        Some(Register::new(3, 5))
    } else if lower.contains("lead")
        || lower.contains("pluck")
        || lower.contains("bell")
        || lower.contains("synth")
    {
        Some(Register::new(5, 7))
    } else {
        Some(Register::new(3, 6))
    }
}

/// Human-readable name for an automation param.
#[must_use]
pub fn param_str(param: &AutomationParam) -> String {
    match param {
        AutomationParam::Cutoff => "cutoff".to_string(),
        AutomationParam::Volume => "volume".to_string(),
        AutomationParam::Pan => "pan".to_string(),
        AutomationParam::Send => "send".to_string(),
    }
}

/// Compact human summary of an FX chain, e.g. `hpf 40 Hz, eq 1 bands,
/// sidechain` — used in receipts and CLI output.
#[must_use]
pub fn fx_summary(fx: &FxChain) -> String {
    let mut parts: Vec<String> = Vec::new();
    if let Some(hz) = fx.high_pass_hz {
        parts.push(format!("hpf {hz:.0} Hz"));
    }
    if !fx.eq.is_empty() {
        parts.push(format!("eq {} bands", fx.eq.len()));
    }
    if fx.compressor.is_some() {
        parts.push("comp".to_string());
    }
    if fx.sidechain_from_kick {
        parts.push("sidechain".to_string());
    }
    if fx.sends.reverb > 0.0 {
        parts.push(format!("reverb {:.2}", fx.sends.reverb));
    }
    if fx.sends.delay > 0.0 {
        parts.push(format!("delay {:.2}", fx.sends.delay));
    }
    if parts.is_empty() {
        "clean".to_string()
    } else {
        parts.join(", ")
    }
}
