//! Offline render engine — renders a timeline IR to audio.
//!
//! Wires Phase 5 exit criteria together: synth instruments, mixer, and WAV
//! writer into a deterministic offline render pipeline. Renders are
//! reproducible given the same timeline, sample rate, and synth parameters
//! (per §17 DSP determinism rules).
//!
//! # Determinism
//!
//! - fixed sample rate and block handling,
//! - deterministic event ordering (timeline events are already sorted),
//! - deterministic summing order (per-layer accumulation in event order),
//! - no wall-clock dependence.

// The renderer converts tick counts and sample counts between u64/u32 and
// f32/f64 for timing math; durations are bounded by practical project
// lengths, so the pedantic cast lints are expected.
#![expect(
    clippy::cast_precision_loss,
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    clippy::float_cmp,
    clippy::missing_errors_doc,
    reason = "Render timing math casts bounded tick/sample counts; durations fit practical ranges; width/level guards are exact-DSP comparisons"
)]

use scorestrata_core::domain::PPQ;
use scorestrata_core::error::ScorestrataError;

use crate::analysis::AudioAnalysis;
use crate::mastering::{LimiterConfig, apply_limiter, apply_master_chain, dither_buffer};
use crate::mixer::{Mixer, MixerChannel};
use crate::synth::{SynthParams, Waveform, drum_kind_for_pitch, render_drum_note, render_note};
use crate::wav::{write_wav, write_wav_stereo};

/// Sample rate used for offline rendering (48 kHz per render policy).
pub const DEFAULT_SAMPLE_RATE: u32 = 48_000;

/// The audio engine crate version, surfaced in render manifests.
///
/// §17.2 requires the manifest to record the engine version so that
/// reproducibility checks can detect a version drift that silently changes
/// output bytes.
pub const AUDIO_ENGINE_VERSION: &str = env!("CARGO_PKG_VERSION");

/// Synth voice configuration for a layer name.
#[derive(Debug, Clone)]
pub struct RenderVoice {
    /// Layer name this voice applies to.
    pub layer_name: String,
    /// Layer instrument name (e.g. `distorted_guitar`) — used to pick the
    /// multi-sample voice when the melodic bank is enabled.
    pub instrument: String,
    /// Synth parameters.
    pub params: SynthParams,
    /// Whether the layer is a percussion (drum) layer — notes route through
    /// the GM drum synthesizer instead of the pitched oscillator.
    pub is_percussion: bool,
}

impl RenderVoice {
    /// Create a voice with the default synth parameters.
    #[must_use]
    pub fn default_for(layer_name: &str) -> Self {
        Self {
            layer_name: layer_name.to_string(),
            instrument: layer_name.to_string(),
            params: SynthParams::default(),
            is_percussion: false,
        }
    }
}

/// Render options controlling the offline mix.
#[derive(Debug, Clone)]
pub struct RenderConfig {
    /// Whether to write a stereo WAV (interleaved L/R) instead of a mono fold.
    pub stereo: bool,
    /// Peak limiter applied to the master mix; `None` disables limiting.
    pub limiter: Option<LimiterConfig>,
    /// Master output gain in dB applied to the folded mix (G5, §8.6
    /// `MixerState::master_gain_db`); 0.0 = unity.
    pub master_gain_db: f32,
    /// Stereo width of the master bus (mid/side): 1.0 = neutral, 0.0 =
    /// mono fold, >1.0 widens the side channel. The mono fold is invariant
    /// under this transform, so mono renders are unaffected.
    pub stereo_width: f32,
    /// Opt-in sampled percussion kit (A): `Some("rock")` etc. routes drum
    /// voices through the embedded sample bank; `None` keeps the legacy
    /// synth-drum path (R1 — bankless renders stay byte-identical).
    pub sound_bank: Option<String>,
    /// Send/return + group-bus routing graph (B). `None` = flat per-layer
    /// fold, byte-identical to pre-bus renders (R1).
    pub bus_graph: Option<scorestrata_core::domain::BusGraph>,
    /// Phase D mastering chain (multiband → saturation → limiter). `None` =
    /// the legacy width/gain/limiter path, byte-identical to pre-Phase-D
    /// renders (R1).
    pub master_chain: Option<scorestrata_core::domain::MasterChain>,
    /// PCM export depth for the WAV (16 or 24). The master chain's dither
    /// `bits` wins when a chain is present; otherwise this applies.
    pub bits_per_sample: u16,
    /// Render sample rate (44100 / 48000 / 96000, plan §3.3). The engine
    /// is sample-rate parametric end-to-end, so a render at any rate is
    /// deterministic; defaults to [`DEFAULT_SAMPLE_RATE`] (48 kHz, R1).
    pub sample_rate: u32,
    /// Opt-in melodic instrument bank (G): `Some(_)` routes guitar/bass/
    /// e-piano layer families through the embedded multi-sample bank;
    /// `None` keeps the subtractive synth voices byte-identical (R1).
    pub melodic_bank: Option<String>,
}

impl Default for RenderConfig {
    fn default() -> Self {
        Self {
            stereo: false,
            limiter: Some(LimiterConfig::default()),
            master_gain_db: 0.0,
            stereo_width: 1.0,
            sound_bank: None,
            bus_graph: None,
            master_chain: None,
            bits_per_sample: 16,
            sample_rate: DEFAULT_SAMPLE_RATE,
            melodic_bank: None,
        }
    }
}

/// A full layer render: per-layer stems plus the mastered fold.
#[derive(Debug, Clone)]
pub struct LayerRender {
    /// Per-layer stems in deterministic layer order.
    pub stems: Vec<Stem>,
    /// The mastered master render.
    pub master: RenderResult,
}

/// Keyed render cache (plan §6 Phase E exit criteria: "audit without full
/// re-render").
///
/// Content-addressed on the timeline input hash **plus** the render-config
/// signature, so a hit implies the exact same engine inputs — reusing the
/// cached result is byte-identical to re-rendering (R2). A project edit or
/// config change misses and re-renders.
#[derive(Debug, Clone, Default)]
pub struct RenderCache {
    entries: std::collections::BTreeMap<String, LayerRender>,
}

impl RenderCache {
    /// An empty cache.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Look up a render by its cache key.
    #[must_use]
    pub fn get(&self, key: &str) -> Option<&LayerRender> {
        self.entries.get(key)
    }

    /// Insert a render under its cache key.
    pub fn insert(&mut self, key: String, render: LayerRender) {
        self.entries.insert(key, render);
    }

    /// Number of cached renders.
    #[must_use]
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// Whether the cache is empty.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }
}

/// Deterministic cache key for a timeline render: the compiler's input hash
/// plus a signature of every config field that changes the output bytes.
///
/// Floats are keyed by their bits so any change (even a tiny dB tweak)
/// misses; `BusGraph`/`MasterChain` are serialized canonically by `serde_json`.
#[must_use]
pub fn render_cache_key(input_hash: &str, config: &RenderConfig) -> String {
    use sha2::{Digest, Sha256};
    let mut buf = String::with_capacity(256);
    buf.push_str(input_hash);
    buf.push('|');
    buf.push_str(if config.stereo { "s1" } else { "s0" });
    buf.push('|');
    buf.push_str(&config.sample_rate.to_string());
    buf.push('|');
    buf.push_str(&config.bits_per_sample.to_string());
    buf.push('|');
    buf.push_str(&config.master_gain_db.to_bits().to_string());
    buf.push('|');
    buf.push_str(&config.stereo_width.to_bits().to_string());
    if let Some(lim) = config.limiter {
        buf.push('|');
        buf.push_str(&lim.ceiling_db.to_bits().to_string());
        buf.push(':');
        buf.push_str(&lim.knee_db.to_bits().to_string());
    }
    if let Some(bank) = &config.sound_bank {
        buf.push('|');
        buf.push_str(bank);
    }
    if let Some(bank) = &config.melodic_bank {
        buf.push('|');
        buf.push_str(bank);
    }
    if let Some(graph) = &config.bus_graph {
        buf.push('|');
        buf.push_str(&serde_json::to_string(graph).unwrap_or_default());
    }
    if let Some(chain) = &config.master_chain {
        buf.push('|');
        buf.push_str(&serde_json::to_string(chain).unwrap_or_default());
    }
    hex::encode(Sha256::digest(buf.as_bytes()))
}

/// Apply the master-bus state (§8.6 `MixerState`, G5) to the folded mix:
/// stereo width (mid/side) then master gain, before the master limiter so
/// the ceiling still protects the final level.
///
/// The width transform keeps `(L+R)/2` invariant, so mono renders are not
/// affected by width changes — only the stereo pair widens.
fn apply_master_bus(
    left: Vec<f32>,
    right: Vec<f32>,
    config: &RenderConfig,
) -> (Vec<f32>, Vec<f32>) {
    let mut left = left;
    let mut right = right;
    if config.stereo_width != 1.0 {
        for (l, r) in left.iter_mut().zip(&mut right) {
            let mid = (*l + *r) * 0.5;
            let side = (*l - *r) * 0.5 * config.stereo_width;
            *l = mid + side;
            *r = mid - side;
        }
    }
    if config.master_gain_db != 0.0 {
        let gain = db_to_linear(config.master_gain_db);
        for sample in &mut left {
            *sample *= gain;
        }
        for sample in &mut right {
            *sample *= gain;
        }
    }
    (left, right)
}

/// A note to render, at absolute tick positions.
#[derive(Debug, Clone)]
pub struct RenderNote {
    /// MIDI note pitch.
    pub pitch: u8,
    /// Start tick.
    pub start_tick: u64,
    /// Duration in ticks.
    pub duration_ticks: u64,
    /// Velocity (0.0–1.0).
    pub velocity: f32,
    /// Layer name for voice assignment.
    pub layer_name: String,
}

/// A per-layer insert FX + mix override feeding the renderer (G5).
///
/// Conversion sites read the project's `Layer.fx` / `mix_gain_db` /
/// `mix_pan` and translate them into these, so the audio engine stays
/// timeline- and project-agnostic.
#[derive(Debug, Clone)]
pub struct RenderFx {
    /// Layer name this applies to.
    pub layer_name: String,
    /// Insert FX chain (HPF → EQ → compressor → sends).
    pub fx: scorestrata_core::domain::FxChain,
    /// Mix gain override in dB; `None` = static gain staging.
    pub mix_gain_db: Option<f32>,
    /// Mix pan override; `None` = static panning.
    pub mix_pan: Option<f32>,
    /// Group-bus assignment (B); `None` = direct to stereo sum.
    pub group: Option<String>,
}

/// A CC automation lane feeding the renderer (G4).
///
/// Mirrors the domain `Automation` entity but keyed by the MIDI CC number
/// (cutoff=74, volume=7, pan=10, send=91) so the audio engine stays
/// timeline-agnostic: conversion sites translate `ControlChange` events into
/// these lanes before rendering.
#[derive(Debug, Clone)]
pub struct RenderAutomation {
    /// Layer name this lane drives.
    pub layer_name: String,
    /// MIDI CC controller number.
    pub controller: u8,
    /// Interpolation points: (tick, value 0.0–1.0), sorted by tick.
    pub points: Vec<(u64, f32)>,
}

impl RenderAutomation {
    /// Interpolated value at `tick` (linear between points, clamped to the
    /// curve ends), or `None` if the lane has no points.
    #[must_use]
    pub fn value_at(&self, tick: u64) -> Option<f32> {
        if self.points.is_empty() {
            return None;
        }
        let first = self.points[0];
        if tick <= first.0 {
            return Some(first.1);
        }
        let last = self.points[self.points.len() - 1];
        if tick >= last.0 {
            return Some(last.1);
        }
        for pair in self.points.windows(2) {
            let (a, b) = (pair[0], pair[1]);
            if tick >= a.0 && tick <= b.0 {
                let span = (b.0 - a.0).max(1) as f32;
                let frac = (tick - a.0) as f32 / span;
                return Some((b.1 - a.1).mul_add(frac, a.1));
            }
        }
        Some(last.1)
    }
}

/// The result of an offline render.
#[derive(Debug, Clone)]
pub struct RenderResult {
    /// Stereo WAV file bytes.
    pub wav_bytes: Vec<u8>,
    /// Total duration in seconds.
    pub duration_seconds: f64,
    /// Sample rate used.
    pub sample_rate: u32,
    /// Number of channels (1 for the mono master fold).
    pub channels: u16,
    /// SHA-256 of the WAV bytes (deterministic checksum).
    pub checksum: String,
    /// The mono mix samples (L+R)/2 — enables Level-4 analysis downstream.
    pub mono_samples: Vec<f32>,
    /// Level-4 audio analysis of this render (LUFS, true peak, …).
    pub analysis: AudioAnalysis,
}

/// A single rendered stem (one layer, mono).
#[derive(Debug, Clone)]
pub struct Stem {
    /// Layer name this stem belongs to.
    pub layer_name: String,
    /// Mono WAV bytes for this layer.
    pub wav_bytes: Vec<u8>,
    /// Mono samples for this layer.
    pub samples: Vec<f32>,
    /// Duration in seconds.
    pub duration_seconds: f64,
    /// SHA-256 of the WAV bytes (deterministic checksum).
    pub checksum: String,
    /// Level-4 analysis of this stem.
    pub analysis: AudioAnalysis,
}

/// A per-layer stereo stem (Phase B — `render_stereo_stems`, replaces the
/// mono fold for DAW-style export).
///
/// L/R are derived from the same deterministic mono layer buffer via the
/// constant-power pan law used by the mixer, so the stems' summed stereo
/// field matches the master's layer placement while the master remains the
/// authoritative mixdown.
#[derive(Debug, Clone)]
pub struct StereoStem {
    /// Layer name this stem belongs to.
    pub layer_name: String,
    /// Interleaved stereo WAV bytes (16-bit PCM).
    pub wav_bytes: Vec<u8>,
    /// Left-channel samples.
    pub left: Vec<f32>,
    /// Right-channel samples.
    pub right: Vec<f32>,
    /// Duration in seconds.
    pub duration_seconds: f64,
    /// SHA-256 of the WAV bytes (deterministic checksum).
    pub checksum: String,
    /// Level-4 stereo analysis of this stem.
    pub analysis: AudioAnalysis,
}

/// Convert ticks to sample index at the given BPM and sample rate.
#[must_use]
pub fn tick_to_sample(tick: u64, bpm: u32, sample_rate: u32) -> u64 {
    let seconds = tick_to_seconds(tick, bpm);
    (seconds * f64::from(sample_rate)) as u64
}

/// Convert ticks to seconds at the given BPM.
#[must_use]
#[expect(
    clippy::cast_precision_loss,
    reason = "Tick counts fit f64 for practical durations"
)]
pub fn tick_to_seconds(tick: u64, bpm: u32) -> f64 {
    let beats = tick as f64 / f64::from(PPQ);
    beats * 60.0 / f64::from(bpm)
}

/// Render a set of notes into WAV bytes with the default render config.
///
/// Notes are grouped into per-layer mono buffers, rendered with the assigned
/// voice (or the default synth voice), mixed, peak-limited, and encoded as
/// a 16-bit WAV.
///
/// # Errors
///
/// Returns an error if `sample_rate` is 0.
pub fn render_notes(
    notes: &[RenderNote],
    bpm: u32,
    sample_rate: u32,
    voices: &[RenderVoice],
) -> Result<RenderResult, ScorestrataError> {
    render_notes_with_config(notes, bpm, sample_rate, voices, &RenderConfig::default())
}

/// Render a set of notes with an explicit render config (stereo / limiter).
///
/// # Errors
///
/// Returns an error if `sample_rate` is 0.
pub fn render_notes_with_config(
    notes: &[RenderNote],
    bpm: u32,
    sample_rate: u32,
    voices: &[RenderVoice],
    config: &RenderConfig,
) -> Result<RenderResult, ScorestrataError> {
    render_notes_with_automation(notes, bpm, sample_rate, voices, &[], config)
}

/// Render notes with CC automation lanes (G4) under an explicit config.
///
/// Automation sweeps are applied per-layer at each note's start tick:
///
/// - CC 74 (cutoff): overrides the voice's filter cutoff (0.0 → darkest,
///   1.0 → fully open) so build-ups audibly open up.
/// - CC 7 (volume): scales the note buffer so levels move over time.
///
/// All interpolation is linear and deterministic; rendering with the same
/// inputs yields byte-identical output (per §17 DSP determinism).
pub fn render_notes_with_automation(
    notes: &[RenderNote],
    bpm: u32,
    sample_rate: u32,
    voices: &[RenderVoice],
    automation: &[RenderAutomation],
    config: &RenderConfig,
) -> Result<RenderResult, ScorestrataError> {
    render_notes_full(notes, bpm, sample_rate, voices, automation, &[], config)
}

/// Render notes with automation lanes **and** per-layer FX / mix (G5).
///
/// Per-layer insert chains (HPF/EQ/compressor/sends) and gain/pan overrides
/// are applied deterministically after each layer buffer is synthesized,
/// then mixed with the static gain staging unless overridden.
#[expect(
    clippy::too_many_lines,
    reason = "Full render pipeline composes ~30 deterministic steps; splitting adds indirection without behavior change"
)]
pub fn render_notes_full(
    notes: &[RenderNote],
    bpm: u32,
    sample_rate: u32,
    voices: &[RenderVoice],
    automation: &[RenderAutomation],
    fx: &[RenderFx],
    config: &RenderConfig,
) -> Result<RenderResult, ScorestrataError> {
    if sample_rate == 0 {
        return Err(ScorestrataError::RenderError {
            message: "sample rate must be non-zero".to_string(),
        });
    }

    // Deterministic layer ordering: collect layer names in first-seen order.
    let mut layer_order: Vec<String> = Vec::new();
    for note in notes {
        if !layer_order.contains(&note.layer_name) {
            layer_order.push(note.layer_name.clone());
        }
    }

    // Total duration in samples: last note end plus the longest release tail.
    let last_note_end = notes
        .iter()
        .map(|n| {
            let start = tick_to_sample(n.start_tick, bpm, sample_rate);
            let dur = tick_to_sample(n.duration_ticks, bpm, sample_rate);
            start + dur
        })
        .max()
        .unwrap_or(0);
    let total_samples = if last_note_end == 0 {
        0
    } else {
        last_note_end + release_tail_samples(voices, sample_rate)
    };

    let mut mixer = Mixer::new();
    let mut bus_layers: Vec<crate::bus::BusLayerInput> = Vec::new();

    for layer_name in &layer_order {
        let layer_automation: Vec<&RenderAutomation> = automation
            .iter()
            .filter(|a| a.layer_name == *layer_name)
            .collect();
        let layer_fx = fx.iter().find(|f| f.layer_name == *layer_name);
        let kit = config
            .sound_bank
            .as_deref()
            .and_then(crate::samplebank::kit);
        let melodic_bank = melodic_bank_for(config);
        let mut buffer = render_layer_buffer(
            notes,
            layer_name,
            voices,
            bpm,
            sample_rate,
            total_samples,
            &layer_automation,
            kit.as_ref(),
            melodic_bank.as_ref(),
        );
        // G5 — insert FX chain (HPF → EQ → compressor). The reverb/delay
        // sends stay out of the inline fold when the bus graph (B) is
        // enabled — they route to the shared returns instead.
        if let Some(rfx) = layer_fx {
            crate::fx::apply_fx_chain(&mut buffer, &rfx.fx, sample_rate);
            if config.bus_graph.is_none() {
                crate::fx::apply_sends(&mut buffer, &rfx.fx.sends, sample_rate);
            }
        }
        let gain = layer_fx
            .and_then(|f| f.mix_gain_db)
            .map_or_else(|| layer_gain(layer_name), db_to_linear);
        let pan = layer_fx
            .and_then(|f| f.mix_pan)
            .unwrap_or_else(|| layer_pan(layer_name));
        if config.bus_graph.is_some() {
            // (B) Collect layers for bus routing; the graph handles sends.
            bus_layers.push(crate::bus::BusLayerInput {
                name: layer_name.clone(),
                group: layer_fx.and_then(|f| f.group.clone()),
                data: buffer,
                gain,
                pan: if config.stereo { pan } else { 0.0 },
                sends: layer_fx.map_or_else(Default::default, |f| f.fx.sends),
            });
        } else {
            mixer.add_channel(MixerChannel {
                name: layer_name.clone(),
                gain,
                pan: if config.stereo { pan } else { 0.0 },
                muted: false,
                data: buffer,
            });
        }
    }

    let (left, right) = config.bus_graph.as_ref().map_or_else(
        || mixer.mix(),
        |graph| crate::bus::mix_bus_graph(&bus_layers, graph, sample_rate, config.stereo),
    );
    // G5 — master bus: stereo width + master gain from `Project.mixer`,
    // applied before the limiter so the ceiling still protects the level.
    let (left, right) = apply_master_bus(left, right, config);
    let mono: Vec<f32> = left
        .iter()
        .zip(&right)
        .map(|(l, r)| (l + r) * 0.5)
        .collect();
    let duration_seconds = f64::from(total_samples as u32) / f64::from(sample_rate);

    if config.stereo {
        let mut left = left;
        let mut right = right;
        let bits = match &config.master_chain {
            // Phase D: the mastering chain owns the limiter and export depth.
            // A present-but-disabled chain falls through to the legacy path
            // so renders stay byte-identical (R1).
            Some(chain) if chain.enabled => {
                apply_master_chain(
                    &mut left,
                    &mut right,
                    chain,
                    sample_rate,
                    config.limiter.as_ref(),
                );
                let dither = chain.dither.as_ref();
                if let Some(d) = dither.filter(|d| d.enabled) {
                    dither_buffer(&mut left, d);
                    dither_buffer(&mut right, d);
                }
                dither.map_or(config.bits_per_sample, |d| d.bits)
            }
            // R1: legacy limiter path, 16-bit by default.
            _ => {
                if let Some(limiter) = &config.limiter {
                    apply_limiter(&mut left, limiter);
                    apply_limiter(&mut right, limiter);
                }
                config.bits_per_sample
            }
        };
        let wav_bytes = write_wav_stereo(&left, &right, sample_rate, bits);
        let checksum = sha256_hex(&wav_bytes);
        let analysis = crate::analysis::analyze_stereo(&left, &right);
        // `mono_samples` must always be the *mastered* fold so it agrees with
        // `analysis.true_peak_dbtp` regardless of the stereo setting.
        let mono = left
            .iter()
            .zip(&right)
            .map(|(l, r)| (l + r) * 0.5)
            .collect();
        Ok(RenderResult {
            wav_bytes,
            duration_seconds,
            sample_rate,
            channels: 2,
            checksum,
            mono_samples: mono,
            analysis,
        })
    } else {
        let mut mono = mono;
        let bits = match &config.master_chain {
            // A present-but-disabled chain falls through to the legacy path (R1).
            Some(chain) if chain.enabled => {
                // The chain is stereo; processing an identical L/R pair leaves
                // a mono fold unchanged (M/S width of equal signals is
                // unity), so the left channel holds the mastered mono after.
                let mut right = mono.clone();
                apply_master_chain(&mut mono, &mut right, chain, sample_rate, None);
                let dither = chain.dither.as_ref();
                if let Some(d) = dither.filter(|d| d.enabled) {
                    dither_buffer(&mut mono, d);
                }
                dither.map_or(config.bits_per_sample, |d| d.bits)
            }
            _ => {
                if let Some(limiter) = &config.limiter {
                    apply_limiter(&mut mono, limiter);
                }
                config.bits_per_sample
            }
        };
        let wav_bytes = write_wav(&mono, sample_rate, bits);
        let checksum = sha256_hex(&wav_bytes);
        let analysis = crate::analysis::analyze_mono(&mono);
        Ok(RenderResult {
            wav_bytes,
            duration_seconds,
            sample_rate,
            channels: 1,
            checksum,
            mono_samples: mono,
            analysis,
        })
    }
}

/// Render each layer to its own mono WAV (stems), plus a mixed master.
///
/// Deterministic stems per §16: each layer is rendered with its assigned
/// voice and encoded as a separate WAV, so downstream tools can export
/// per-layer artifacts or inspect layer-level loudness.
///
/// # Errors
///
/// Returns an error if `sample_rate` is 0.
pub fn render_stems(
    notes: &[RenderNote],
    bpm: u32,
    sample_rate: u32,
    voices: &[RenderVoice],
) -> Result<(Vec<Stem>, RenderResult), ScorestrataError> {
    render_stems_with_config(notes, bpm, sample_rate, voices, &RenderConfig::default())
}

/// Render per-layer mono stems plus a mastered mix with an explicit config.
///
/// Each stem is peak-limited at the same ceiling as the master (per-layer
/// overlapping notes can sum past full scale, so stems never hard-clip).
/// Note the stems are individually limited, so summing them does not
/// reproduce the mastered master exactly — that is expected for a stem
/// export; the master is the authoritative mixdown.
///
/// # Errors
///
/// Returns an error if `sample_rate` is 0.
pub fn render_stems_with_config(
    notes: &[RenderNote],
    bpm: u32,
    sample_rate: u32,
    voices: &[RenderVoice],
    config: &RenderConfig,
) -> Result<(Vec<Stem>, RenderResult), ScorestrataError> {
    render_stems_with_automation(notes, bpm, sample_rate, voices, &[], config)
}

/// Render stems with CC automation lanes (G4) under an explicit config.
///
/// Per-layer automation is applied identically to the master render so stems
/// and master stay in agreement.
pub fn render_stems_with_automation(
    notes: &[RenderNote],
    bpm: u32,
    sample_rate: u32,
    voices: &[RenderVoice],
    automation: &[RenderAutomation],
    config: &RenderConfig,
) -> Result<(Vec<Stem>, RenderResult), ScorestrataError> {
    render_stems_full(notes, bpm, sample_rate, voices, automation, &[], config)
}

/// Render stems with automation **and** per-layer FX / mix (G5).
#[expect(
    clippy::too_many_lines,
    reason = "Stem pipeline mirrors render_notes_full; splitting would duplicate the deterministic chain"
)]
pub fn render_stems_full(
    notes: &[RenderNote],
    bpm: u32,
    sample_rate: u32,
    voices: &[RenderVoice],
    automation: &[RenderAutomation],
    fx: &[RenderFx],
    config: &RenderConfig,
) -> Result<(Vec<Stem>, RenderResult), ScorestrataError> {
    if sample_rate == 0 {
        return Err(ScorestrataError::RenderError {
            message: "sample rate must be non-zero".to_string(),
        });
    }

    // Deterministic layer ordering: collect layer names in first-seen order.
    let mut layer_order: Vec<String> = Vec::new();
    for note in notes {
        if !layer_order.contains(&note.layer_name) {
            layer_order.push(note.layer_name.clone());
        }
    }

    let last_note_end = notes
        .iter()
        .map(|n| {
            let start = tick_to_sample(n.start_tick, bpm, sample_rate);
            let dur = tick_to_sample(n.duration_ticks, bpm, sample_rate);
            start + dur
        })
        .max()
        .unwrap_or(0);
    let total_samples = if last_note_end == 0 {
        0
    } else {
        last_note_end + release_tail_samples(voices, sample_rate)
    };

    let mut stems = Vec::new();
    let mut mixer = Mixer::new();
    let mut bus_layers: Vec<crate::bus::BusLayerInput> = Vec::new();
    // (G) Build the melodic bank once for the whole render.
    let melodic_bank = melodic_bank_for(config);

    for layer_name in &layer_order {
        let layer_automation: Vec<&RenderAutomation> = automation
            .iter()
            .filter(|a| a.layer_name == *layer_name)
            .collect();
        let layer_fx = fx.iter().find(|f| f.layer_name == *layer_name);
        let kit = config
            .sound_bank
            .as_deref()
            .and_then(crate::samplebank::kit);
        let mut buffer = render_layer_buffer(
            notes,
            layer_name,
            voices,
            bpm,
            sample_rate,
            total_samples,
            &layer_automation,
            kit.as_ref(),
            melodic_bank.as_ref(),
        );
        // G5 — insert FX chain, mirroring the master render so stems and
        // master stay in agreement.
        if let Some(rfx) = layer_fx {
            crate::fx::apply_fx_chain(&mut buffer, &rfx.fx, sample_rate);
            if config.bus_graph.is_none() {
                crate::fx::apply_sends(&mut buffer, &rfx.fx.sends, sample_rate);
            }
        }

        // Stem artifact (mono WAV per layer). Per-layer overlapping notes can
        // sum past full scale, so each stem is peak-limited before encoding —
        // the same ceiling as the master — so exported stems never hard-clip
        // (the first-listen defect: the master was protected but stems were not).
        if let Some(limiter) = &config.limiter {
            apply_limiter(&mut buffer, limiter);
        }
        let stem_wav = write_wav(&buffer, sample_rate, 16);
        let stem_checksum = sha256_hex(&stem_wav);
        let stem_analysis = crate::analysis::analyze_mono(&buffer);
        stems.push(Stem {
            layer_name: layer_name.clone(),
            wav_bytes: stem_wav,
            samples: buffer.clone(),
            duration_seconds: f64::from(total_samples as u32) / f64::from(sample_rate),
            checksum: stem_checksum,
            analysis: stem_analysis,
        });

        let gain = layer_fx
            .and_then(|f| f.mix_gain_db)
            .map_or_else(|| layer_gain(layer_name), db_to_linear);
        let pan = layer_fx
            .and_then(|f| f.mix_pan)
            .unwrap_or_else(|| layer_pan(layer_name));
        if config.bus_graph.is_some() {
            bus_layers.push(crate::bus::BusLayerInput {
                name: layer_name.clone(),
                group: layer_fx.and_then(|f| f.group.clone()),
                data: buffer,
                gain,
                pan: if config.stereo { pan } else { 0.0 },
                sends: layer_fx.map_or_else(Default::default, |f| f.fx.sends),
            });
        } else {
            mixer.add_channel(MixerChannel {
                name: layer_name.clone(),
                gain,
                pan: if config.stereo { pan } else { 0.0 },
                muted: false,
                data: buffer,
            });
        }
    }

    let (left, right) = config.bus_graph.as_ref().map_or_else(
        || mixer.mix(),
        |graph| crate::bus::mix_bus_graph(&bus_layers, graph, sample_rate, config.stereo),
    );
    // G5 — master bus, mirroring the master render so stems/master agree.
    let (left, right) = apply_master_bus(left, right, config);
    let mut mono: Vec<f32> = left
        .iter()
        .zip(&right)
        .map(|(l, r)| (l + r) * 0.5)
        .collect();
    let duration_seconds = f64::from(total_samples as u32) / f64::from(sample_rate);

    let (master_wav, channels, master_analysis, mono_fold) = if config.stereo {
        let mut left = left;
        let mut right = right;
        if let Some(limiter) = &config.limiter {
            apply_limiter(&mut left, limiter);
            apply_limiter(&mut right, limiter);
        }
        let mono_fold: Vec<f32> = left
            .iter()
            .zip(&right)
            .map(|(l, r)| (l + r) * 0.5)
            .collect();
        (
            write_wav_stereo(&left, &right, sample_rate, 16),
            2,
            crate::analysis::analyze_stereo(&left, &right),
            mono_fold,
        )
    } else {
        if let Some(limiter) = &config.limiter {
            apply_limiter(&mut mono, limiter);
        }
        (
            write_wav(&mono, sample_rate, 16),
            1,
            crate::analysis::analyze_mono(&mono),
            mono,
        )
    };
    let master_checksum = sha256_hex(&master_wav);
    let master = RenderResult {
        wav_bytes: master_wav,
        duration_seconds,
        sample_rate,
        channels,
        checksum: master_checksum,
        mono_samples: mono_fold,
        analysis: master_analysis,
    };

    Ok((stems, master))
}

/// Render per-layer **stereo** stems plus the mastered mix (Phase B).
///
/// Replaces the mono-fold stem export with true L/R stems: each layer's
/// deterministic mono buffer is panned into the stereo field with the same
/// constant-power law the mixer uses (`pan_norm = midpoint(pan, 1.0)`), so
/// the stems reassemble into the master's stereo placement. The master is
/// still the authoritative mixdown; stems carry the layer audio for DAW
/// rebalance.
///
/// The layer's pan comes from `fx[].mix_pan` when present, else the
/// deterministic `layer_pan` default. Gain is deliberately *not* baked into
/// the stem (mirroring the mono stem, which is the raw layer buffer) — the
/// mixer applies gain at the fold.
///
/// # Errors
///
/// Returns an error if `sample_rate` is 0 (propagated from the mono pass).
pub fn render_stereo_stems_full(
    notes: &[RenderNote],
    bpm: u32,
    sample_rate: u32,
    voices: &[RenderVoice],
    automation: &[RenderAutomation],
    fx: &[RenderFx],
    config: &RenderConfig,
) -> Result<(Vec<StereoStem>, RenderResult), ScorestrataError> {
    let (mono_stems, master) =
        render_stems_full(notes, bpm, sample_rate, voices, automation, fx, config)?;

    let stereo_stems = mono_stems
        .into_iter()
        .map(|stem| {
            let pan = fx
                .iter()
                .find(|f| f.layer_name == stem.layer_name)
                .and_then(|f| f.mix_pan)
                .unwrap_or_else(|| layer_pan(&stem.layer_name));
            let pan_norm = f32::midpoint(pan, 1.0); // -1..1 → 0..1
            let left_gain = (pan_norm * std::f32::consts::FRAC_PI_2).cos();
            let right_gain = (pan_norm * std::f32::consts::FRAC_PI_2).sin();
            let left: Vec<f32> = stem.samples.iter().map(|s| s * left_gain).collect();
            let right: Vec<f32> = stem.samples.iter().map(|s| s * right_gain).collect();
            let wav_bytes = write_wav_stereo(&left, &right, sample_rate, 16);
            let checksum = sha256_hex(&wav_bytes);
            let analysis = crate::analysis::analyze_stereo(&left, &right);
            StereoStem {
                layer_name: stem.layer_name,
                wav_bytes,
                left,
                right,
                duration_seconds: stem.duration_seconds,
                checksum,
                analysis,
            }
        })
        .collect();

    Ok((stereo_stems, master))
}

/// Render one layer's notes into a mono sample buffer.
///
/// Shared by the master render and the stems renderer so both produce
/// identical per-layer audio.
#[expect(
    clippy::too_many_arguments,
    reason = "Render pipeline threads the full note context through one shared helper"
)]
fn render_layer_buffer(
    notes: &[RenderNote],
    layer_name: &str,
    voices: &[RenderVoice],
    bpm: u32,
    sample_rate: u32,
    total_samples: u64,
    automation: &[&RenderAutomation],
    kit: Option<&crate::samplebank::DrumKit>,
    melodic_bank: Option<&crate::multisample::InstrumentBank>,
) -> Vec<f32> {
    let mut buffer = vec![0.0f32; total_samples as usize];
    let voice = voices.iter().find(|v| v.layer_name == layer_name);
    let params = voice.map_or_else(SynthParams::default, |v| v.params.clone());
    let is_percussion = voice.is_some_and(|v| v.is_percussion);
    // (G) Multi-sample voice: the bank instrument this layer maps to, if any.
    // Guitar / bass / e-piano families route to the embedded sample bank
    // when enabled; everything else keeps the subtractive voice (R1).
    let bank_instrument = melodic_bank.and_then(|_| {
        voice
            .as_ref()
            .and_then(|v| melodic_instrument_for(&v.instrument))
    });

    // G4 — resolve this layer's automation lanes once: cutoff (CC 74) and
    // volume (CC 7). Values are interpolated per note at its start tick.
    let cutoff_lane = automation.iter().find(|a| a.controller == 74);
    let volume_lane = automation.iter().find(|a| a.controller == 7);

    for note in notes.iter().filter(|n| n.layer_name == layer_name) {
        let start = tick_to_sample(note.start_tick, bpm, sample_rate) as usize;
        if start >= buffer.len() {
            continue;
        }
        let dur = tick_to_sample(note.duration_ticks, bpm, sample_rate).max(1) as usize;
        let release = (params.release.max(0.0) * sample_rate as f32) as usize;
        let end = (start + dur + release).min(buffer.len());
        let mut note_buffer = vec![0.0f32; end - start];

        // G4 — automation at this note's start tick: cutoff opens/closes the
        // filter, volume scales the rendered body. Both are deterministic
        // linear interpolations of the layer's lanes.
        let mut note_params = params.clone();
        if let Some(lane) = cutoff_lane
            && let Some(value) = lane.value_at(note.start_tick)
        {
            // 0.0 → darkest (0.05), 1.0 → fully open (1.0).
            note_params.filter_cutoff = value.clamp(0.0, 1.0).mul_add(0.95, 0.05);
        }
        let volume = volume_lane
            .and_then(|lane| lane.value_at(note.start_tick))
            .unwrap_or(1.0)
            .clamp(0.0, 1.0);

        if is_percussion {
            if let Some(kind) = drum_kind_for_pitch(note.pitch) {
                if let Some(kit) = kit {
                    // (A) Sampled percussion — the embedded kit's one-shot.
                    crate::samplebank::render_kit_note(kit, kind, note.velocity, &mut note_buffer);
                } else {
                    // Legacy real drum synthesis (kick pitch-sweep, snare/hat
                    // noise) — the bankless path stays byte-identical (R1).
                    render_drum_note(kind, note.velocity, sample_rate as f32, &mut note_buffer);
                }
            } else {
                let freq = midi_to_frequency(note.pitch);
                render_note(
                    &note_params,
                    freq,
                    note.velocity,
                    sample_rate as f32,
                    dur.min(note_buffer.len()),
                    &mut note_buffer,
                );
            }
        } else if let Some(bank) = melodic_bank
            && let Some(instrument) = bank_instrument
        {
            // (G) Multi-sample voice: the embedded bank pitch-shifts its
            // one-shots to this note. The round-robin seed is a deterministic
            // hash of the note (pitch, velocity bits, start tick) so repeated
            // identical hits still cycle (R2).
            let rr_seed =
                u64::from(note.pitch) ^ u64::from(note.velocity.to_bits()) << 32 ^ note.start_tick;
            bank.render_note(
                instrument,
                note.pitch,
                note.velocity,
                rr_seed,
                &mut note_buffer,
            );
            // (G) Amp drive: the bank plays *clean* one-shots; a guitar
            // family's voice carries a drive amount (e.g. `distorted_guitar`
            // = 0.85) that previously lived in the synth voice. Re-apply it
            // to the sampled body so the rock chug survives the switch.
            if note_params.drive > 0.0 {
                let gain = note_params.drive.clamp(0.0, 1.0).mul_add(8.0, 1.0);
                let norm = gain.tanh().max(1e-6);
                for s in &mut note_buffer {
                    *s = (gain * *s).tanh() / norm;
                }
            }
            // (G) Release fade: the sample engine stops writing when the
            // one-shot ends, but a short note buffer would otherwise hard-
            // truncate mid-pluck → a click on every chug. Ramp the tail of
            // the note buffer down over the voice's release time.
            fade_note_tail(&mut note_buffer, params.release.max(0.02), sample_rate);
        } else {
            let freq = midi_to_frequency(note.pitch);
            render_note(
                &note_params,
                freq,
                note.velocity,
                sample_rate as f32,
                dur.min(note_buffer.len()),
                &mut note_buffer,
            );
        }
        if volume < 1.0 {
            for sample in &mut note_buffer {
                *sample *= volume;
            }
        }
        for (i, sample) in note_buffer.iter().enumerate() {
            buffer[start + i] += sample;
        }
    }
    buffer
}

/// Ramp the last `release` seconds of a note buffer to zero (linear fade).
///
/// The multi-sample engine (G) writes the one-shot and stops; without this
/// the buffer's hard end would clip every short note (rock chugs are 16th
/// notes inside a 1.7 s pluck). The synth path already renders its ADSR
/// release, so this only touches sampled notes.
fn fade_note_tail(buffer: &mut [f32], release: f32, sample_rate: u32) {
    let fade_samples = (release * sample_rate as f32) as usize;
    let fade_samples = fade_samples.min(buffer.len());
    if fade_samples == 0 {
        return;
    }
    let start = buffer.len() - fade_samples;
    for (i, sample) in buffer[start..].iter_mut().enumerate() {
        let t = i as f32 / fade_samples as f32;
        *sample *= 1.0 - t;
    }
}

/// Convert a MIDI note number to a frequency in Hz.
#[must_use]
pub fn midi_to_frequency(midi: u8) -> f32 {
    // A4 = 440 Hz = MIDI 69
    440.0 * ((f32::from(midi) - 69.0) / 12.0).exp2()
}

/// Compute the SHA-256 hex digest of bytes.
#[must_use]
pub fn sha256_hex(data: &[u8]) -> String {
    use sha2::{Digest, Sha256};
    hex::encode(Sha256::digest(data))
}

/// Waveform selection from a layer/instrument name (used by render tools).
#[must_use]
pub fn waveform_for_instrument(instrument: &str) -> Waveform {
    match instrument {
        "lead" | "synth" | "pluck" => Waveform::Square,
        "pad" | "strings" | "keys" => Waveform::Triangle,
        "bell" | "drums" | "percussion" => Waveform::Sine,
        // bass, guitars, brass and everything else default to saw.
        _ => Waveform::Saw,
    }
}

/// Map a layer instrument name to its voice family (normalizes suffixes like
/// `bass_synth`, `drum_sampler`, `lead_synth` down to the family).
fn instrument_family(instrument: &str) -> &str {
    let lower = instrument.to_lowercase();
    if lower.contains("drum") || lower.contains("perc") || lower.contains("sampler") {
        "drums"
    } else if lower.contains("bass") || lower.contains("sub") {
        "bass"
    } else if lower.contains("guitar") || lower.contains("riff") {
        "guitar"
    } else if lower.contains("bell") {
        "bell"
    } else if lower.contains("brass") || lower.contains("horn") {
        "brass"
    } else if lower.contains("lead")
        || lower.contains("pluck")
        || lower.contains("arp")
        || lower.contains("vocal")
        || lower.contains("flute")
    {
        "lead"
    } else if lower.contains("pad")
        || lower.contains("string")
        || lower.contains("key")
        || lower.contains("choir")
        || lower.contains("organ")
    {
        "pad"
    } else {
        "synth"
    }
}

/// Whether an instrument renders through the GM drum synthesizer.
#[must_use]
pub fn is_percussion_instrument(instrument: &str) -> bool {
    instrument_family(instrument) == "drums"
}

/// Select the melodic instrument bank by config name (G/H).
///
/// - `"embedded"` → the synthetic Karplus-Strong bank
///   (`embedded_bank`), the Phase G default.
/// - `"sf2"` → the recorded `SoundFont` bank (`sf2_bank`), Phase H — real
///   multi-sampled instruments rendered from `FluidR3_GM`.
/// - any other value → `None` (the subtractive synth voice, R1). Unknown
///   names deliberately do **not** fall back to a sample bank — a typo like
///   `"sft"` must not silently swap the instrument sound.
#[must_use]
fn melodic_bank_for(config: &RenderConfig) -> Option<crate::multisample::InstrumentBank> {
    match config.melodic_bank.as_deref() {
        Some("sf2") => Some(crate::sf2_bank::sf2_instrument_bank()),
        Some("embedded") => Some(crate::embedded_bank::embedded_instrument_bank()),
        Some(_) | None => None,
    }
}

/// Map a layer instrument to a multi-sample bank voice (G).
///
/// Returns the embedded-bank instrument name for the instrument families the
/// sample bank ships (`guitar`, `bass`, `epiano`); other families keep the
/// subtractive synth voice. The mapping is by family substring so suffixed
/// names (`distorted_guitar`, `bass_synth`, `lead_synth`) resolve naturally.
#[must_use]
pub fn melodic_instrument_for(instrument: &str) -> Option<&'static str> {
    match instrument_family(instrument) {
        "guitar" => Some("guitar"),
        "bass" => Some("bass"),
        "bell" => Some("epiano"),
        _ => None,
    }
}

/// Production-grade per-family voice timbres.
///
/// Each family is a **layered** voice now: a detuned second oscillator
/// (the classic "thick" spread), a sine sub-oscillator for weight, a noise
/// blend for air/texture, a drive stage for harmonic richness, a filter
/// envelope for the "wow", and a vibrato for life. The values encode the
/// first-listen learnings: bass is a driven detuned saw with sub, lead is
/// a detuned square with a cutoff sweep and vibrato, pads are slow-attack
/// detuned triangles with air, guitars are heavily driven saws.
#[must_use]
#[expect(
    clippy::too_many_lines,
    reason = "Voice preset table maps ~10 instrument families to full SynthParams; splitting adds indirection"
)]
pub fn voice_params_for_instrument(instrument: &str) -> SynthParams {
    match instrument_family(instrument) {
        "drums" => SynthParams {
            waveform: Waveform::Sine,
            filter_cutoff: 0.6,
            filter_resonance: 0.1,
            attack: 0.002,
            decay: 0.1,
            sustain: 0.2,
            release: 0.05,
            ..Default::default()
        },
        "bass" => SynthParams {
            waveform: Waveform::Saw,
            filter_cutoff: 0.42,
            filter_resonance: 0.3,
            attack: 0.01,
            decay: 0.12,
            sustain: 0.8,
            release: 0.15,
            // Detuned saw pair + a sine sub an octave down: the "fat"
            // bass sound (weight without mud, presence without buzz).
            osc2_mix: 0.55,
            detune_cents: 9.0,
            sub_mix: 0.6,
            drive: 0.28,
            filter_env_amount: 0.35,
            filter_env_decay: 0.09,
            ..Default::default()
        },
        "guitar" => SynthParams {
            waveform: Waveform::Saw,
            filter_cutoff: 0.5,
            filter_resonance: 0.3,
            attack: 0.003,
            decay: 0.1,
            sustain: 0.55,
            release: 0.12,
            // Heavy drive + noise fizz = the distorted-guitar chug;
            // the filter envelope adds the pick "snarl" on each note.
            osc2_mix: 0.45,
            detune_cents: 6.0,
            noise_mix: 0.12,
            drive: 0.85,
            filter_env_amount: 0.45,
            filter_env_decay: 0.06,
            ..Default::default()
        },
        "bell" => SynthParams {
            waveform: Waveform::Sine,
            filter_cutoff: 0.8,
            filter_resonance: 0.15,
            attack: 0.002,
            decay: 0.35,
            sustain: 0.0,
            release: 0.25,
            // Beating detuned sine pair = the shimmer of struck metal.
            osc2_mix: 0.7,
            detune_cents: 5.0,
            ..Default::default()
        },
        "brass" => SynthParams {
            waveform: Waveform::Saw,
            filter_cutoff: 0.5,
            filter_resonance: 0.25,
            attack: 0.03,
            decay: 0.15,
            sustain: 0.7,
            release: 0.2,
            osc2_mix: 0.5,
            detune_cents: 7.0,
            drive: 0.4,
            filter_env_amount: 0.5,
            filter_env_decay: 0.12,
            ..Default::default()
        },
        "lead" => SynthParams {
            waveform: Waveform::Square,
            filter_cutoff: 0.55,
            filter_resonance: 0.25,
            attack: 0.005,
            decay: 0.08,
            sustain: 0.6,
            release: 0.15,
            osc2_mix: 0.6,
            detune_cents: 7.0,
            drive: 0.25,
            filter_env_amount: 0.4,
            filter_env_decay: 0.1,
            vibrato_rate_hz: 5.0,
            vibrato_depth: 4.0,
            ..Default::default()
        },
        "pad" => SynthParams {
            waveform: Waveform::Triangle,
            filter_cutoff: 0.35,
            filter_resonance: 0.2,
            attack: 0.6,
            decay: 0.25,
            sustain: 0.8,
            release: 0.8,
            // Wide detuned pair + a little noise air = the "breathing" pad.
            osc2_mix: 0.75,
            detune_cents: 10.0,
            noise_mix: 0.06,
            drive: 0.12,
            filter_env_amount: 0.3,
            filter_env_decay: 0.6,
            vibrato_rate_hz: 0.4,
            vibrato_depth: 3.0,
            ..Default::default()
        },
        _ => SynthParams {
            waveform: Waveform::Saw,
            filter_cutoff: 0.5,
            filter_resonance: 0.2,
            attack: 0.01,
            decay: 0.1,
            sustain: 0.7,
            release: 0.2,
            osc2_mix: 0.5,
            detune_cents: 8.0,
            drive: 0.2,
            filter_env_amount: 0.35,
            filter_env_decay: 0.12,
            ..Default::default()
        },
    }
}

/// Build the voice table for a set of `(layer name, instrument)` pairs.
///
/// Callers gather the pairs from the arrangement so every layer gets an
/// explicit timbre (previously the render path passed an empty voice table
/// and every layer fell back to the same default saw).
#[must_use]
pub fn build_voices(layers: &[(&str, &str)]) -> Vec<RenderVoice> {
    layers
        .iter()
        .map(|(name, instrument)| RenderVoice {
            layer_name: name.to_string(),
            instrument: instrument.to_string(),
            params: voice_params_for_instrument(instrument),
            is_percussion: is_percussion_instrument(instrument),
        })
        .collect()
}

/// Convert a gain in dB to a linear amplitude multiplier.
#[must_use]
pub fn db_to_linear(db: f32) -> f32 {
    10.0f32.powf(db / 20.0)
}

/// Deterministic per-layer mix gain (gain staging).
///
/// Drums sit forward, bass supports, and melodic layers sit slightly behind
/// so the lead is audible instead of 10 dB under the bass (the first-listen
/// defect).
#[must_use]
pub fn layer_gain(layer_name: &str) -> f32 {
    let lower = layer_name.to_lowercase();
    if lower.contains("drum") || lower.contains("perc") {
        0.9
    } else if lower.contains("bass") || lower.contains("sub") {
        0.7
    } else if lower.contains("lead") || lower.contains("pluck") || lower.contains("arp") {
        0.65
    } else if lower.contains("pad") || lower.contains("string") || lower.contains("key") {
        0.5
    } else {
        0.7
    }
}

/// Deterministic per-layer pan for stereo renders (mono keeps everything
/// centered). Doubled layers split left/right for width.
#[must_use]
pub fn layer_pan(layer_name: &str) -> f32 {
    let lower = layer_name.to_lowercase();
    if lower.contains("octave") {
        -0.35
    } else if lower.contains("lead") || lower.contains("pluck") || lower.contains("arp") {
        0.35
    } else {
        0.0
    }
}

/// The longest release tail across the voices (in samples), so note tails
/// are not truncated at the end of the buffer.
fn release_tail_samples(voices: &[RenderVoice], sample_rate: u32) -> u64 {
    let default_release = SynthParams::default().release;
    let max_release = voices
        .iter()
        .map(|v| v.params.release)
        .fold(default_release, f32::max);
    (max_release * sample_rate as f32) as u64
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn midi_frequency_a4_is_440() {
        assert!((midi_to_frequency(69) - 440.0).abs() < 0.001);
        // A5 is 880
        assert!((midi_to_frequency(81) - 880.0).abs() < 0.001);
        // A3 is 220
        assert!((midi_to_frequency(57) - 220.0).abs() < 0.001);
    }

    #[test]
    fn tick_conversion_is_consistent() {
        // 1 quarter note at 120 BPM = 0.5 s
        let seconds = tick_to_seconds(u64::from(PPQ), 120);
        assert!((seconds - 0.5).abs() < 1e-6);
        let samples = tick_to_sample(u64::from(PPQ), 120, 48_000);
        assert_eq!(samples, 24_000);
    }

    #[test]
    fn render_notes_produces_wav() {
        let notes = vec![
            RenderNote {
                pitch: 60,
                start_tick: 0,
                duration_ticks: u64::from(PPQ),
                velocity: 0.8,
                layer_name: "lead".to_string(),
            },
            RenderNote {
                pitch: 64,
                start_tick: u64::from(PPQ),
                duration_ticks: u64::from(PPQ),
                velocity: 0.7,
                layer_name: "lead".to_string(),
            },
        ];
        #[expect(clippy::unwrap_used, reason = "Test assertions")]
        let result = render_notes(&notes, 120, 48_000, &[]).unwrap();
        assert_eq!(&result.wav_bytes[0..4], b"RIFF");
        assert!(!result.checksum.is_empty());
        assert!(result.duration_seconds > 0.4);
    }

    #[test]
    fn render_is_deterministic() {
        let notes = vec![RenderNote {
            pitch: 60,
            start_tick: 0,
            duration_ticks: u64::from(PPQ),
            velocity: 0.8,
            layer_name: "lead".to_string(),
        }];
        #[expect(clippy::unwrap_used, reason = "Test assertions")]
        let a = render_notes(&notes, 120, 48_000, &[]).unwrap();
        #[expect(clippy::unwrap_used, reason = "Test assertions")]
        let b = render_notes(&notes, 120, 48_000, &[]).unwrap();
        assert_eq!(a.checksum, b.checksum);
        assert_eq!(a.wav_bytes, b.wav_bytes);
    }

    #[test]
    fn render_rejects_zero_sample_rate() {
        let notes = vec![];
        assert!(render_notes(&notes, 120, 0, &[]).is_err());
    }

    #[test]
    fn voice_selection_applies_waveform() {
        assert!(matches!(waveform_for_instrument("bass"), Waveform::Saw));
        assert!(matches!(waveform_for_instrument("lead"), Waveform::Square));
        assert!(matches!(waveform_for_instrument("pad"), Waveform::Triangle));
    }

    #[test]
    fn volume_automation_silences_layer() {
        let notes = vec![RenderNote {
            pitch: 60,
            start_tick: 0,
            duration_ticks: u64::from(PPQ),
            velocity: 0.8,
            layer_name: "lead".to_string(),
        }];
        let config = RenderConfig::default();
        // Volume lane pinned to 0 → the layer must render silent.
        let muted = vec![RenderAutomation {
            layer_name: "lead".to_string(),
            controller: 7,
            points: vec![(0, 0.0), (u64::from(PPQ), 0.0)],
        }];
        #[expect(clippy::unwrap_used, reason = "Test assertions")]
        let full = render_notes_with_automation(&notes, 120, 48_000, &[], &[], &config).unwrap();
        #[expect(clippy::unwrap_used, reason = "Test assertions")]
        let silent =
            render_notes_with_automation(&notes, 120, 48_000, &[], &muted, &config).unwrap();
        let full_rms = rms(&full.mono_samples);
        let silent_rms = rms(&silent.mono_samples);
        assert!(full_rms > 0.01, "unmuted layer is audible: {full_rms}");
        assert!(
            silent_rms < 0.001,
            "volume-0 lane silences the layer: {silent_rms}"
        );
    }

    #[test]
    fn cutoff_automation_changes_spectral_content() {
        let notes = vec![RenderNote {
            pitch: 60,
            start_tick: 0,
            duration_ticks: u64::from(PPQ),
            velocity: 0.8,
            layer_name: "lead".to_string(),
        }];
        let config = RenderConfig::default();
        let dark = vec![RenderAutomation {
            layer_name: "lead".to_string(),
            controller: 74,
            points: vec![(0, 0.0), (u64::from(PPQ), 0.0)],
        }];
        let open = vec![RenderAutomation {
            layer_name: "lead".to_string(),
            controller: 74,
            points: vec![(0, 1.0), (u64::from(PPQ), 1.0)],
        }];
        #[expect(clippy::unwrap_used, reason = "Test assertions")]
        let dark_r =
            render_notes_with_automation(&notes, 120, 48_000, &[], &dark, &config).unwrap();
        #[expect(clippy::unwrap_used, reason = "Test assertions")]
        let open_r =
            render_notes_with_automation(&notes, 120, 48_000, &[], &open, &config).unwrap();
        // Same pitch/velocity, different cutoff → different spectral energy
        // (RMS of an unfiltered saw is larger than a heavily filtered one).
        let dark_rms = rms(&dark_r.mono_samples);
        let open_rms = rms(&open_r.mono_samples);
        assert!(
            (dark_rms - open_rms).abs() > 0.005,
            "cutoff sweep changes output (dark {dark_rms} vs open {open_rms})"
        );
    }

    fn rms(samples: &[f32]) -> f32 {
        if samples.is_empty() {
            return 0.0;
        }
        let sum: f32 = samples.iter().map(|s| s * s).sum();
        (sum / samples.len() as f32).sqrt()
    }

    #[test]
    fn per_track_fx_alters_spectrum_and_mix() {
        let notes = vec![RenderNote {
            pitch: 60,
            start_tick: 0,
            duration_ticks: u64::from(PPQ) * 2,
            velocity: 0.8,
            layer_name: "lead".to_string(),
        }];
        let config = RenderConfig::default();
        // Clean render: static staging gain.
        #[expect(clippy::unwrap_used, reason = "Test assertions")]
        let clean = render_notes_full(&notes, 120, 48_000, &[], &[], &[], &config).unwrap();
        // FX render: 200 Hz HPF + 6 dB high shelf-ish boost + reverb send.
        let fx = vec![RenderFx {
            layer_name: "lead".to_string(),
            fx: scorestrata_core::domain::FxChain {
                high_pass_hz: Some(200.0),
                eq: vec![scorestrata_core::domain::FreqBand {
                    freq_hz: 2000.0,
                    gain_db: 6.0,
                    q: 1.0,
                }],
                compressor: None,
                sidechain_from_kick: false,
                sends: scorestrata_core::domain::SendLevels {
                    reverb: 0.3,
                    delay: 0.0,
                },
                sidechain_amount: None,
            },
            mix_gain_db: Some(-6.0),
            mix_pan: None,
            group: None,
        }];
        #[expect(clippy::unwrap_used, reason = "Test assertions")]
        let fx_r = render_notes_full(&notes, 120, 48_000, &[], &[], &fx, &config).unwrap();
        // The mix override (-6 dB) changes the level provably.
        let clean_rms = rms(&clean.mono_samples);
        let fx_rms = rms(&fx_r.mono_samples);
        assert!(
            fx_rms < clean_rms * 0.8,
            "−6 dB mix override reduces level: {fx_rms} vs {clean_rms}"
        );
        // And the FX changes the output bytes (HPF + EQ + reverb tail).
        assert_ne!(clean.checksum, fx_r.checksum, "FX alters the render");
    }

    #[test]
    fn melodic_bank_routes_guitar_and_bass_to_samples() {
        // A guitar power-chord dyad + bass note. With the bank enabled the
        // layer must sound (peaks present) and differ from the synth path;
        // with the bank disabled it must stay byte-identical to legacy (R1).
        let notes = vec![
            RenderNote {
                pitch: 45,
                start_tick: 0,
                duration_ticks: u64::from(PPQ) * 2,
                velocity: 0.8,
                layer_name: "Guitar".to_string(),
            },
            RenderNote {
                pitch: 52,
                start_tick: 0,
                duration_ticks: u64::from(PPQ) * 2,
                velocity: 0.8,
                layer_name: "Guitar".to_string(),
            },
            RenderNote {
                pitch: 33,
                start_tick: 0,
                duration_ticks: u64::from(PPQ) * 2,
                velocity: 0.9,
                layer_name: "Bass".to_string(),
            },
        ];
        let voices = build_voices(&[("Guitar", "distorted_guitar"), ("Bass", "bass_synth")]);

        let plain = RenderConfig::default();
        let with_bank = RenderConfig {
            melodic_bank: Some("embedded".to_string()),
            ..RenderConfig::default()
        };

        #[expect(clippy::unwrap_used, reason = "Test assertions")]
        let synth = render_notes_full(&notes, 120, 48_000, &voices, &[], &[], &plain).unwrap();
        #[expect(clippy::unwrap_used, reason = "Test assertions")]
        let sampled =
            render_notes_full(&notes, 120, 48_000, &voices, &[], &[], &with_bank).unwrap();

        // Both paths are audible.
        assert!(rms(&synth.mono_samples) > 0.01, "synth path audible");
        assert!(rms(&sampled.mono_samples) > 0.01, "sampled path audible");
        // The bank changes the tone (plucked string vs driven saw).
        assert_ne!(
            synth.checksum, sampled.checksum,
            "melodic bank must change guitar/bass output"
        );

        // R1: repeat the synth render — byte-identical to the first.
        #[expect(clippy::unwrap_used, reason = "Test assertions")]
        let synth_again =
            render_notes_full(&notes, 120, 48_000, &voices, &[], &[], &plain).unwrap();
        assert_eq!(
            synth.checksum, synth_again.checksum,
            "bankless render stays byte-identical (R1)"
        );
        // And the banked render is deterministic (R2).
        #[expect(clippy::unwrap_used, reason = "Test assertions")]
        let sampled_again =
            render_notes_full(&notes, 120, 48_000, &voices, &[], &[], &with_bank).unwrap();
        assert_eq!(
            sampled.checksum, sampled_again.checksum,
            "banked render is deterministic (R2)"
        );
    }

    #[test]
    fn unknown_melodic_bank_name_falls_back_to_synth_not_samples() {
        // A typo like "sft" must NOT silently swap to a sample bank — it
        // falls back to the subtractive synth voice (R1), so the render is
        // byte-identical to the no-bank render.
        let notes = vec![RenderNote {
            pitch: 57,
            start_tick: 0,
            duration_ticks: u64::from(PPQ) * 2,
            velocity: 0.8,
            layer_name: "Guitar".to_string(),
        }];
        let voices = build_voices(&[("Guitar", "distorted_guitar")]);
        let plain = RenderConfig::default();
        let typo = RenderConfig {
            melodic_bank: Some("sft".to_string()),
            ..RenderConfig::default()
        };
        #[expect(clippy::unwrap_used, reason = "Test assertions")]
        let synth = render_notes_full(&notes, 120, 48_000, &voices, &[], &[], &plain).unwrap();
        #[expect(clippy::unwrap_used, reason = "Test assertions")]
        let unknown = render_notes_full(&notes, 120, 48_000, &voices, &[], &[], &typo).unwrap();
        assert_eq!(
            synth.checksum, unknown.checksum,
            "unknown bank name falls back to the synth voice, not a sample bank"
        );
    }

    #[test]
    fn sf2_melodic_bank_renders_and_differs_from_embedded() {
        // Phase H: melodic_bank "sf2" uses the recorded SoundFont bank; it
        // must render audibly and produce different bytes than "embedded"
        // (recorded vs synthetic source), and stay deterministic (R2).
        let notes = vec![RenderNote {
            pitch: 57,
            start_tick: 0,
            duration_ticks: u64::from(PPQ) * 2,
            velocity: 0.8,
            layer_name: "Guitar".to_string(),
        }];
        let voices = build_voices(&[("Guitar", "distorted_guitar")]);
        let embedded = RenderConfig {
            melodic_bank: Some("embedded".to_string()),
            ..RenderConfig::default()
        };
        let sf2 = RenderConfig {
            melodic_bank: Some("sf2".to_string()),
            ..RenderConfig::default()
        };
        #[expect(clippy::unwrap_used, reason = "Test assertions")]
        let embedded_r =
            render_notes_full(&notes, 120, 48_000, &voices, &[], &[], &embedded).unwrap();
        #[expect(clippy::unwrap_used, reason = "Test assertions")]
        let sf2_r = render_notes_full(&notes, 120, 48_000, &voices, &[], &[], &sf2).unwrap();
        assert!(rms(&sf2_r.mono_samples) > 0.01, "SF2 bank path audible");
        assert_ne!(
            embedded_r.checksum, sf2_r.checksum,
            "recorded SF2 bank differs from the synthetic bank"
        );
        #[expect(clippy::unwrap_used, reason = "Test assertions")]
        let sf2_again = render_notes_full(&notes, 120, 48_000, &voices, &[], &[], &sf2).unwrap();
        assert_eq!(
            sf2_r.checksum, sf2_again.checksum,
            "SF2 render is deterministic (R2)"
        );
    }

    #[test]
    fn master_gain_db_scales_the_master_fold() {
        let notes = vec![RenderNote {
            pitch: 60,
            start_tick: 0,
            duration_ticks: u64::from(PPQ),
            velocity: 0.8,
            layer_name: "lead".to_string(),
        }];
        let unity = RenderConfig::default();
        let boosted = RenderConfig {
            master_gain_db: 12.0,
            ..Default::default()
        };
        #[expect(clippy::unwrap_used, reason = "Test assertions")]
        let a = render_notes_full(&notes, 120, 48_000, &[], &[], &[], &unity).unwrap();
        #[expect(clippy::unwrap_used, reason = "Test assertions")]
        let b = render_notes_full(&notes, 120, 48_000, &[], &[], &[], &boosted).unwrap();
        // +12 dB master gain ≈ 4× linear, so the mastered fold rises clearly.
        assert!(
            rms(&b.mono_samples) > rms(&a.mono_samples) * 2.0,
            "master gain raises the fold ({:.4} vs {:.4})",
            rms(&b.mono_samples),
            rms(&a.mono_samples)
        );
        // The limiter still protects the boosted render (ceiling −1 dBTP).
        assert!(
            !b.analysis.clipping_detected && b.analysis.true_peak_dbtp <= -0.4,
            "boosted master stays within the limiter ceiling ({:.2} dBTP)",
            b.analysis.true_peak_dbtp
        );
    }

    #[test]
    fn stereo_width_alters_channels_but_not_mono_fold() {
        // A centered note plus a hard-panned note: width 0.0 collapses the
        // side (channels equal), width 2.0 widens it (channels diverge) —
        // while the mono fold (L+R)/2 stays identical (mid/side invariant).
        let notes = vec![
            RenderNote {
                pitch: 60,
                start_tick: 0,
                duration_ticks: u64::from(PPQ),
                velocity: 0.8,
                layer_name: "Lead".to_string(),
            },
            RenderNote {
                pitch: 72,
                start_tick: 0,
                duration_ticks: u64::from(PPQ),
                velocity: 0.8,
                layer_name: "Lead Octave".to_string(),
            },
        ];
        let voices = build_voices(&[("Lead", "lead_synth"), ("Lead Octave", "lead_synth")]);
        // No limiter (it is per-channel and nonlinear, which would break the
        // mid/side mono invariance under test); −6 dB master gain keeps the
        // un-clipped two-layer sum inside the 16-bit range.
        let narrow = RenderConfig {
            stereo: true,
            limiter: None,
            master_gain_db: -6.0,
            stereo_width: 0.0,
            sound_bank: None,
            bus_graph: None,
            master_chain: None,
            bits_per_sample: 16,
            sample_rate: DEFAULT_SAMPLE_RATE,
            melodic_bank: None,
        };
        let wide = RenderConfig {
            stereo: true,
            limiter: None,
            master_gain_db: -6.0,
            stereo_width: 2.0,
            sound_bank: None,
            bus_graph: None,
            master_chain: None,
            bits_per_sample: 16,
            sample_rate: DEFAULT_SAMPLE_RATE,
            melodic_bank: None,
        };
        #[expect(clippy::unwrap_used, reason = "Test assertions")]
        let a = render_notes_full(&notes, 120, 48_000, &voices, &[], &[], &narrow).unwrap();
        #[expect(clippy::unwrap_used, reason = "Test assertions")]
        let b = render_notes_full(&notes, 120, 48_000, &voices, &[], &[], &wide).unwrap();
        let channel_split = |result: &RenderResult| -> f64 {
            let data = &result.wav_bytes[44..];
            let frames = data.len() / 4;
            let mut left_energy = 0.0f64;
            let mut right_energy = 0.0f64;
            for i in 0..frames {
                let l = f64::from(i16::from_le_bytes([data[i * 4], data[i * 4 + 1]])) / 32768.0;
                let r = f64::from(i16::from_le_bytes([data[i * 4 + 2], data[i * 4 + 3]])) / 32768.0;
                left_energy += l * l;
                right_energy += r * r;
            }
            (left_energy - right_energy).abs()
        };
        // Width 0 collapses to identical channels; width 2 diverges them.
        assert!(
            channel_split(&a) < channel_split(&b) * 0.5,
            "width changes channel split (narrow {:.4} vs wide {:.4})",
            channel_split(&a),
            channel_split(&b)
        );
        // The mono fold is invariant under the mid/side transform.
        assert!(
            rms(&a.mono_samples) > 0.0
                && (rms(&a.mono_samples) - rms(&b.mono_samples)).abs() < 1e-4,
            "mono fold unchanged by width ({:.5} vs {:.5})",
            rms(&a.mono_samples),
            rms(&b.mono_samples)
        );
    }

    #[test]
    fn empty_notes_renders_empty_wav() {
        #[expect(clippy::unwrap_used, reason = "Test assertions")]
        let result = render_notes(&[], 120, 48_000, &[]).unwrap();
        assert_eq!(result.duration_seconds, 0.0);
        assert!(!result.wav_bytes.is_empty()); // RIFF header still present
    }

    #[test]
    fn render_result_carries_analysis() {
        let notes = vec![RenderNote {
            pitch: 60,
            start_tick: 0,
            duration_ticks: u64::from(PPQ),
            velocity: 0.8,
            layer_name: "lead".to_string(),
        }];
        #[expect(clippy::unwrap_used, reason = "Test assertions")]
        let result = render_notes(&notes, 120, 48_000, &[]).unwrap();
        assert!(result.analysis.integrated_lufs > -70.0);
        assert!(!result.mono_samples.is_empty());
        assert!(!result.analysis.clipping_detected);
    }

    #[test]
    fn render_stems_produces_one_stem_per_layer() {
        let notes = vec![
            RenderNote {
                pitch: 36,
                start_tick: 0,
                duration_ticks: u64::from(PPQ),
                velocity: 0.9,
                layer_name: "Drums".to_string(),
            },
            RenderNote {
                pitch: 48,
                start_tick: 0,
                duration_ticks: u64::from(PPQ),
                velocity: 0.8,
                layer_name: "Bass".to_string(),
            },
        ];
        #[expect(clippy::unwrap_used, reason = "Test assertions")]
        let (stems, master) = render_stems(&notes, 120, 48_000, &[]).unwrap();
        assert_eq!(stems.len(), 2);
        assert_eq!(stems[0].layer_name, "Drums");
        assert_eq!(stems[1].layer_name, "Bass");
        assert_eq!(&stems[0].wav_bytes[0..4], b"RIFF");
        assert!(!stems[0].checksum.is_empty());
        assert_eq!(
            master.analysis.integrated_lufs.to_bits(),
            master.analysis.integrated_lufs.to_bits()
        );
    }

    #[test]
    fn render_stems_is_deterministic() {
        let notes = vec![RenderNote {
            pitch: 60,
            start_tick: 0,
            duration_ticks: u64::from(PPQ),
            velocity: 0.8,
            layer_name: "lead".to_string(),
        }];
        #[expect(clippy::unwrap_used, reason = "Test assertions")]
        let (a, _) = render_stems(&notes, 120, 48_000, &[]).unwrap();
        #[expect(clippy::unwrap_used, reason = "Test assertions")]
        let (b, _) = render_stems(&notes, 120, 48_000, &[]).unwrap();
        assert_eq!(a[0].checksum, b[0].checksum);
        assert_eq!(a[0].wav_bytes, b[0].wav_bytes);
    }

    #[test]
    fn render_stems_rejects_zero_sample_rate() {
        assert!(render_stems(&[], 120, 0, &[]).is_err());
    }

    #[test]
    fn stereo_render_produces_two_channel_wav() {
        let notes = vec![RenderNote {
            pitch: 60,
            start_tick: 0,
            duration_ticks: u64::from(PPQ),
            velocity: 0.8,
            layer_name: "lead".to_string(),
        }];
        let config = RenderConfig {
            stereo: true,
            limiter: Some(LimiterConfig::default()),
            ..Default::default()
        };
        #[expect(clippy::unwrap_used, reason = "Test assertions")]
        let result = render_notes_with_config(&notes, 120, 48_000, &[], &config).unwrap();
        assert_eq!(result.channels, 2);
        // WAV fmt chunk channel field is 2 for stereo.
        assert_eq!(
            u16::from_le_bytes([result.wav_bytes[22], result.wav_bytes[23]]),
            2
        );
        // Data size covers twice the sample count (L+R interleaved).
        let data_size = u32::from_le_bytes([
            result.wav_bytes[40],
            result.wav_bytes[41],
            result.wav_bytes[42],
            result.wav_bytes[43],
        ]);
        assert_eq!(data_size % 4, 0, "stereo frames are 4 bytes each");
    }

    #[test]
    fn stereo_pan_splits_doubled_lead() {
        // A centered layer and a panned layer must differ across channels.
        let notes = vec![
            RenderNote {
                pitch: 60,
                start_tick: 0,
                duration_ticks: u64::from(PPQ),
                velocity: 0.8,
                layer_name: "Lead".to_string(),
            },
            RenderNote {
                pitch: 72,
                start_tick: 0,
                duration_ticks: u64::from(PPQ),
                velocity: 0.8,
                layer_name: "Lead Octave".to_string(),
            },
        ];
        let voices = build_voices(&[("Lead", "lead_synth"), ("Lead Octave", "lead_synth")]);
        let config = RenderConfig {
            stereo: true,
            limiter: Some(LimiterConfig::default()),
            ..Default::default()
        };
        #[expect(clippy::unwrap_used, reason = "Test assertions")]
        let result = render_notes_with_config(&notes, 120, 48_000, &voices, &config).unwrap();
        assert_eq!(result.channels, 2);
        // Decode the interleaved i16 frames from the WAV data chunk (offset 44).
        let data = &result.wav_bytes[44..];
        let frames = data.len() / 4;
        let mut left_energy = 0.0f64;
        let mut right_energy = 0.0f64;
        for i in 0..frames {
            let l = f64::from(i16::from_le_bytes([data[i * 4], data[i * 4 + 1]])) / 32768.0;
            let r = f64::from(i16::from_le_bytes([data[i * 4 + 2], data[i * 4 + 3]])) / 32768.0;
            left_energy += l * l;
            right_energy += r * r;
        }
        // Panned layers (lead +0.35, octave −0.35) make the channels differ
        // while both carry real signal.
        assert!(
            left_energy > 0.0 && right_energy > 0.0,
            "both channels live"
        );
        let ratio = (left_energy / right_energy).abs();
        assert!(
            (ratio - 1.0).abs() > 0.01,
            "panning splits the channels (L/R energy ratio {ratio:.3})"
        );
    }

    #[test]
    fn voice_wiring_gives_distinct_timbres() {
        let bass = build_voices(&[("Bass", "bass_synth")]);
        let lead = build_voices(&[("Lead", "lead_synth")]);
        let drums = build_voices(&[("Drums", "drum_sampler")]);
        assert!(matches!(bass[0].params.waveform, Waveform::Saw));
        assert!(matches!(lead[0].params.waveform, Waveform::Square));
        assert!(!bass[0].is_percussion);
        assert!(
            drums[0].is_percussion,
            "drum_sampler routes through drum synth"
        );
    }

    #[test]
    fn mastered_render_never_clips_even_when_mix_overs() {
        // Three loud simultaneous layers would overs the old 0.8-gain mix;
        // the limiter must keep the master under full scale.
        let notes = vec![
            RenderNote {
                pitch: 36,
                start_tick: 0,
                duration_ticks: u64::from(PPQ),
                velocity: 1.0,
                layer_name: "Drums".to_string(),
            },
            RenderNote {
                pitch: 38,
                start_tick: 0,
                duration_ticks: u64::from(PPQ),
                velocity: 1.0,
                layer_name: "Drums".to_string(),
            },
            RenderNote {
                pitch: 40,
                start_tick: 0,
                duration_ticks: u64::from(PPQ),
                velocity: 1.0,
                layer_name: "Bass".to_string(),
            },
        ];
        let voices = build_voices(&[("Drums", "drum_sampler"), ("Bass", "bass_synth")]);
        #[expect(clippy::unwrap_used, reason = "Test assertions")]
        let result = render_notes(&notes, 120, 48_000, &voices).unwrap();
        assert!(
            !result.analysis.clipping_detected,
            "limiter prevents clipping"
        );
        assert!(
            result.analysis.true_peak_dbtp <= -0.4,
            "true peak {:.2} dBTP within ceiling",
            result.analysis.true_peak_dbtp
        );
        assert!(
            result.analysis.true_peak_dbtp > -3.0,
            "render is not silence ({:.2} dBTP)",
            result.analysis.true_peak_dbtp
        );
    }

    #[test]
    fn lead_stem_is_within_6db_of_bass() {
        // The first-listen defect: the lead was 10 dB under the bass and
        // inaudible. With gain staging + timbre the gap must be small.
        let notes = vec![
            RenderNote {
                pitch: 36,
                start_tick: 0,
                duration_ticks: u64::from(PPQ),
                velocity: 0.8,
                layer_name: "Bass".to_string(),
            },
            RenderNote {
                pitch: 72,
                start_tick: 0,
                duration_ticks: u64::from(PPQ),
                velocity: 0.8,
                layer_name: "Lead".to_string(),
            },
        ];
        let voices = build_voices(&[("Bass", "bass_synth"), ("Lead", "lead_synth")]);
        #[expect(clippy::unwrap_used, reason = "Test assertions")]
        let (stems, _) = render_stems(&notes, 120, 48_000, &voices).unwrap();
        #[expect(clippy::expect_used, reason = "Test assertions")]
        let bass = stems
            .iter()
            .find(|s| s.layer_name == "Bass")
            .expect("bass stem");
        #[expect(clippy::expect_used, reason = "Test assertions")]
        let lead = stems
            .iter()
            .find(|s| s.layer_name == "Lead")
            .expect("lead stem");
        let gap = (bass.analysis.integrated_lufs - lead.analysis.integrated_lufs).abs();
        assert!(
            gap < 6.0,
            "lead must be audible relative to bass (gap {gap:.1} dB)"
        );
    }

    #[test]
    fn stems_are_peak_limited_never_clip() {
        // Three loud overlapping layers on one layer can sum past full scale;
        // each exported stem must be limited so no stem hard-clips (the master
        // was protected but raw stems were not — first-listen defect).
        let notes = vec![
            RenderNote {
                pitch: 36,
                start_tick: 0,
                duration_ticks: u64::from(PPQ),
                velocity: 1.0,
                layer_name: "Bass".to_string(),
            },
            RenderNote {
                pitch: 43,
                start_tick: u64::from(PPQ) / 2,
                duration_ticks: u64::from(PPQ),
                velocity: 1.0,
                layer_name: "Bass".to_string(),
            },
            RenderNote {
                pitch: 50,
                start_tick: u64::from(PPQ),
                duration_ticks: u64::from(PPQ),
                velocity: 1.0,
                layer_name: "Bass".to_string(),
            },
        ];
        let voices = build_voices(&[("Bass", "bass_synth")]);
        #[expect(clippy::unwrap_used, reason = "Test assertions")]
        let (stems, _) = render_stems(&notes, 120, 48_000, &voices).unwrap();
        assert_eq!(stems.len(), 1);
        assert!(
            !stems[0].analysis.clipping_detected,
            "stem must not clip: {} clipped samples",
            stems[0].analysis.clipped_sample_count
        );
        assert!(
            stems[0].analysis.true_peak_dbtp <= -0.4,
            "stem true peak {:.2} dBTP within ceiling",
            stems[0].analysis.true_peak_dbtp
        );
    }

    #[test]
    fn drum_layer_uses_percussion_synthesis() {
        // A GM kick (36) on a drum layer must produce a pitch-swept body, not
        // a sustained saw: compare the first 50 ms against the same pitch on a
        // non-percussion layer — the drum decays far faster.
        let drums = build_voices(&[("Drums", "drum_sampler")]);
        let synthy = build_voices(&[("Drums", "synth")]);
        let note = RenderNote {
            pitch: 36,
            start_tick: 0,
            duration_ticks: u64::from(PPQ),
            velocity: 1.0,
            layer_name: "Drums".to_string(),
        };
        #[expect(clippy::unwrap_used, reason = "Test assertions")]
        let (drum_stems, _) =
            render_stems(std::slice::from_ref(&note), 120, 48_000, &drums).unwrap();
        #[expect(clippy::unwrap_used, reason = "Test assertions")]
        let (tone_stems, _) = render_stems(&[note], 120, 48_000, &synthy).unwrap();
        // Compare late in the note (250-270 ms): the kick body has decayed
        // (0.22 s envelope) while the sustained synth tone is still at its
        // sustain level.
        let drum_tail = drum_stems[0].samples[12_000..13_000]
            .iter()
            .map(|s| s * s)
            .sum::<f32>()
            / 1_000.0;
        let tone_tail = tone_stems[0].samples[12_000..13_000]
            .iter()
            .map(|s| s * s)
            .sum::<f32>()
            / 1_000.0;
        assert!(
            drum_tail < tone_tail * 0.5,
            "drum decays faster than a sustained tone (drum {drum_tail:.4} vs tone {tone_tail:.4})"
        );
    }

    #[test]
    fn bus_graph_disabled_is_byte_identical_to_flat_fold() {
        // R1: enabling the bus-graph *field* but leaving it empty (no groups,
        // no reverb IR) must not change the render at all.
        let notes = vec![RenderNote {
            pitch: 60,
            start_tick: 0,
            duration_ticks: u64::from(PPQ),
            velocity: 0.8,
            layer_name: "lead".to_string(),
        }];
        let flat = RenderConfig::default();
        let with_graph = RenderConfig {
            bus_graph: Some(scorestrata_core::domain::BusGraph::default()),
            ..RenderConfig::default()
        };
        #[expect(clippy::unwrap_used, reason = "Test assertions")]
        let a = render_notes_full(&notes, 120, 48_000, &[], &[], &[], &flat).unwrap();
        #[expect(clippy::unwrap_used, reason = "Test assertions")]
        let b = render_notes_full(&notes, 120, 48_000, &[], &[], &[], &with_graph).unwrap();
        assert_eq!(
            a.wav_bytes, b.wav_bytes,
            "empty bus graph is byte-identical"
        );
    }

    #[test]
    fn master_chain_disabled_is_byte_identical_to_legacy_path() {
        // R1: a master chain present-but-disabled must not change the render.
        let notes = vec![RenderNote {
            pitch: 60,
            start_tick: 0,
            duration_ticks: u64::from(PPQ),
            velocity: 0.8,
            layer_name: "lead".to_string(),
        }];
        let plain = RenderConfig {
            stereo: true,
            ..RenderConfig::default()
        };
        let mut chain = scorestrata_core::domain::MasterChain {
            enabled: false,
            multiband: scorestrata_core::domain::MultibandCfg::default(),
            saturation: None,
            dither: None,
            target_lufs: None,
        };
        chain.dither = Some(scorestrata_core::domain::DitherCfg {
            bits: 16,
            enabled: false,
        });
        let with_chain = RenderConfig {
            stereo: true,
            master_chain: Some(chain),
            ..RenderConfig::default()
        };
        #[expect(clippy::unwrap_used, reason = "Test assertions")]
        let a = render_notes_full(&notes, 120, 48_000, &[], &[], &[], &plain).unwrap();
        #[expect(clippy::unwrap_used, reason = "Test assertions")]
        let b = render_notes_full(&notes, 120, 48_000, &[], &[], &[], &with_chain).unwrap();
        assert_eq!(
            a.wav_bytes, b.wav_bytes,
            "disabled master chain is byte-identical (R1)"
        );
    }

    #[test]
    fn master_chain_enabled_changes_the_render_and_respects_bit_depth() {
        // The chain actually processes: louder input, multiband + saturation
        // + limiter → output differs from the plain path, and a 24-bit dither
        // yields a 24-bit WAV.
        let notes = vec![RenderNote {
            pitch: 60,
            start_tick: 0,
            duration_ticks: u64::from(PPQ),
            velocity: 1.0,
            layer_name: "lead".to_string(),
        }];
        let plain = RenderConfig {
            stereo: true,
            ..RenderConfig::default()
        };
        let with_chain = RenderConfig {
            stereo: true,
            master_chain: Some(scorestrata_core::domain::MasterChain {
                enabled: true,
                multiband: scorestrata_core::domain::MultibandCfg::default(),
                saturation: Some(scorestrata_core::domain::SaturationCfg {
                    drive_db: 6.0,
                    softness: 0.4,
                    trim_db: -3.0,
                    oversample: 2,
                }),
                dither: Some(scorestrata_core::domain::DitherCfg {
                    bits: 24,
                    enabled: true,
                }),
                target_lufs: None,
            }),
            ..RenderConfig::default()
        };
        #[expect(clippy::unwrap_used, reason = "Test assertions")]
        let a = render_notes_full(&notes, 120, 48_000, &[], &[], &[], &plain).unwrap();
        #[expect(clippy::unwrap_used, reason = "Test assertions")]
        let b = render_notes_full(&notes, 120, 48_000, &[], &[], &[], &with_chain).unwrap();
        assert_ne!(
            a.wav_bytes, b.wav_bytes,
            "an enabled chain must alter the audio"
        );
        // 24-bit header: bits-per-sample at offset 34.
        let bits = u16::from_le_bytes([b.wav_bytes[34], b.wav_bytes[35]]);
        assert_eq!(bits, 24, "24-bit dither chain must export 24-bit PCM");
        // Mastered render must not exceed the limiter ceiling.
        let analysis = &b.analysis;
        assert!(
            analysis.true_peak_dbtp <= -0.9,
            "mastered true peak must respect the −1 dBTP ceiling, got {}",
            analysis.true_peak_dbtp
        );
    }

    #[test]
    fn bus_graph_reverb_extends_render_audibly() {
        // A layer with a reverb send through the convolution return must
        // produce a longer tail than the dry-only flat fold.
        let notes = vec![RenderNote {
            pitch: 64,
            start_tick: 0,
            duration_ticks: u64::from(PPQ),
            velocity: 0.7,
            layer_name: "pad".to_string(),
        }];
        let fx = vec![RenderFx {
            layer_name: "pad".to_string(),
            fx: scorestrata_core::domain::FxChain {
                sends: scorestrata_core::domain::SendLevels {
                    reverb: 0.6,
                    delay: 0.0,
                },
                ..scorestrata_core::domain::FxChain::default()
            },
            mix_gain_db: None,
            mix_pan: None,
            group: None,
        }];
        let flat = RenderConfig::default();
        let graph = scorestrata_core::domain::BusGraph {
            reverb_ir: Some(scorestrata_core::domain::ReverbIr::Hall),
            reverb_pre_delay_ms: Some(30.0),
            ..scorestrata_core::domain::BusGraph::default()
        };
        let with_graph = RenderConfig {
            bus_graph: Some(graph),
            ..RenderConfig::default()
        };
        #[expect(clippy::unwrap_used, reason = "Test assertions")]
        let a = render_notes_full(&notes, 120, 48_000, &[], &[], &fx, &flat).unwrap();
        #[expect(clippy::unwrap_used, reason = "Test assertions")]
        let b = render_notes_full(&notes, 120, 48_000, &[], &[], &fx, &with_graph).unwrap();
        let dry_len = a.mono_samples.len();
        let wet_len = b.mono_samples.len();
        assert!(
            wet_len > dry_len + 10_000,
            "convolution reverb extends the render (dry {dry_len} vs wet {wet_len})"
        );
        // The tail after the dry end must actually carry energy.
        let tail_energy: f32 = b.mono_samples[dry_len..].iter().map(|s| s * s).sum::<f32>()
            / (wet_len - dry_len).max(1) as f32;
        assert!(tail_energy > 1e-7, "wet tail carries audible energy");
    }

    #[test]
    fn stereo_stems_should_pan_layers_into_lr_and_match_master_placement() {
        let notes = vec![
            RenderNote {
                pitch: 60,
                start_tick: 0,
                duration_ticks: 240,
                velocity: 0.9,
                layer_name: "lead".to_string(),
            },
            RenderNote {
                pitch: 36,
                start_tick: 0,
                duration_ticks: 240,
                velocity: 0.9,
                layer_name: "bass".to_string(),
            },
        ];
        let voices = build_voices(&[("lead", "lead"), ("bass", "bass")]);
        // lead defaults to pan +0.35 (right-heavy), bass to center.
        #[expect(clippy::unwrap_used, reason = "Test assertions")]
        let (stems, _master) = render_stereo_stems_full(
            &notes,
            120,
            48_000,
            &voices,
            &[],
            &[],
            &RenderConfig::default(),
        )
        .unwrap();
        assert_eq!(stems.len(), 2, "one stereo stem per layer");
        #[expect(clippy::unwrap_used, reason = "Test assertions")]
        let lead = stems.iter().find(|s| s.layer_name == "lead").unwrap();
        #[expect(clippy::unwrap_used, reason = "Test assertions")]
        let bass = stems.iter().find(|s| s.layer_name == "bass").unwrap();
        assert_eq!(lead.left.len(), lead.right.len());
        // Right-heavy layer: right energy exceeds left energy.
        let l_energy: f32 = lead.left.iter().map(|s| s * s).sum();
        let r_energy: f32 = lead.right.iter().map(|s| s * s).sum();
        assert!(
            r_energy > l_energy,
            "lead pans right (L {l_energy} < R {r_energy})"
        );
        // Center layer: L ≈ R.
        let b_l: f32 = bass.left.iter().map(|s| s * s).sum();
        let b_r: f32 = bass.right.iter().map(|s| s * s).sum();
        assert!(
            (b_l - b_r).abs() < 1e-4 * b_l.max(b_r).max(1.0),
            "bass stays centered (L {b_l} vs R {b_r})"
        );
        // Stereo WAV header says 2 channels.
        let channels = u16::from_le_bytes([lead.wav_bytes[22], lead.wav_bytes[23]]);
        assert_eq!(channels, 2, "stereo stem WAV is 2-channel");
    }

    #[test]
    fn render_at_alternate_sample_rates_is_deterministic_and_valid() {
        // Plan §3.3 / §4.2: `ss render --rate 44100|96000` — the engine is
        // sample-rate parametric, so a render at any rate is deterministic
        // (same project + same rate → byte-identical bytes) and the WAV
        // header records the requested rate.
        let notes = [RenderNote {
            pitch: 60,
            start_tick: 0,
            duration_ticks: u64::from(PPQ),
            velocity: 0.8,
            layer_name: "lead".to_string(),
        }];
        for rate in [44_100u32, 48_000, 96_000] {
            let config = RenderConfig {
                stereo: true,
                sample_rate: rate,
                ..RenderConfig::default()
            };
            #[expect(clippy::unwrap_used, reason = "Test assertions")]
            let a = render_notes_full(&notes, 120, rate, &[], &[], &[], &config).unwrap();
            #[expect(clippy::unwrap_used, reason = "Test assertions")]
            let b = render_notes_full(&notes, 120, rate, &[], &[], &[], &config).unwrap();
            assert_eq!(
                a.wav_bytes, b.wav_bytes,
                "{rate} Hz render is deterministic (R2)"
            );
            assert_eq!(a.sample_rate, rate, "render reports the requested rate");
            // WAV sample-rate field lives at byte offset 24 (LE u32).
            let header_rate = u32::from_le_bytes([
                a.wav_bytes[24],
                a.wav_bytes[25],
                a.wav_bytes[26],
                a.wav_bytes[27],
            ]);
            assert_eq!(header_rate, rate, "WAV header records {rate} Hz");
        }
    }

    #[test]
    fn render_cache_key_is_deterministic_and_sensitive_to_config_and_input() {
        // Phase E exit: caching keyed on input hash + render config — same
        // inputs must always produce the same key, any change must invalidate.
        let cfg_a = RenderConfig::default();
        let cfg_b = RenderConfig {
            sample_rate: 96_000,
            ..cfg_a.clone()
        };
        let key_a1 = render_cache_key("abc123", &cfg_a);
        let key_a2 = render_cache_key("abc123", &cfg_a);
        assert_eq!(
            key_a1, key_a2,
            "same input hash + config must map to the same cache key"
        );
        let key_b = render_cache_key("abc123", &cfg_b);
        assert_ne!(key_a1, key_b, "a config change must invalidate the key");
        let key_c = render_cache_key("abc124", &cfg_a);
        assert_ne!(
            key_a1, key_c,
            "an input-hash change must invalidate the key"
        );
    }

    #[test]
    fn render_cache_is_empty_then_grows_on_insert() {
        let mut cache = RenderCache::new();
        assert!(cache.is_empty());
        assert_eq!(cache.len(), 0);
        let render = LayerRender {
            stems: vec![],
            master: RenderResult {
                wav_bytes: vec![1, 2, 3],
                duration_seconds: 0.5,
                sample_rate: 48_000,
                channels: 1,
                checksum: "deadbeef".to_string(),
                mono_samples: vec![0.0],
                analysis: crate::analysis::analyze_mono(&[]),
            },
        };
        cache.insert("k".to_string(), render);
        assert_eq!(cache.len(), 1);
        assert!(!cache.is_empty());
        #[expect(clippy::expect_used, reason = "Test assertions")]
        let hit = cache.get("k").expect("key must resolve after insert");
        assert_eq!(hit.master.checksum, "deadbeef");
        assert!(hit.stems.is_empty());
        assert!(cache.get("missing").is_none(), "unknown keys must miss");
    }
}
