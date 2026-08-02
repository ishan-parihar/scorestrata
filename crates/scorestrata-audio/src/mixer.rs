//! Audio mixer — mixes multiple audio tracks into a single output.
//!
//! The architecture spec requires: "implement mixer, implement gain/pan."

use serde::{Deserialize, Serialize};

/// A single mixer channel.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MixerChannel {
    /// Channel name.
    pub name: String,
    /// Gain (0.0–2.0).
    pub gain: f32,
    /// Pan (-1.0 left, 0.0 center, 1.0 right).
    pub pan: f32,
    /// Whether channel is muted.
    pub muted: bool,
    /// Audio data (mono f32 samples).
    pub data: Vec<f32>,
}

/// Audio mixer — mixes channels into stereo output.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Mixer {
    /// All mixer channels.
    pub channels: Vec<MixerChannel>,
}

impl Mixer {
    /// Create a new empty mixer.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Add a channel.
    pub fn add_channel(&mut self, channel: MixerChannel) {
        self.channels.push(channel);
    }

    /// Mix all channels into stereo output.
    ///
    /// Uses constant-power pan law: left = gain * cos(pan * π/2),
    /// right = gain * sin(pan * π/2). Pan range: -1.0 (left) to 1.0 (right).
    #[must_use]
    pub fn mix(&self) -> (Vec<f32>, Vec<f32>) {
        let max_len = self
            .channels
            .iter()
            .map(|c| c.data.len())
            .max()
            .unwrap_or(0);
        let mut left = vec![0.0f32; max_len];
        let mut right = vec![0.0f32; max_len];

        for channel in &self.channels {
            if channel.muted {
                continue;
            }
            // Constant-power pan law (symmetric)
            let pan_norm = f32::midpoint(channel.pan, 1.0); // Map -1..1 to 0..1
            let left_gain = channel.gain * (pan_norm * std::f32::consts::FRAC_PI_2).cos();
            let right_gain = channel.gain * (pan_norm * std::f32::consts::FRAC_PI_2).sin();
            for (i, &sample) in channel.data.iter().enumerate() {
                left[i] += sample * left_gain;
                right[i] += sample * right_gain;
            }
        }

        (left, right)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mixer_mixes_channels() {
        let mut mixer = Mixer::new();
        mixer.add_channel(MixerChannel {
            name: "drums".to_string(),
            gain: 1.0,
            pan: 0.0,
            muted: false,
            data: vec![0.5, 0.3],
        });
        mixer.add_channel(MixerChannel {
            name: "bass".to_string(),
            gain: 0.8,
            pan: 0.0,
            muted: false,
            data: vec![0.2, 0.4],
        });
        let (left, right) = mixer.mix();
        // At center pan (0.0), left and right should be equal
        assert!((left[0] - right[0]).abs() < 0.001);
        // With constant-power pan law at center, gain is scaled by cos(π/4) ≈ 0.707
        // left[0] = 0.5*0.707 + 0.2*0.8*0.707 ≈ 0.47
        assert!((left[0] - 0.47).abs() < 0.02);
    }

    #[test]
    fn mixer_skips_muted_channels() {
        let mut mixer = Mixer::new();
        mixer.add_channel(MixerChannel {
            name: "drums".to_string(),
            gain: 1.0,
            pan: 0.0,
            muted: true,
            data: vec![0.5],
        });
        let (left, _) = mixer.mix();
        assert!((left[0]).abs() < f32::EPSILON);
    }

    #[test]
    fn mixer_pan_law_is_symmetric() {
        let mut mixer = Mixer::new();
        // Left-panned channel
        mixer.add_channel(MixerChannel {
            name: "left".to_string(),
            gain: 1.0,
            pan: -0.5,
            muted: false,
            data: vec![1.0],
        });
        let (left_l, right_l) = mixer.mix();

        let mut mixer2 = Mixer::new();
        // Right-panned channel (opposite pan)
        mixer2.add_channel(MixerChannel {
            name: "right".to_string(),
            gain: 1.0,
            pan: 0.5,
            muted: false,
            data: vec![1.0],
        });
        let (left_r, right_r) = mixer2.mix();

        // Left channel of left-panned should equal right channel of right-panned
        assert!((left_l[0] - right_r[0]).abs() < 0.001);
        // Right channel of left-panned should equal left channel of right-panned
        assert!((right_l[0] - left_r[0]).abs() < 0.001);
    }
}
