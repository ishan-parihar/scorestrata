//! Drum sampler — plays back audio samples at MIDI note positions.
//!
//! The architecture spec requires: "implement drum sampler, implement
//! simple subtractive synth, implement bass synth."

use serde::{Deserialize, Serialize};

/// A single drum sample with its metadata.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DrumSample {
    /// Sample name (e.g. "kick", "snare", "hihat").
    pub name: String,
    /// MIDI note that triggers this sample.
    pub midi_note: u8,
    /// Sample data (raw PCM f32 samples, mono).
    pub data: Vec<f32>,
    /// Sample rate in Hz.
    pub sample_rate: u32,
    /// Base velocity (0.0–1.0).
    pub base_velocity: f32,
}

/// Drum sampler instrument.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct DrumSampler {
    /// Loaded samples, keyed by MIDI note.
    pub samples: std::collections::BTreeMap<u8, DrumSample>,
}

impl DrumSampler {
    /// Create a new empty sampler.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Load a sample for a MIDI note.
    pub fn load_sample(&mut self, sample: DrumSample) {
        self.samples.insert(sample.midi_note, sample);
    }

    /// Get a sample by MIDI note.
    #[must_use]
    pub fn get_sample(&self, midi_note: u8) -> Option<&DrumSample> {
        self.samples.get(&midi_note)
    }

    /// Render a single note at a position.
    ///
    /// Note: adds to existing output buffer; clear buffer first for isolated
    /// rendering. This is intentional for mixing multiple samples together.
    pub fn render_note(&self, midi_note: u8, velocity: f32, output: &mut [f32]) {
        if let Some(sample) = self.samples.get(&midi_note) {
            let gain = velocity * sample.base_velocity;
            for (i, out) in output.iter_mut().enumerate() {
                if i < sample.data.len() {
                    *out = sample.data[i].mul_add(gain, *out);
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sampler_loads_and_retrieves_sample() {
        let mut sampler = DrumSampler::new();
        let sample = DrumSample {
            name: "kick".to_string(),
            midi_note: 36,
            data: vec![0.5, 0.3, 0.1],
            sample_rate: 44100,
            base_velocity: 1.0,
        };
        sampler.load_sample(sample);
        assert!(sampler.get_sample(36).is_some());
        assert!(sampler.get_sample(37).is_none());
    }

    #[test]
    fn render_note_adds_to_output() {
        let mut sampler = DrumSampler::new();
        sampler.load_sample(DrumSample {
            name: "kick".to_string(),
            midi_note: 36,
            data: vec![0.5, 0.3, 0.1],
            sample_rate: 44100,
            base_velocity: 1.0,
        });
        let mut output = vec![0.0; 4];
        sampler.render_note(36, 0.8, &mut output);
        assert!((output[0] - 0.4).abs() < 0.001);
    }

    #[test]
    fn render_note_ignores_unknown_midi_note() {
        let sampler = DrumSampler::new();
        let mut output = vec![0.0; 4];
        sampler.render_note(36, 0.8, &mut output);
        assert_eq!(output, vec![0.0; 4]);
    }
}
