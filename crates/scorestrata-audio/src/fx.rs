//! Insert FX DSP (G5) — the "production" in production-grade.
//!
//! Deterministic, sample-rate-aware effects consumed by the renderer via
//! [`apply_fx_chain`]. Every effect is pure over its buffer + params, so
//! identical inputs yield identical output (per §17 DSP determinism).
//!
//! Effects (production-plan §7.2):
//!
//! - **HPF** — RBJ biquad high-pass (the "high-pass the pads at 120 Hz" move).
//! - **Peaking EQ** — RBJ bell filter (low-mid scoop, presence, air shelf).
//! - **Compressor** — feed-forward gain computer with attack/release smoothing.
//! - **Sidechain** — duck a layer against the kick's envelope (the
//!   electronic-music staple).
//! - **Reverb** — Schroeder–Moorer (4 comb + 2 all-pass), deterministic.
//! - **Delay** — ping-pong delay with feedback + low-pass dampening.

use scorestrata_core::domain::{CompressorCfg, FreqBand, FxChain, SendLevels};

/// Apply a full insert [`FxChain`] to a mono buffer in place.
///
/// Order: high-pass → peaking EQ → compressor. Sidechain ducking is applied
/// by [`apply_sidechain`] separately (the renderer drives it with the kick
/// layer's envelope, which is a cross-layer dependency).
pub fn apply_fx_chain(buffer: &mut [f32], fx: &FxChain, sample_rate: u32) {
    if let Some(cutoff) = fx.high_pass_hz {
        if cutoff > 0.0 {
            apply_high_pass(buffer, cutoff, sample_rate);
        }
    }
    for band in &fx.eq {
        apply_peaking_eq(buffer, *band, sample_rate);
    }
    if let Some(cfg) = &fx.compressor {
        apply_compressor(buffer, *cfg, sample_rate);
    }
}

/// RBJ biquad high-pass filter.
///
/// # Parameters
/// - `cutoff_hz`: −3 dB corner frequency.
pub fn apply_high_pass(buffer: &mut [f32], cutoff_hz: f32, sample_rate: u32) {
    let sr = f32::max(sample_rate as f32, 1.0);
    let w0 = 2.0 * std::f32::consts::PI * cutoff_hz / sr;
    let q = 0.707; // Butterworth-ish
    let cos_w0 = w0.cos();
    let alpha = w0.sin() / (2.0 * q);

    let b0 = (1.0 + cos_w0) / 2.0;
    let b1 = -(1.0 + cos_w0);
    let b2 = (1.0 + cos_w0) / 2.0;
    let a0 = 1.0 + alpha;
    let a1 = -2.0 * cos_w0;
    let a2 = 1.0 - alpha;

    let (b0, b1, b2, a1, a2) = (b0 / a0, b1 / a0, b2 / a0, a1 / a0, a2 / a0);
    let mut x1 = 0.0f32;
    let mut x2 = 0.0f32;
    let mut y1 = 0.0f32;
    let mut y2 = 0.0f32;
    for sample in buffer {
        let x = *sample;
        let y = b0 * x + b1 * x1 + b2 * x2 - a1 * y1 - a2 * y2;
        x2 = x1;
        x1 = x;
        y2 = y1;
        y1 = y;
        *sample = y;
    }
}

/// RBJ peaking (bell) EQ filter — a band of `gain_db` dB at `freq_hz`.
pub fn apply_peaking_eq(buffer: &mut [f32], band: FreqBand, sample_rate: u32) {
    if (band.gain_db.abs() < 0.01) || band.freq_hz <= 0.0 {
        return;
    }
    let sr = f32::max(sample_rate as f32, 1.0);
    let a = 10.0f32.powf(band.gain_db / 40.0);
    let w0 = 2.0 * std::f32::consts::PI * band.freq_hz / sr;
    let q = band.q.max(0.01);
    let alpha = w0.sin() / (2.0 * q);

    let b0 = 1.0 + alpha * a;
    let b1 = -2.0 * w0.cos();
    let b2 = 1.0 - alpha * a;
    let a0 = 1.0 + alpha / a;
    let a1 = -2.0 * w0.cos();
    let a2 = 1.0 - alpha / a;

    let (b0, b1, b2, a1, a2) = (b0 / a0, b1 / a0, b2 / a0, a1 / a0, a2 / a0);
    let mut x1 = 0.0f32;
    let mut x2 = 0.0f32;
    let mut y1 = 0.0f32;
    let mut y2 = 0.0f32;
    for sample in buffer {
        let x = *sample;
        let y = b0 * x + b1 * x1 + b2 * x2 - a1 * y1 - a2 * y2;
        x2 = x1;
        x1 = x;
        y2 = y1;
        y1 = y;
        *sample = y;
    }
}

/// Feed-forward compressor: detector → gain computer → attack/release
/// smoothing → gain (+ make-up).
pub fn apply_compressor(buffer: &mut [f32], cfg: CompressorCfg, sample_rate: u32) {
    let threshold = 10.0f32.powf(cfg.threshold_db / 20.0);
    let sr = f32::max(sample_rate as f32, 1.0);
    let attack_coef = one_pole_coef(cfg.attack_ms, sr);
    let release_coef = one_pole_coef(cfg.release_ms, sr);
    let makeup = 10.0f32.powf(cfg.makeup_db / 20.0);

    let mut env = 0.0f32; // smoothed |x| detector envelope
    let mut gain_env = 1.0f32; // smoothed gain
    for sample in buffer {
        let abs = sample.abs();
        let coef = if abs > env { attack_coef } else { release_coef };
        env += coef * (abs - env);

        let target_gain = if env > threshold {
            // Above threshold: output level = threshold + (env - threshold) / ratio
            let out = threshold + (env - threshold) / cfg.ratio;
            if env > 0.0 { out / env } else { 1.0 }
        } else {
            1.0
        };
        let coef = if target_gain < gain_env {
            attack_coef
        } else {
            release_coef
        };
        gain_env += coef * (target_gain - gain_env);

        *sample *= gain_env * makeup;
    }
}

/// One-pole smoothing coefficient from an ms time constant.
fn one_pole_coef(ms: f32, sample_rate: f32) -> f32 {
    let tau = (ms.max(0.01) / 1000.0) * sample_rate;
    1.0 - (-1.0 / tau).exp()
}

/// Duck `buffer` with an envelope follower derived from `kick` (sidechain).
///
/// The classic electronic-music pump: whenever the kick hits, the target
/// layer's level drops by `depth` (0.0–1.0) and recovers with `release_ms`.
pub fn apply_sidechain(
    buffer: &mut [f32],
    kick: &[f32],
    depth: f32,
    release_ms: f32,
    sample_rate: u32,
) {
    let sr = f32::max(sample_rate as f32, 1.0);
    let release_coef = one_pole_coef(release_ms, sr);
    let depth = depth.clamp(0.0, 1.0);
    let mut env = 0.0f32;
    for (i, sample) in buffer.iter_mut().enumerate() {
        let kick_abs = kick.get(i).copied().unwrap_or(0.0).abs();
        // Attack toward the kick peak (fast), decay with release.
        env = if kick_abs > env {
            kick_abs
        } else {
            env + release_coef * (kick_abs - env)
        };
        let duck = 1.0 - depth * env.clamp(0.0, 1.0);
        *sample *= duck;
    }
}

/// Schroeder–Moorer reverb: 4 comb filters + 2 all-pass in series.
/// Returns a *wet* buffer (dry stays untouched).
#[must_use]
pub fn apply_reverb(buffer: &[f32], sample_rate: u32) -> Vec<f32> {
    let sr = f32::max(sample_rate as f32, 1.0);
    // Prime-based delay lengths (ms) — deterministic, no wall-clock state.
    let comb_delays_ms = [29.7, 37.1, 41.1, 43.7];
    let allpass_delays_ms = [5.0, 1.7];
    let comb_feedback = 0.77;
    let allpass_feedback = 0.7;

    let delays: Vec<usize> = comb_delays_ms
        .iter()
        .chain(allpass_delays_ms.iter())
        .map(|ms| ((ms * sr) / 1000.0).round() as usize)
        .collect();

    // Comb stage: 4 parallel delay lines whose outputs sum into `wet`.
    let mut lines: Vec<Vec<f32>> = delays[..4].iter().map(|d| vec![0.0f32; *d]).collect();
    let mut wet = vec![0.0f32; buffer.len()];
    for (line, delay) in lines.iter_mut().zip(delays[..4].iter()) {
        let mut idx = 0usize;
        for (o, &x) in buffer.iter().enumerate() {
            let delayed = line[idx];
            wet[o] += delayed;
            line[idx] = x + comb_feedback * delayed;
            idx += 1;
            if idx >= *delay {
                idx = 0;
            }
        }
    }
    for sample in &mut wet {
        *sample /= 4.0;
    }

    // All-pass stage (two in series).
    for (line, delay) in lines[4..].iter_mut().zip(delays[4..].iter()) {
        let mut idx = 0usize;
        for o in 0..buffer.len() {
            let input = wet[o];
            let delayed = line[idx];
            wet[o] = -input + delayed;
            line[idx] = input + allpass_feedback * delayed;
            idx += 1;
            if idx >= *delay {
                idx = 0;
            }
        }
    }

    // Soft output normalization keeps the wet tail from pumping.
    let peak = wet.iter().fold(0.0f32, |m, v| m.max(v.abs())).max(1e-6);
    let norm = (1.0 / peak).min(1.0);
    wet.iter().map(|v| v * norm).collect()
}

/// Ping-pong delay with feedback + low-pass dampening. Returns a *wet* buffer.
#[must_use]
pub fn apply_delay(buffer: &[f32], sample_rate: u32, time_ms: f32, feedback: f32) -> Vec<f32> {
    let sr = f32::max(sample_rate as f32, 1.0);
    let delay = (((time_ms.max(10.0)) * sr) / 1000.0).round() as usize;
    let mut line_l = vec![0.0f32; delay];
    let mut line_r = vec![0.0f32; delay];
    let mut wet_l = vec![0.0f32; buffer.len()];
    let mut wet_r = vec![0.0f32; buffer.len()];
    let mut idx = 0usize;
    let damp = 0.4; // LP dampening coefficient (one-pole)
    let mut lp = 0.0f32;

    for (i, &x) in buffer.iter().enumerate() {
        let d_l = line_l[idx];
        let d_r = line_r[idx];
        wet_l[i] = d_l;
        wet_r[i] = d_r;
        // Ping-pong: input feeds the opposite channel, feedback crosses over.
        lp += damp * (x + feedback * d_r - lp);
        line_l[idx] = lp;
        let lp2 = {
            let mut lp2 = 0.0f32;
            lp2 += damp * (x + feedback * d_l - lp2);
            lp2
        };
        line_r[idx] = lp2;
        idx += 1;
        if idx >= delay {
            idx = 0;
        }
    }
    wet_l
        .iter()
        .zip(&wet_r)
        .map(|(l, r)| (l + r) * 0.5)
        .collect()
}

/// Fold reverb/delay send amounts into a mono buffer (dry + wet × send).
pub fn apply_sends(buffer: &mut [f32], sends: &SendLevels, sample_rate: u32) {
    if sends.reverb > 0.001 {
        let wet = apply_reverb(buffer, sample_rate);
        for (b, w) in buffer.iter_mut().zip(&wet) {
            *b += *w * sends.reverb.clamp(0.0, 1.0);
        }
    }
    if sends.delay > 0.001 {
        let wet = apply_delay(buffer, sample_rate, 375.0, 0.4);
        for (b, w) in buffer.iter_mut().zip(&wet) {
            *b += *w * sends.delay.clamp(0.0, 1.0);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn impulse() -> Vec<f32> {
        let mut buf = vec![0.0f32; 2048];
        buf[0] = 1.0;
        buf
    }

    #[test]
    fn high_pass_removes_low_frequency_content() {
        // A 60 Hz sine should be attenuated by a 300 Hz HPF.
        let sr = 48_000;
        let n = sr as usize;
        let mut low: Vec<f32> = (0..n)
            .map(|i| (2.0 * std::f32::consts::PI * 60.0 * i as f32 / sr as f32).sin())
            .collect();
        let mut high: Vec<f32> = (0..n)
            .map(|i| (2.0 * std::f32::consts::PI * 2000.0 * i as f32 / sr as f32).sin())
            .collect();
        let low_before = low.iter().map(|s| s * s).sum::<f32>();
        let high_before = high.iter().map(|s| s * s).sum::<f32>();
        apply_high_pass(&mut low, 300.0, sr);
        apply_high_pass(&mut high, 300.0, sr);
        let low_after = low.iter().map(|s| s * s).sum::<f32>();
        let high_after = high.iter().map(|s| s * s).sum::<f32>();
        // Low freq energy collapses; high freq survives.
        assert!(
            low_after < low_before * 0.05,
            "60 Hz is heavily attenuated: {low_after} vs {low_before}"
        );
        assert!(
            high_after > high_before * 0.3,
            "2 kHz survives: {high_after} vs {high_before}"
        );
    }

    #[test]
    fn peaking_eq_boosts_center_frequency() {
        let sr = 48_000;
        let band = FreqBand {
            freq_hz: 1000.0,
            gain_db: 6.0,
            q: 1.0,
        };
        let mut buf = impulse();
        let before: f32 = buf.iter().map(|s| s * s).sum();
        apply_peaking_eq(&mut buf, band, sr);
        let after: f32 = buf.iter().map(|s| s * s).sum();
        assert!(
            after > before,
            "6 dB boost increases total energy: {after} vs {before}"
        );
    }

    #[test]
    fn compressor_reduces_loud_transients() {
        let sr = 48_000;
        // A loud steady square-ish burst at 0.9 amplitude, long enough for
        // the detector + gain smoothing to fully engage.
        let mut buf: Vec<f32> = (0..sr / 2)
            .map(|i| if (i / 100) % 2 == 0 { 0.9 } else { -0.9 })
            .collect();
        let cfg = CompressorCfg {
            threshold_db: -12.0,
            ratio: 4.0,
            attack_ms: 1.0,
            release_ms: 50.0,
            makeup_db: 0.0,
        };
        let before = buf[sr as usize / 4];
        apply_compressor(&mut buf, cfg, sr);
        let after = buf[sr as usize / 4];
        assert!(
            after.abs() < before.abs() * 0.8,
            "compressor tames steady-state amplitude: {} vs {before}",
            after
        );
    }

    #[test]
    fn sidechain_ducks_target_with_kick_hits() {
        let sr = 48_000;
        let n = sr as usize / 4;
        let mut target = vec![0.5f32; n];
        // A kick impulse that decays to silence: hit at sample 0, gone by
        // sample ~2000 (so the envelope provably recovers afterwards).
        let kick: Vec<f32> = (0..n).map(|i| if i < 2000 { 1.0 } else { 0.0 }).collect();
        let before = target[0];
        apply_sidechain(&mut target, &kick, 0.6, 60.0, sr);
        assert!(
            target[0] < before * 0.6,
            "target ducks under the kick: {} vs {before}",
            target[0]
        );
        // Once the kick is gone the layer recovers to ~full level.
        let recovered = target[n - 1];
        assert!(
            recovered > before * 0.9,
            "envelope recovers after the hit: {recovered} vs {before}"
        );
    }

    #[test]
    fn reverb_produces_a_tail_after_the_impulse() {
        // Input longer than the longest comb delay (29.7 ms ≈ 1426 samples)
        // so the wet tail is fully inside the returned buffer.
        let mut buf = vec![0.0f32; 48_000];
        buf[0] = 1.0;
        let wet = apply_reverb(&buf, 48_000);
        let tail_energy: f32 = wet[2000..].iter().map(|s| s * s).sum();
        assert!(
            tail_energy > 1e-6,
            "reverb echoes past the impulse (tail energy {tail_energy})"
        );
    }

    #[test]
    fn delay_echoes_at_the_set_time() {
        let sr = 48_000;
        // Buffer long enough to hold the 100 ms echo.
        let mut buf = vec![0.0f32; sr as usize / 2];
        buf[0] = 1.0;
        let wet = apply_delay(&buf, sr, 100.0, 0.5);
        let echo = (0.1 * sr as f32).round() as usize;
        assert!(
            wet[echo].abs() > 0.01,
            "echo lands ~100 ms later: {}",
            wet[echo]
        );
    }

    #[test]
    fn fx_chain_applies_hpf_and_eq_deterministically() {
        let sr = 48_000;
        let fx = FxChain {
            high_pass_hz: Some(200.0),
            eq: vec![FreqBand {
                freq_hz: 400.0,
                gain_db: -2.0,
                q: 1.0,
            }],
            ..Default::default()
        };
        let mut a = impulse();
        let mut b = impulse();
        apply_fx_chain(&mut a, &fx, sr);
        apply_fx_chain(&mut b, &fx, sr);
        assert_eq!(a, b, "FX chain is deterministic");
    }
}
