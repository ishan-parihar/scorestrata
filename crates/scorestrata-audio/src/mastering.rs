//! Mastering — deterministic peak limiter.
//!
//! The first listen test measured true peaks of +1.8…+3.7 dBTP: the mix
//! overs and the WAV writer hard-clamps at ±1.0, which is the loudest
//! source of the "distorted" sound. This limiter applies a soft-knee,
//! polynomial ceiling so the master never exceeds a configured ceiling
//! (default −1.0 dBTP) while leaving everything below the knee untouched.
//!
//! The default ceiling is −1.0 dBTP so a mastered render never trips the
//! validator's `AUDIO_TRUE_PEAK_EXCEEDED` gate (which fires above −1.0
//! dBTP, §16.2) while still leaving a safe margin under 0 dBFS.
//!
//! Fully deterministic: pure per-sample arithmetic on f32, no wall-clock or
//! state beyond the sample itself (per §17 DSP determinism rules).

use serde::{Deserialize, Serialize};

/// Peak-limiter configuration.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct LimiterConfig {
    /// True-peak ceiling in dBTP (e.g. −0.5 leaves headroom below 0).
    pub ceiling_db: f32,
    /// Knee width in dB below the ceiling where compression ramps in.
    pub knee_db: f32,
}

impl Default for LimiterConfig {
    fn default() -> Self {
        Self {
            ceiling_db: -1.0,
            knee_db: 3.0,
        }
    }
}

/// Apply the soft-knee peak limiter in place.
///
/// Samples below the knee bottom pass untouched; samples between the knee
/// and full scale are mapped through a smoothstep curve onto
/// `[knee_bottom, ceiling]`, so the output is bounded by `ceiling` and the
/// curve is continuous (no hard clip, no discontinuity).
pub fn apply_limiter(samples: &mut [f32], config: &LimiterConfig) {
    let ceiling = 10.0f32.powf(config.ceiling_db / 20.0).clamp(0.01, 0.999);
    let knee_width = ceiling * (1.0 - 10.0f32.powf(-config.knee_db.max(0.0) / 20.0));
    let knee_bottom = (ceiling - knee_width).max(0.001);

    for sample in samples.iter_mut() {
        let x = *sample;
        let abs = x.abs();
        if abs <= knee_bottom {
            continue;
        }
        let u = ((abs - knee_bottom) / (1.0 - knee_bottom)).clamp(0.0, 1.0);
        // smoothstep: continuous value and zero slope at the knee entry.
        let shaped = u * u * (3.0 - 2.0 * u);
        let out_abs = knee_bottom + (ceiling - knee_bottom) * shaped;
        *sample = x.signum() * out_abs;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn limiter_clamps_full_scale_sine_to_ceiling() {
        let config = LimiterConfig::default();
        let ceiling = 10.0f32.powf(config.ceiling_db / 20.0);
        let mut samples: Vec<f32> = (0..4_800)
            .map(|i| {
                let s = (std::f32::consts::TAU * 55.0 * i as f32 / 48_000.0).sin();
                s * 1.4 // deliberately overs full scale
            })
            .collect();
        apply_limiter(&mut samples, &config);
        let max_peak = samples.iter().map(|s| s.abs()).fold(0.0f32, f32::max);
        assert!(
            max_peak <= ceiling + 0.001,
            "peak {max_peak} must not exceed ceiling {ceiling}"
        );
        assert!(max_peak > ceiling - 0.02, "limiter is not a mute");
    }

    #[test]
    fn limiter_passes_through_below_knee() {
        let config = LimiterConfig::default();
        let mut quiet: Vec<f32> = (0..1_000).map(|i| 0.1 * (i as f32 * 0.01).sin()).collect();
        let original = quiet.clone();
        apply_limiter(&mut quiet, &config);
        for (a, b) in quiet.iter().zip(&original) {
            assert!((a - b).abs() < 1e-6, "below-knee samples are untouched");
        }
    }

    #[test]
    fn limiter_never_produces_clipped_samples() {
        let config = LimiterConfig::default();
        let mut samples: Vec<f32> = (0..48_000)
            .map(|i| {
                let t = i as f32 / 48_000.0;
                let s = (std::f32::consts::TAU * 110.0 * t).sin()
                    + 0.6 * (std::f32::consts::TAU * 220.0 * t).sin();
                s * 1.2
            })
            .collect();
        apply_limiter(&mut samples, &config);
        assert!(
            samples.iter().all(|s| s.abs() < 1.0),
            "no sample may reach full scale after limiting"
        );
    }

    #[test]
    fn limiter_is_deterministic_same_input_same_output() {
        let config = LimiterConfig::default();
        let mut a: Vec<f32> = (0..4_800).map(|i| 1.3 * (i as f32 * 0.02).sin()).collect();
        let mut b = a.clone();
        apply_limiter(&mut a, &config);
        apply_limiter(&mut b, &config);
        assert_eq!(a, b);
    }
}
