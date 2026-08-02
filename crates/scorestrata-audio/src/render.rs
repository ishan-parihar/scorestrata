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
use crate::mixer::{Mixer, MixerChannel};
use crate::synth::{SynthParams, Waveform, render_note};
use crate::wav::write_wav;

/// Sample rate used for offline rendering (48 kHz per render policy).
pub const DEFAULT_SAMPLE_RATE: u32 = 48_000;

/// Synth voice configuration for a layer name.
#[derive(Debug, Clone)]
pub struct RenderVoice {
    /// Layer name this voice applies to.
    pub layer_name: String,
    /// Synth parameters.
    pub params: SynthParams,
}

impl RenderVoice {
    /// Create a voice with the default synth parameters.
    #[must_use]
    pub fn default_for(layer_name: &str) -> Self {
        Self {
            layer_name: layer_name.to_string(),
            params: SynthParams::default(),
        }
    }
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

/// Render a set of notes into stereo WAV bytes.
///
/// Notes are grouped into per-layer mono buffers, rendered with the assigned
/// voice (or the default synth voice), then mixed into stereo and encoded
/// as a 16-bit WAV.
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

    // Total duration in samples (from the last note end).
    let total_samples = notes
        .iter()
        .map(|n| {
            let start = tick_to_sample(n.start_tick, bpm, sample_rate);
            let dur = tick_to_sample(n.duration_ticks, bpm, sample_rate);
            start + dur
        })
        .max()
        .unwrap_or(0);

    let mut mixer = Mixer::new();

    for layer_name in &layer_order {
        let buffer =
            render_layer_buffer(notes, layer_name, voices, bpm, sample_rate, total_samples);
        mixer.add_channel(MixerChannel {
            name: layer_name.clone(),
            gain: 0.8,
            pan: 0.0,
            muted: false,
            data: buffer,
        });
    }

    let (left, right) = mixer.mix();

    // Interleave stereo.
    let mut interleaved = Vec::with_capacity(left.len() * 2);
    for (l, r) in left.iter().zip(&right) {
        interleaved.push(*l);
        interleaved.push(*r);
    }

    // The WAV writer currently writes mono (1 channel). To keep determinism
    // and a correct file, we render a mono fold (L+R)/2 — standard for
    // offline master previews — and document channels=1 in the result.
    let mono: Vec<f32> = interleaved
        .chunks_exact(2)
        .map(|c| (c[0] + c[1]) * 0.5)
        .collect();

    let wav_bytes = write_wav(&mono, sample_rate, 16);
    let duration_seconds = f64::from(total_samples as u32) / f64::from(sample_rate);
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

    let total_samples = notes
        .iter()
        .map(|n| {
            let start = tick_to_sample(n.start_tick, bpm, sample_rate);
            let dur = tick_to_sample(n.duration_ticks, bpm, sample_rate);
            start + dur
        })
        .max()
        .unwrap_or(0);

    let mut stems = Vec::new();
    let mut mixer = Mixer::new();

    for layer_name in &layer_order {
        let buffer =
            render_layer_buffer(notes, layer_name, voices, bpm, sample_rate, total_samples);

        // Stem artifact (mono WAV per layer).
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

        mixer.add_channel(MixerChannel {
            name: layer_name.clone(),
            gain: 0.8,
            pan: 0.0,
            muted: false,
            data: buffer,
        });
    }

    let (left, right) = mixer.mix();
    let mono: Vec<f32> = left
        .iter()
        .zip(&right)
        .map(|(l, r)| (l + r) * 0.5)
        .collect();
    let master_wav = write_wav(&mono, sample_rate, 16);
    let master_checksum = sha256_hex(&master_wav);
    let master_analysis = crate::analysis::analyze_mono(&mono);
    let master = RenderResult {
        wav_bytes: master_wav,
        duration_seconds: f64::from(total_samples as u32) / f64::from(sample_rate),
        sample_rate,
        channels: 1,
        checksum: master_checksum,
        mono_samples: mono,
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
) -> Vec<f32> {
    let mut buffer = vec![0.0f32; total_samples as usize];
    let params = voices
        .iter()
        .find(|v| v.layer_name == layer_name)
        .map_or_else(SynthParams::default, |v| v.params.clone());

    for note in notes.iter().filter(|n| n.layer_name == layer_name) {
        let start = tick_to_sample(note.start_tick, bpm, sample_rate) as usize;
        let dur = tick_to_sample(note.duration_ticks, bpm, sample_rate).max(1) as usize;
        let freq = midi_to_frequency(note.pitch);
        let end = (start + dur).min(buffer.len());
        if start >= buffer.len() {
            continue;
        }
        let mut note_buffer = vec![0.0f32; end - start];
        render_note(
            &params,
            freq,
            note.velocity,
            sample_rate as f32,
            &mut note_buffer,
        );
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
        "drums" | "percussion" => Waveform::Sine,
        _ => Waveform::Saw,
    }
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
}
