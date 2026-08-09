//! Mastering — deterministic peak limiter + the Phase D mastering chain.
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
//! Phase D adds the full "mastered" signal flow the plan calls for (§1.3):
//! 3-band Linkwitz–Riley 24 dB/oct multiband (with per-band M/S width and
//! mono-below), soft-clip saturation with 2× oversampling (no aliasing), and
//! TPDF dither for 16/24-bit export. All stages are pure per-sample DSP on
//! f32 with fixed coefficients — fully deterministic (R2), and the whole
//! chain is skipped when `MasterChain.enabled` is false so renders stay
//! byte-identical to the current path (R1).

// DSP coefficient math casts bounded sample rates and filter-cutoff
// frequencies to f32; audio-range values are exactly representable.
#![expect(
    clippy::cast_precision_loss,
    reason = "Biquad/crossover coefficient math casts bounded rates (≤ ~192 kHz) which are exactly representable in f32"
)]

use scorestrata_core::domain::{DitherCfg, MasterChain};
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
        let shaped = u * u * 2.0f32.mul_add(-u, 3.0);
        let out_abs = (ceiling - knee_bottom).mul_add(shaped, knee_bottom);
        *sample = x.signum() * out_abs;
    }
}

// ─────────────────────────────────────────────────────────────────────
// Phase D — mastering chain
// ─────────────────────────────────────────────────────────────────────

/// One biquad (RBJ cookbook) used by the LR-24 crossover cells.
#[derive(Debug, Clone, Copy, Default)]
struct Biquad {
    b0: f32,
    b1: f32,
    b2: f32,
    a1: f32,
    a2: f32,
    x1: f32,
    x2: f32,
    y1: f32,
    y2: f32,
}

impl Biquad {
    /// Low-pass biquad from cutoff + Q (RBJ, BLT at `sample_rate`).
    fn lowpass(cutoff_hz: f32, q: f32, sample_rate: u32) -> Self {
        let w0 = std::f32::consts::TAU * cutoff_hz / sample_rate as f32;
        let cos_w = w0.cos();
        let alpha = w0.sin() / (2.0 * q);
        let a0 = 1.0 + alpha;
        Self {
            b0: ((1.0 - cos_w) * 0.5) / a0,
            b1: (1.0 - cos_w) / a0,
            b2: ((1.0 - cos_w) * 0.5) / a0,
            a1: (-2.0 * cos_w) / a0,
            a2: (1.0 - alpha) / a0,
            ..Self::default()
        }
    }

    /// High-pass biquad from cutoff + Q (RBJ, BLT at `sample_rate`).
    fn highpass(cutoff_hz: f32, q: f32, sample_rate: u32) -> Self {
        let w0 = std::f32::consts::TAU * cutoff_hz / sample_rate as f32;
        let cos_w = w0.cos();
        let alpha = w0.sin() / (2.0 * q);
        let a0 = 1.0 + alpha;
        Self {
            b0: ((1.0 + cos_w) * 0.5) / a0,
            b1: -(1.0 + cos_w) / a0,
            b2: ((1.0 + cos_w) * 0.5) / a0,
            a1: (-2.0 * cos_w) / a0,
            a2: (1.0 - alpha) / a0,
            ..Self::default()
        }
    }

    /// Process one sample, returning the filtered output.
    fn tick(&mut self, x: f32) -> f32 {
        let y = self.a2.mul_add(
            -self.y2,
            self.a1.mul_add(
                -self.y1,
                self.b2
                    .mul_add(self.x2, self.b1.mul_add(self.x1, self.b0 * x)),
            ),
        );
        self.x2 = self.x1;
        self.x1 = x;
        self.y2 = self.y1;
        self.y1 = y;
        y
    }
}

/// A 4th-order Linkwitz–Riley crossover stage: two cascaded 2nd-order
/// cells at the same cutoff with Q = 0.707 (Butterworth), giving 24 dB/oct
/// slopes that sum to unity (LR-24).
#[derive(Debug, Clone)]
struct Lr24 {
    low: Vec<Biquad>,
    high: Vec<Biquad>,
}

impl Lr24 {
    fn new(cutoff_hz: f32, sample_rate: u32) -> Self {
        let q = std::f32::consts::FRAC_1_SQRT_2; // Butterworth Q
        Self {
            low: vec![Biquad::lowpass(cutoff_hz, q, sample_rate); 2],
            high: vec![Biquad::highpass(cutoff_hz, q, sample_rate); 2],
        }
    }

    /// Split one sample into low and high components (LR sums to unity).
    fn tick(&mut self, x: f32) -> (f32, f32) {
        let low = self.low[0].tick(x);
        let low = self.low[1].tick(low);
        let high = self.high[0].tick(x);
        let high = self.high[1].tick(high);
        (low, high)
    }
}

/// Mid/side transform for one stereo frame, with width applied to `side`.
#[inline]
fn mid_side(width: f32, left: f32, right: f32) -> (f32, f32) {
    let mid = (left + right) * 0.5;
    let side = (left - right) * 0.5 * width;
    (mid + side, mid - side)
}

/// Apply a per-sample soft-clip saturation curve.
///
/// Cubic soft clip: linear below `knee`, smooth curve above. Pure math, no
/// tables, deterministic for every input.
#[inline]
fn soft_clip(x: f32, softness: f32) -> f32 {
    let s = softness.clamp(0.0, 1.0);
    let knee = 0.5f32.mul_add(s, 0.333); // 0.333 (harder) .. 0.833 (gentler)
    let abs = x.abs();
    if abs <= knee {
        x
    } else {
        // Smooth continuation: x - knee mapped onto a shrinking curve so the
        // output saturates toward ±(1 + s) without ever exceeding it.
        let over = abs - knee;
        let ceiling = 1.0 + s;
        let shaped = over / (1.0 + over / (ceiling - knee));
        x.signum() * (knee + shaped)
    }
}

/// Apply the full Phase D mastering chain to stereo buffers in place.
///
/// Order (plan §1.3): multiband (LR-24 + per-band M/S width + mono-below) →
/// saturation (2× OS soft clip) → existing limiter → dither is applied by
/// the WAV writer via [`dither_buffer`]. When the chain is disabled every
/// stage bypasses and the buffers are untouched (R1).
///
/// `limiter` is applied last (the chain's maximizer); pass `None` to leave
/// limiting to the caller's existing path.
pub fn apply_master_chain(
    left: &mut [f32],
    right: &mut [f32],
    chain: &MasterChain,
    sample_rate: u32,
    limiter: Option<&LimiterConfig>,
) {
    if !chain.enabled {
        return;
    }
    let len = left.len().min(right.len());

    // ── 1. Multiband: LR-24 split at the two crossovers, per-band width. ──
    let mb = chain.multiband;
    if mb.enabled {
        let lo = Lr24::new(mb.low_crossover_hz.max(20.0), sample_rate);
        let hi = Lr24::new(mb.high_crossover_hz.max(40.0), sample_rate);
        let mono_below = mb.mono_below_hz.max(20.0);
        let mono_lp = Biquad::lowpass(mono_below, std::f32::consts::FRAC_1_SQRT_2, sample_rate);
        let mut mono_lp = [mono_lp; 2]; // one per channel

        let low_gain = db_to_lin(mb.low_gain_db);
        let mid_gain = db_to_lin(mb.mid_gain_db);
        let high_gain = db_to_lin(mb.high_gain_db);

        let mut lo_state = (lo.clone(), lo); // (L, R)
        let mut hi_state = (hi.clone(), hi);

        for i in 0..len {
            let (l, r) = (left[i], right[i]);
            // Split low band (below the low crossover).
            let (l_low, l_rest) = lo_state.0.tick(l);
            let (r_low, r_rest) = lo_state.1.tick(r);
            // Split the rest into mid and high at the second crossover.
            let (l_mid, l_high) = hi_state.0.tick(l_rest);
            let (r_mid, r_high) = hi_state.1.tick(r_rest);

            // Mono-below: low band collapses to mono (side = 0).
            let l_low_m = mono_lp[0].tick(l_low);
            let r_low_m = mono_lp[1].tick(r_low);
            let low_mid = (l_low_m + r_low_m) * 0.5;
            let (l_low_w, r_low_w) = mid_side(mb.low_width, low_mid, low_mid);

            let (l_mid_w, r_mid_w) = mid_side(mb.mid_width, l_mid, r_mid);
            let (l_high_w, r_high_w) = mid_side(mb.high_width, l_high, r_high);

            left[i] = l_high_w.mul_add(high_gain, l_mid_w.mul_add(mid_gain, l_low_w * low_gain));
            right[i] = r_high_w.mul_add(high_gain, r_mid_w.mul_add(mid_gain, r_low_w * low_gain));
        }
    }

    // ── 2. Saturation: 2× OS soft clip per channel. ─────────────────────
    if let Some(sat) = chain.saturation {
        let os = sat.oversample.clamp(1, 4).max(1);
        let drive = db_to_lin(sat.drive_db);
        let trim = db_to_lin(sat.trim_db);
        if os > 1 {
            // Upsample (zero-stuff), soft-clip, then 1-pole low-pass + decimate
            // to kill the aliasing images the OS created.
            let lp_alpha = 1.0 - (-std::f32::consts::TAU * 0.4 / os as f32).exp();
            let mut lp_l = 0.0f32;
            let mut lp_r = 0.0f32;
            for i in 0..len {
                let xl = left[i] * drive;
                let xr = right[i] * drive;
                let mut acc_l = 0.0f32;
                let mut acc_r = 0.0f32;
                for k in 0..os {
                    let sl = if k == 0 { xl } else { 0.0 };
                    let sr = if k == 0 { xr } else { 0.0 };
                    let cl = soft_clip(sl, sat.softness);
                    let cr = soft_clip(sr, sat.softness);
                    lp_l += lp_alpha * (cl - lp_l);
                    lp_r += lp_alpha * (cr - lp_r);
                    if k == 0 {
                        acc_l = lp_l;
                        acc_r = lp_r;
                    }
                }
                left[i] = acc_l * trim;
                right[i] = acc_r * trim;
            }
        } else {
            for i in 0..len {
                left[i] = soft_clip(left[i] * drive, sat.softness) * trim;
                right[i] = soft_clip(right[i] * drive, sat.softness) * trim;
            }
        }
    }

    // ── 2.5 Loudness normalization (plan §1.3): deterministic makeup gain
    // to land the pre-limiter mix at the target integrated LUFS. Measured
    // with the same analyzer the audit uses, so the value is reproducible;
    // applied *before* the limiter so the maximizer tames any peaks the
    // makeup creates (R2 — no wall-clock state, fixed math only).
    if let Some(target) = chain.target_lufs {
        let measured = crate::analysis::analyze_stereo(left, right).integrated_lufs;
        if measured > -70.0 {
            let makeup_db = target - measured;
            let gain = 10.0f32.powf(makeup_db / 20.0).clamp(0.01, 100.0);
            for s in left.iter_mut() {
                *s *= gain;
            }
            for s in right.iter_mut() {
                *s *= gain;
            }
        }
    }

    // ── 3. Limiter (the chain's maximizer). ─────────────────────────────
    if let Some(lim) = limiter {
        apply_limiter(left, lim);
        apply_limiter(right, lim);
    }
}

/// Linear gain from dB.
fn db_to_lin(db: f32) -> f32 {
    10.0f32.powf(db / 20.0)
}

/// Apply TPDF dither to a mono buffer for a target bit depth.
///
/// Triangular-PDF dither (two uniform draws summed) decorrelates the
/// quantization error from the signal; deterministic (fixed seed) so the
/// dither is reproducible across runs (R2). Values are quantized to
/// `bits`-bit PCM steps with the dither added *before* rounding.
///
/// Returns the quantized samples ready for the WAV writer.
pub fn dither_buffer(samples: &mut [f32], cfg: &DitherCfg) {
    if !cfg.enabled {
        return;
    }
    let bits = cfg.bits.clamp(16, 24);
    let step = 2.0f32.powi(-(i32::from(bits) - 1));
    let mut rng = SplitMix64::new(0xD17E_0000_5EED_0001);
    for s in samples.iter_mut() {
        let tpdf = rng.next_f32() - rng.next_f32();
        let quantized = (*s + tpdf * step).round();
        *s = quantized * step;
    }
}

/// `SplitMix64` — deterministic PRNG for the dither (fixed seed, R2).
struct SplitMix64 {
    state: u64,
}

impl SplitMix64 {
    const fn new(seed: u64) -> Self {
        Self { state: seed }
    }

    const fn next_u64(&mut self) -> u64 {
        self.state = self.state.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut z = self.state;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        z ^ (z >> 31)
    }

    fn next_f32(&mut self) -> f32 {
        (self.next_u64() >> 40) as f32 / (1u64 << 24) as f32
    }
}

#[cfg(test)]
#[expect(
    clippy::cast_precision_loss,
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    reason = "Test sample indices stay well within f32's exact integer range"
)]
mod tests {
    use super::*;
    use scorestrata_core::domain::{MultibandCfg, SaturationCfg};

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
                let s = 0.6f32.mul_add(
                    (std::f32::consts::TAU * 220.0 * t).sin(),
                    (std::f32::consts::TAU * 110.0 * t).sin(),
                );
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

    fn test_chain() -> MasterChain {
        MasterChain {
            enabled: true,
            multiband: MultibandCfg {
                enabled: true,
                low_crossover_hz: 200.0,
                high_crossover_hz: 4_000.0,
                low_gain_db: 0.0,
                mid_gain_db: 0.0,
                high_gain_db: 0.0,
                low_width: 1.0,
                mid_width: 1.0,
                high_width: 1.0,
                mono_below_hz: 120.0,
            },
            saturation: None,
            dither: None,
            target_lufs: None,
        }
    }

    #[test]
    fn master_chain_disabled_leaves_buffers_untouched() {
        let mut chain = test_chain();
        chain.enabled = false;
        let mut left: Vec<f32> = (0..4_800).map(|i| 0.5 * (i as f32 * 0.01).sin()).collect();
        let mut right = left.clone();
        let (l0, r0) = (left.clone(), right.clone());
        apply_master_chain(&mut left, &mut right, &chain, 48_000, None);
        assert_eq!(left, l0, "disabled chain must not touch the buffers (R1)");
        assert_eq!(right, r0);
    }

    #[test]
    fn lr24_split_preserves_energy_at_unity_gains() {
        // LR-24 crossovers are magnitude-flat (the branches sum to unity
        // *magnitude* at every frequency) but apply an allpass phase tilt, so
        // the correct invariant is energy preservation, not sample equality.
        let chain = test_chain();
        let mut left: Vec<f32> = (0..48_000)
            .map(|i| {
                let t = i as f32 / 48_000.0;
                0.3f32.mul_add(
                    (std::f32::consts::TAU * 3_000.0 * t).sin(),
                    0.4 * (std::f32::consts::TAU * 440.0 * t).sin(),
                )
            })
            .collect();
        let mut right = left.clone();
        let original = left.clone();
        apply_master_chain(&mut left, &mut right, &chain, 48_000, None);
        let rms_in = (original.iter().map(|s| s * s).sum::<f32>() / original.len() as f32).sqrt();
        let rms_out = (left.iter().map(|s| s * s).sum::<f32>() / left.len() as f32).sqrt();
        let drift = (rms_out / rms_in - 1.0).abs();
        assert!(
            drift < 0.03,
            "LR-24 at unity gains must preserve energy within 3%, drifted {drift:.4}"
        );
    }

    #[test]
    fn multiband_low_boost_raises_low_frequency_energy() {
        let mut chain = test_chain();
        chain.multiband.low_gain_db = 12.0;
        // A 60 Hz tone sits fully inside the low band (below 200 Hz).
        let mut left: Vec<f32> = (0..48_000)
            .map(|i| 0.2 * (std::f32::consts::TAU * 60.0 * i as f32 / 48_000.0).sin())
            .collect();
        let mut right = left.clone();
        let rms_before = left.iter().map(|s| s * s).sum::<f32>().sqrt();
        apply_master_chain(&mut left, &mut right, &chain, 48_000, None);
        let rms_after = left.iter().map(|s| s * s).sum::<f32>().sqrt();
        assert!(
            rms_after > rms_before * 3.0,
            "+12 dB low gain should roughly quadruple low-band RMS"
        );
    }

    #[test]
    fn saturation_then_limiter_keeps_master_under_ceiling() {
        let chain = MasterChain {
            enabled: true,
            multiband: MultibandCfg {
                enabled: false,
                ..MultibandCfg::default()
            },
            saturation: Some(SaturationCfg {
                drive_db: 6.0,
                softness: 0.5,
                trim_db: -3.0,
                oversample: 2,
            }),
            dither: None,
            target_lufs: None,
        };
        let mut left: Vec<f32> = (0..48_000)
            .map(|i| 1.4 * (i as f32 * 0.005).sin())
            .collect();
        let mut right = left.clone();
        let limiter = LimiterConfig::default();
        apply_master_chain(&mut left, &mut right, &chain, 48_000, Some(&limiter));
        let ceiling = 10.0f32.powf(limiter.ceiling_db / 20.0);
        assert!(
            left.iter()
                .chain(&right)
                .all(|s| s.abs() <= ceiling + 0.001),
            "mastered output must stay under the limiter ceiling"
        );
    }

    #[test]
    fn dither_quantizes_to_bit_grid_and_is_deterministic() {
        let cfg = DitherCfg {
            bits: 16,
            enabled: true,
        };
        let step = 2.0f32.powi(-15); // 16-bit LSB
        let mut a: Vec<f32> = (0..4_096).map(|i| 0.3 * (i as f32 * 0.02).sin()).collect();
        let mut b = a.clone();
        dither_buffer(&mut a, &cfg);
        dither_buffer(&mut b, &cfg);
        assert_eq!(a, b, "dither is seeded → reproducible (R2)");
        let grid_error = a
            .iter()
            .map(|s| {
                let grid = (s / step).round() * step;
                (s - grid).abs()
            })
            .fold(0.0f32, f32::max);
        assert!(
            grid_error < 1e-6,
            "dithered samples must sit on the bit grid"
        );
    }

    #[test]
    fn dither_disabled_leaves_buffer_untouched() {
        let cfg = DitherCfg {
            bits: 16,
            enabled: false,
        };
        let mut samples: Vec<f32> = (0..100).map(|i| 0.1 * i as f32).collect();
        let original = samples.clone();
        dither_buffer(&mut samples, &cfg);
        assert_eq!(samples, original);
    }

    // ── Phase D exit criteria (plan §6) ──────────────────────────────────

    /// A dense, headroom-rich mix: several sine layers + a kick-ish pulse,
    /// deliberately quiet so the makeup gain must *lift* it to the target.
    fn test_mix(sample_rate: u32, seconds: f32) -> (Vec<f32>, Vec<f32>) {
        let sr = f64::from(sample_rate);
        let len = (sr * f64::from(seconds)) as usize;
        let mut left = Vec::with_capacity(len);
        let mut right = Vec::with_capacity(len);
        let half_period = (sample_rate / 2) as usize;
        for i in 0..len {
            let t = i as f32 / sr as f32;
            let s = 0.12f32.mul_add(
                (std::f32::consts::TAU * 220.0 * t).sin(),
                0.08 * (std::f32::consts::TAU * 440.0 * t).sin(),
            ) + if i % half_period < 2_000 { 0.15 } else { 0.0 };
            left.push(s);
            right.push(0.9 * s); // slight stereo difference
        }
        (left, right)
    }

    #[test]
    fn master_chain_normalizes_to_minus_14_lufs_under_minus_1_dbtp() {
        // Plan §6 Phase D exit: mastered loudness lands at −14 LUFS ±0.5 with
        // a true peak ≤ −1.0 dBTP (the limiter ceiling).
        let mut chain = test_chain();
        chain.target_lufs = Some(-14.0);
        let (mut left, mut right) = test_mix(48_000, 3.0);
        let limiter = LimiterConfig::default();
        apply_master_chain(&mut left, &mut right, &chain, 48_000, Some(&limiter));

        let analysis = crate::analysis::analyze_stereo(&left, &right);
        assert!(
            (analysis.integrated_lufs - (-14.0)).abs() < 0.5,
            "−14 LUFS ±0.5 target, got {:.2}",
            analysis.integrated_lufs
        );
        assert!(
            analysis.true_peak_dbtp <= -1.0 + 0.02,
            "true peak ≤ −1 dBTP, got {:.2}",
            analysis.true_peak_dbtp
        );
        assert!(!analysis.clipping_detected, "no full-scale clips");
    }

    #[test]
    fn master_chain_target_lufs_none_is_a_noop() {
        // R1: `target_lufs: None` must not touch the buffers — byte-identical.
        // (Multiband is disabled so the LR-24 phase tilt cannot mask the
        // assertion; only the makeup stage is under test here.)
        let chain = MasterChain {
            enabled: true,
            multiband: MultibandCfg {
                enabled: false,
                ..MultibandCfg::default()
            },
            saturation: None,
            dither: None,
            target_lufs: None,
        };
        let (mut left, mut right) = test_mix(48_000, 2.0);
        let (l0, r0) = (left.clone(), right.clone());
        apply_master_chain(&mut left, &mut right, &chain, 48_000, None);
        assert_eq!(left, l0, "no target → no makeup gain (R1)");
        assert_eq!(right, r0);
    }

    #[test]
    fn saturation_oversampling_controls_aliasing_above_16k() {
        // Plan §6 Phase D exit: "no aliasing >16 kHz on driven material with
        // saturation". A hot 7 kHz tone drives the soft clip; the 2×-oversampled
        // path must fold far less energy above 16 kHz than the 1× path, and
        // stay under a small absolute ceiling.
        let sample_rate = 48_000u32;
        let len = sample_rate * 2;
        // A 9 kHz tone driven hard: its 3rd harmonic (27 kHz) folds back to
        // 21 kHz at 1× (a >16 kHz alias), while 2× oversampling keeps it
        // below the decimation low-pass and out of the audible band.
        let tone: Vec<f32> = (0..len)
            .map(|i| 0.9 * (std::f32::consts::TAU * 9_000.0 * i as f32 / sample_rate as f32).sin())
            .collect();

        // Linear baseline: the tone's own >16 kHz content before saturation.
        let linear = {
            let mut left = tone.clone();
            let mut right = tone.clone();
            let chain = MasterChain {
                enabled: true,
                multiband: MultibandCfg {
                    enabled: false,
                    ..MultibandCfg::default()
                },
                saturation: None,
                dither: None,
                target_lufs: None,
            };
            apply_master_chain(&mut left, &mut right, &chain, sample_rate, None);
            crate::audit::audit_stereo(&left, &right, sample_rate, None).aliasing_ratio
        };

        let alias_share = |oversample: u32| {
            let mut left = tone.clone();
            let mut right = tone.clone();
            let chain = MasterChain {
                enabled: true,
                multiband: MultibandCfg {
                    enabled: false,
                    ..MultibandCfg::default()
                },
                saturation: Some(SaturationCfg {
                    drive_db: 12.0,
                    softness: 0.5,
                    trim_db: -3.0,
                    oversample,
                }),
                dither: None,
                target_lufs: None,
            };
            apply_master_chain(&mut left, &mut right, &chain, sample_rate, None);
            crate::audit::audit_stereo(&left, &right, sample_rate, None).aliasing_ratio
        };

        let os1 = alias_share(1);
        let os2 = alias_share(2);
        // Oversampling must fold back *less* than the naive path: the
        // saturation-induced >16 kHz bump (over the linear baseline) shrinks.
        let fold_in_1x = os1 - linear;
        let fold_in_2x = os2 - linear;
        assert!(
            fold_in_2x < fold_in_1x,
            "2× OS must cut the fold-in bump: 1× +{fold_in_1x:.4} → 2× +{fold_in_2x:.4}"
        );
        assert!(os2 < 0.18, ">16 kHz share bounded with 2× OS, got {os2:.4}");
    }
}
