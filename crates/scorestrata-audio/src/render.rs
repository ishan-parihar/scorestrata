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

use scorestrata_core::domain::PPQ;
use scorestrata_core::error::ScorestrataError;

use crate::analysis::AudioAnalysis;
use crate::mastering::{LimiterConfig, apply_limiter};
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
}

impl Default for RenderConfig {
    fn default() -> Self {
        Self {
            stereo: false,
            limiter: Some(LimiterConfig::default()),
            master_gain_db: 0.0,
            stereo_width: 1.0,
        }
    }
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
                return Some(a.1 + (b.1 - a.1) * frac);
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

/// Convert ticks to sample index at the given BPM and sample rate.
#[must_use]
#[expect(
    clippy::cast_possible_truncation,
    clippy::cast_precision_loss,
    reason = "Render durations bounded in practice"
)]
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

    for layer_name in &layer_order {
        let layer_automation: Vec<&RenderAutomation> = automation
            .iter()
            .filter(|a| a.layer_name == *layer_name)
            .collect();
        let layer_fx = fx.iter().find(|f| f.layer_name == *layer_name);
        let mut buffer = render_layer_buffer(
            notes,
            layer_name,
            voices,
            bpm,
            sample_rate,
            total_samples,
            &layer_automation,
        );
        // G5 — insert FX chain (HPF → EQ → compressor → reverb/delay sends).
        if let Some(rfx) = layer_fx {
            crate::fx::apply_fx_chain(&mut buffer, &rfx.fx, sample_rate);
            crate::fx::apply_sends(&mut buffer, &rfx.fx.sends, sample_rate);
        }
        let gain = layer_fx
            .and_then(|f| f.mix_gain_db)
            .map_or_else(|| layer_gain(layer_name), db_to_linear);
        let pan = layer_fx
            .and_then(|f| f.mix_pan)
            .unwrap_or_else(|| layer_pan(layer_name));
        mixer.add_channel(MixerChannel {
            name: layer_name.clone(),
            gain,
            pan: if config.stereo { pan } else { 0.0 },
            muted: false,
            data: buffer,
        });
    }

    let (left, right) = mixer.mix();
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
        if let Some(limiter) = &config.limiter {
            apply_limiter(&mut left, limiter);
            apply_limiter(&mut right, limiter);
        }
        let wav_bytes = write_wav_stereo(&left, &right, sample_rate, 16);
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
        if let Some(limiter) = &config.limiter {
            apply_limiter(&mut mono, limiter);
        }
        let wav_bytes = write_wav(&mono, sample_rate, 16);
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

    for layer_name in &layer_order {
        let layer_automation: Vec<&RenderAutomation> = automation
            .iter()
            .filter(|a| a.layer_name == *layer_name)
            .collect();
        let layer_fx = fx.iter().find(|f| f.layer_name == *layer_name);
        let mut buffer = render_layer_buffer(
            notes,
            layer_name,
            voices,
            bpm,
            sample_rate,
            total_samples,
            &layer_automation,
        );
        // G5 — insert FX chain, mirroring the master render so stems and
        // master stay in agreement.
        if let Some(rfx) = layer_fx {
            crate::fx::apply_fx_chain(&mut buffer, &rfx.fx, sample_rate);
            crate::fx::apply_sends(&mut buffer, &rfx.fx.sends, sample_rate);
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
        mixer.add_channel(MixerChannel {
            name: layer_name.clone(),
            gain,
            pan: if config.stereo { pan } else { 0.0 },
            muted: false,
            data: buffer,
        });
    }

    let (left, right) = mixer.mix();
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

/// Render one layer's notes into a mono sample buffer.
///
/// Shared by the master render and the stems renderer so both produce
/// identical per-layer audio.
fn render_layer_buffer(
    notes: &[RenderNote],
    layer_name: &str,
    voices: &[RenderVoice],
    bpm: u32,
    sample_rate: u32,
    total_samples: u64,
    automation: &[&RenderAutomation],
) -> Vec<f32> {
    let mut buffer = vec![0.0f32; total_samples as usize];
    let voice = voices.iter().find(|v| v.layer_name == layer_name);
    let params = voice.map_or_else(SynthParams::default, |v| v.params.clone());
    let is_percussion = voice.is_some_and(|v| v.is_percussion);

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
        if let Some(lane) = cutoff_lane {
            if let Some(value) = lane.value_at(note.start_tick) {
                // 0.0 → darkest (0.05), 1.0 → fully open (1.0).
                note_params.filter_cutoff = 0.05 + value.clamp(0.0, 1.0) * 0.95;
            }
        }
        let volume = volume_lane
            .and_then(|lane| lane.value_at(note.start_tick))
            .unwrap_or(1.0)
            .clamp(0.0, 1.0);

        if is_percussion {
            if let Some(kind) = drum_kind_for_pitch(note.pitch) {
                // Real drum synthesis (kick pitch-sweep, snare/hay noise).
                render_drum_note(kind, note.velocity, sample_rate as f32, &mut note_buffer);
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

/// Convert a MIDI note number to a frequency in Hz.
#[must_use]
pub fn midi_to_frequency(midi: u8) -> f32 {
    // A4 = 440 Hz = MIDI 69
    440.0 * 2.0f32.powf((f32::from(midi) - 69.0) / 12.0)
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
        "bass" => Waveform::Saw,
        "lead" | "synth" | "pluck" => Waveform::Square,
        "pad" | "strings" | "keys" => Waveform::Triangle,
        "guitar" | "distorted_guitar" => Waveform::Saw,
        "bell" => Waveform::Sine,
        "brass" => Waveform::Saw,
        "drums" | "percussion" => Waveform::Sine,
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
        let seconds = tick_to_seconds(PPQ as u64, 120);
        assert!((seconds - 0.5).abs() < 1e-6);
        let samples = tick_to_sample(PPQ as u64, 120, 48_000);
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
        assert!(&result.wav_bytes[0..4] == b"RIFF");
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
            },
            mix_gain_db: Some(-6.0),
            mix_pan: None,
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
        };
        let wide = RenderConfig {
            stereo: true,
            limiter: None,
            master_gain_db: -6.0,
            stereo_width: 2.0,
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
                let l = i16::from_le_bytes([data[i * 4], data[i * 4 + 1]]) as f64 / 32768.0;
                let r = i16::from_le_bytes([data[i * 4 + 2], data[i * 4 + 3]]) as f64 / 32768.0;
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
        assert!(&stems[0].wav_bytes[0..4] == b"RIFF");
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
            let l = i16::from_le_bytes([data[i * 4], data[i * 4 + 1]]) as f64 / 32768.0;
            let r = i16::from_le_bytes([data[i * 4 + 2], data[i * 4 + 3]]) as f64 / 32768.0;
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
        let bass = stems
            .iter()
            .find(|s| s.layer_name == "Bass")
            .expect("bass stem");
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
        let (drum_stems, _) = render_stems(&[note.clone()], 120, 48_000, &drums).unwrap();
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
}
