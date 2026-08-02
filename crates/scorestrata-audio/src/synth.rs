//! Simple subtractive synthesizer.
//!
//! Generates audio from oscillator + filter + envelope.

// Audio DSP code intentionally uses integer-to-float casts for sample indexing.
// Buffer sizes are bounded and precision loss is acceptable for f32 audio math.
#![expect(
    clippy::cast_precision_loss,
    reason = "DSP code uses integer-to-float casts for sample indexing, buffer sizes are bounded and precision loss is acceptable for f32 audio math"
)]

use serde::{Deserialize, Serialize};

/// Oscillator waveform type.
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub enum Waveform {
    /// Sine wave.
    Sine,
    /// Sawtooth wave.
    Saw,
    /// Square wave.
    Square,
    /// Triangle wave.
    Triangle,
}

/// Synthesizer parameters.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SynthParams {
    /// Oscillator waveform.
    pub waveform: Waveform,
    /// Filter cutoff frequency (0.0–1.0).
    pub filter_cutoff: f32,
    /// Filter resonance (0.0–1.0).
    pub filter_resonance: f32,
    /// Attack time in seconds.
    pub attack: f32,
    /// Decay time in seconds.
    pub decay: f32,
    /// Sustain level (0.0–1.0).
    pub sustain: f32,
    /// Release time in seconds.
    pub release: f32,
}

impl Default for SynthParams {
    fn default() -> Self {
        Self {
            waveform: Waveform::Saw,
            filter_cutoff: 0.8,
            filter_resonance: 0.2,
            attack: 0.01,
            decay: 0.1,
            sustain: 0.7,
            release: 0.2,
        }
    }
}

/// Generate a single oscillator sample.
#[must_use]
pub fn oscillator(waveform: Waveform, phase: f32) -> f32 {
    match waveform {
        Waveform::Sine => (phase * std::f32::consts::TAU).sin(),
        Waveform::Saw => 2.0f32.mul_add(phase, -1.0),
        Waveform::Square => {
            if phase < 0.5 {
                1.0
            } else {
                -1.0
            }
        }
        Waveform::Triangle => {
            if phase < 0.5 {
                4.0f32.mul_add(phase, -1.0)
            } else {
                4.0f32.mul_add(-phase, 3.0)
            }
        }
    }
}

/// Simple one-pole low-pass filter.
///
/// Applies a simple low-pass filter to a buffer. The `cutoff` parameter
/// controls the filter frequency (0.0 = fully open, 1.0 = fully closed).
/// The `resonance` parameter adds emphasis at the cutoff frequency.
pub fn apply_low_pass_filter(buffer: &mut [f32], cutoff: f32, resonance: f32) {
    // Skip filtering when cutoff is near 1.0 (fully open = no filtering)
    if cutoff >= 0.99 {
        return;
    }
    // Map cutoff 0.0–0.99 to a coefficient 0.0–0.5
    // Lower cutoff = more filtering
    let coeff = (1.0 - cutoff) * 0.5;
    let res_boost = resonance.mul_add(2.0, 1.0);

    let mut prev = 0.0f32;
    for sample in buffer.iter_mut() {
        // Simple one-pole IIR filter
        let filtered = prev + coeff * (*sample - prev);
        prev = filtered;
        // Apply resonance boost: blend original with filtered
        *sample = (*sample)
            .mul_add(1.0 - res_boost * 0.5, filtered * res_boost * 0.5)
            .clamp(-1.0, 1.0);
    }
}

/// Render a note with the given parameters into the output buffer.
///
/// The ADSR envelope is applied with guards against zero attack/decay times.
/// When attack or decay is zero, the envelope jumps immediately to the next phase.
///
/// The filter is applied after the envelope using `filter_cutoff` and
/// `filter_resonance` parameters.
pub fn render_note(
    params: &SynthParams,
    frequency: f32,
    velocity: f32,
    sample_rate: f32,
    output: &mut [f32],
) {
    let mut phase = 0.0f32;
    let phase_inc = frequency / sample_rate;
    let len = output.len() as f32;

    for (i, sample) in output.iter_mut().enumerate() {
        let t = i as f32 / len;

        // ADSR envelope with guards against zero attack/decay
        let envelope = if params.attack <= 0.0 || t < params.attack {
            // If attack is zero or we're in attack phase
            if params.attack <= 0.0 {
                1.0 // Jump to peak immediately
            } else {
                t / params.attack
            }
        } else if params.decay <= 0.0 || t < params.attack + params.decay {
            // If decay is zero or we're in decay phase
            if params.decay <= 0.0 {
                params.sustain // Jump to sustain immediately
            } else {
                (1.0 - params.sustain).mul_add(-((t - params.attack) / params.decay), 1.0)
            }
        } else {
            // Sustain phase
            params.sustain
        };

        *sample = oscillator(params.waveform, phase) * envelope * velocity;
        phase = (phase + phase_inc) % 1.0;
    }

    // Apply low-pass filter after envelope
    apply_low_pass_filter(output, params.filter_cutoff, params.filter_resonance);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn oscillator_sine_produces_values_in_range() {
        let samples: Vec<f32> = (0u32..100)
            .map(|i| oscillator(Waveform::Sine, i as f32 / 100.0))
            .collect();
        assert!(samples.iter().all(|s| *s >= -1.01 && *s <= 1.01));
    }

    #[test]
    fn render_note_fills_buffer() {
        let params = SynthParams::default();
        let mut output = vec![0.0; 441];
        render_note(&params, 440.0, 0.8, 44100.0, &mut output);
        assert!(output.iter().any(|s| s.abs() > 0.01));
    }

    #[test]
    fn render_note_handles_zero_attack() {
        let params = SynthParams {
            attack: 0.0,
            filter_cutoff: 1.0, // Disable filtering to test envelope only
            filter_resonance: 0.0,
            ..Default::default()
        };
        let mut output = vec![0.0; 100];
        render_note(&params, 440.0, 1.0, 44100.0, &mut output);
        // With zero attack, first sample should be at full amplitude
        assert!(output[0].abs() > 0.5);
    }

    #[test]
    fn render_note_handles_zero_decay() {
        let params = SynthParams {
            attack: 0.0,
            decay: 0.0,
            sustain: 0.5,
            ..Default::default()
        };
        let mut output = vec![0.0; 100];
        render_note(&params, 440.0, 1.0, 44100.0, &mut output);
        // With zero decay, should jump to sustain level
        assert!(output[50].abs() > 0.1);
    }

    #[test]
    fn low_pass_filter_reduces_high_frequency_content() {
        let mut buffer = vec![0.0; 1000];
        // Generate a signal with high frequency content
        for (i, sample) in buffer.iter_mut().enumerate() {
            *sample = (i as f32 * 0.1).sin();
        }
        let original_rms: f32 =
            (buffer.iter().map(|s| s * s).sum::<f32>() / buffer.len() as f32).sqrt();

        // Apply low-pass filter with low cutoff
        let mut filtered = buffer.clone();
        apply_low_pass_filter(&mut filtered, 0.1, 0.0);
        let filtered_rms: f32 =
            (filtered.iter().map(|s| s * s).sum::<f32>() / filtered.len() as f32).sqrt();

        // Filtered signal should have less energy
        assert!(filtered_rms < original_rms);
    }
}
