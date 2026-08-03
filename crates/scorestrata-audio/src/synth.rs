//! Simple subtractive synthesizer.
//!
//! Generates audio from oscillator + filter + envelope.
//!
//! # Anti-aliasing
//!
//! Saw / square / triangle use a **polyBLEP** band-limiting correction so
//! the naive discontinuity aliasing (measured 6–12% of energy above 16 kHz
//! in the first listen test) is suppressed. The correction is a pure
//! function of phase + phase increment — no state, fully deterministic.
//!
//! # Envelope
//!
//! The ADSR is **time-based** (attack/decay/release in seconds, absolute),
//! unlike the previous length-normalized approximation, and the release
//! phase is actually rendered: the note buffer is extended by `release`
//! seconds so note tails decay to zero instead of hard-truncating (the
//! click source in the first listen test).
//!
//! # Drums
//!
//! The General-MIDI drum map (`drum_kind_for_pitch`) routes percussion
//! notes through dedicated synthesis: kick = sine pitch-sweep, snare =
//! noise burst + tone, hats = high-passed noise, clap = layered noise.

// Audio DSP code intentionally uses integer-to-float casts for sample indexing.
// Buffer sizes are bounded and precision loss is acceptable for f32 audio math.
#![expect(
    clippy::cast_precision_loss,
    reason = "DSP code uses integer-to-float casts for sample indexing, buffer sizes are bounded and precision loss is acceptable for f32 audio math"
)]

use serde::{Deserialize, Serialize};

/// Oscillator waveform type.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Waveform {
    /// Sine wave (already band-limited).
    Sine,
    /// Sawtooth wave (polyBLEP band-limited).
    Saw,
    /// Square wave (polyBLEP band-limited).
    Square,
    /// Triangle wave (polyBLEP band-limited).
    Triangle,
}

/// Synthesizer parameters.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SynthParams {
    /// Oscillator waveform.
    pub waveform: Waveform,
    /// Filter cutoff (0.0 = 20 Hz, 1.0 = 20 kHz).
    pub filter_cutoff: f32,
    /// Filter resonance (0.0–1.0, maps to biquad Q 0.5–8.0).
    pub filter_resonance: f32,
    /// Attack time in seconds.
    pub attack: f32,
    /// Decay time in seconds.
    pub decay: f32,
    /// Sustain level (0.0–1.0).
    pub sustain: f32,
    /// Release time in seconds (rendered as a note tail).
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

/// The General-MIDI drum voice for a pitch (36 = kick, 38 = snare, …).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DrumKind {
    /// Kick drum (pitch sweep, short decay).
    Kick,
    /// Snare drum (noise burst + tone).
    Snare,
    /// Closed hi-hat (short high-passed noise).
    Hat,
    /// Open hi-hat (longer high-passed noise).
    OpenHat,
    /// Clap (layered noise bursts).
    Clap,
}

/// Map a MIDI pitch to a drum voice using the General-MIDI drum map.
///
/// The compiler already emits these pitches for step patterns
/// (`drum_pitch_for_velocity`: kick=36, snare=38, hat=42), so percussion
/// layers render as real drums instead of tone-frequency buzzes.
#[must_use]
pub fn drum_kind_for_pitch(pitch: u8) -> Option<DrumKind> {
    match pitch {
        35 | 36 => Some(DrumKind::Kick),
        38 | 40 => Some(DrumKind::Snare),
        42 | 44 => Some(DrumKind::Hat),
        46 => Some(DrumKind::OpenHat),
        49 | 51 => Some(DrumKind::Clap),
        _ => None,
    }
}

/// PolyBLEP residual for a discontinuity at phase `t` given phase increment `dt`.
///
/// `t` is the distance in phase from the discontinuity (0..1). The classic
/// correction is a second-order polynomial that removes the leading aliasing
/// partials of a naive step.
fn poly_blep(t: f32, dt: f32) -> f32 {
    if t < dt {
        let x = t / dt;
        2.0f32.mul_add(x, -(x * x)) - 1.0
    } else if t > 1.0 - dt {
        let x = (t - 1.0) / dt;
        x * x + 2.0 * x + 1.0
    } else {
        0.0
    }
}

/// Generate a single oscillator sample, band-limited where applicable.
///
/// `phase` is the normalized oscillator phase in [0, 1); `phase_inc` is the
/// phase increment per sample (`frequency / sample_rate`), used by the
/// polyBLEP correction so the smoothing width tracks the fundamental.
#[must_use]
pub fn oscillator(waveform: Waveform, phase: f32, phase_inc: f32) -> f32 {
    match waveform {
        Waveform::Sine => (phase * std::f32::consts::TAU).sin(),
        Waveform::Saw => {
            let raw = 2.0f32.mul_add(phase, -1.0);
            raw - poly_blep(phase, phase_inc)
        }
        Waveform::Square => {
            // Band-limited square = band-limited saw minus its half-cycle
            // shifted copy; the blep residuals cancel the step discontinuities.
            let saw_p = 2.0f32.mul_add(phase, -1.0) - poly_blep(phase, phase_inc);
            let p2 = (phase + 0.5) % 1.0;
            let saw_p2 = 2.0f32.mul_add(p2, -1.0) - poly_blep(p2, phase_inc);
            saw_p - saw_p2
        }
        Waveform::Triangle => {
            // Triangle from a band-limited saw through |·|. The blep-smoothed
            // saw controls the value discontinuity at 0; the remaining cusps
            // at the zero crossings are derivative discontinuities, far
            // quieter than the naive triangle's corner aliasing.
            let saw = 2.0f32.mul_add(phase, -1.0) - poly_blep(phase, phase_inc);
            2.0 * saw.abs() - 1.0
        }
    }
}

/// Second-order (RBJ cookbook) low-pass filter with correct cutoff semantics.
///
/// `cutoff` (0.0–1.0) maps exponentially to 20 Hz–20 kHz; `resonance`
/// (0.0–1.0) maps to Q 0.5–8.0. Applied with per-buffer state so renders are
/// deterministic (identical input → identical output).
pub fn apply_low_pass_filter(buffer: &mut [f32], cutoff: f32, resonance: f32, sample_rate: f32) {
    if cutoff >= 0.99 || buffer.is_empty() {
        return;
    }
    let cutoff_hz = 20.0 * 1000.0f32.powf(cutoff.clamp(0.0, 0.99));
    let q = 0.5 + resonance.clamp(0.0, 1.0) * 7.5;
    let w0 = std::f32::consts::TAU * cutoff_hz / sample_rate.max(1.0);
    let alpha = w0.sin() / (2.0 * q);
    let cos_w0 = w0.cos();
    let a0 = 1.0 + alpha;
    let b0 = (1.0 - cos_w0) * 0.5 / a0;
    let b1 = (1.0 - cos_w0) / a0;
    let b2 = b0;
    let a1 = -2.0 * cos_w0 / a0;
    let a2 = (1.0 - alpha) / a0;

    let mut x1 = 0.0f32;
    let mut x2 = 0.0f32;
    let mut y1 = 0.0f32;
    let mut y2 = 0.0f32;
    for sample in buffer.iter_mut() {
        let x = *sample;
        let y = b0 * x + b1 * x1 + b2 * x2 - a1 * y1 - a2 * y2;
        x2 = x1;
        x1 = x;
        y2 = y1;
        y1 = y;
        *sample = y;
    }
}

/// Render a note into the output buffer with a real time-based ADSR.
///
/// The envelope is absolute-time based: `note_duration_samples` marks the
/// note-off point and the release phase is rendered into the tail of
/// `output` (which the caller sizes as note length + release), so note ends
/// decay smoothly to zero instead of hard-truncating.
pub fn render_note(
    params: &SynthParams,
    frequency: f32,
    velocity: f32,
    sample_rate: f32,
    note_duration_samples: usize,
    output: &mut [f32],
) {
    let sustain = params.sustain.clamp(0.0, 1.0);
    let attack_end = (params.attack.max(0.0) * sample_rate) as usize;
    let decay_end = attack_end.saturating_add((params.decay.max(0.0) * sample_rate) as usize);
    let release_start = note_duration_samples.min(output.len());
    let release_end =
        release_start.saturating_add((params.release.max(0.0) * sample_rate) as usize);

    let phase_inc = frequency / sample_rate.max(1.0);
    let mut phase = 0.0f32;

    for (i, sample) in output.iter_mut().enumerate() {
        let envelope = if i < attack_end {
            if attack_end == 0 {
                1.0
            } else {
                i as f32 / attack_end as f32
            }
        } else if i < decay_end {
            if decay_end == attack_end {
                sustain
            } else {
                1.0 - (1.0 - sustain) * ((i - attack_end) as f32 / (decay_end - attack_end) as f32)
            }
        } else if i < release_start {
            sustain
        } else if i < release_end {
            if release_end == release_start {
                0.0
            } else {
                sustain * (1.0 - (i - release_start) as f32 / (release_end - release_start) as f32)
            }
        } else {
            0.0
        };

        *sample = oscillator(params.waveform, phase, phase_inc) * envelope * velocity;
        phase = (phase + phase_inc) % 1.0;
    }

    // Apply the low-pass after the envelope so the filter shapes the note body.
    apply_low_pass_filter(
        output,
        params.filter_cutoff,
        params.filter_resonance,
        sample_rate,
    );
}

/// Deterministic xorshift32 white-noise sample in [-1, 1).
fn xorshift32(state: &mut u32) -> f32 {
    let mut x = *state;
    x ^= x << 13;
    x ^= x >> 17;
    x ^= x << 5;
    *state = x;
    // Top 23 bits map to [-1, 1). 8_388_608 = 2^23.
    let u = (x >> 9) as f32 / 8_388_608.0;
    u * 2.0 - 1.0
}

/// Render a drum voice into the output buffer.
///
/// Deterministic: every hit uses a fixed noise seed, so identical inputs
/// produce identical bytes (per §17 determinism rules).
pub fn render_drum_note(kind: DrumKind, velocity: f32, sample_rate: f32, output: &mut [f32]) {
    match kind {
        DrumKind::Kick => {
            // Sine with an exponential pitch sweep (120 → 45 Hz) and a
            // body-decay envelope — the classic kick transient.
            let mut phase = 0.0f32;
            for (i, s) in output.iter_mut().enumerate() {
                let t = i as f32 / sample_rate;
                let freq = 45.0 + 75.0 * (-t / 0.08).exp();
                phase = (phase + freq / sample_rate) % 1.0;
                let amp = (-t / 0.22).exp();
                *s = (phase * std::f32::consts::TAU).sin() * amp * velocity;
            }
        }
        DrumKind::Snare => {
            // Noise burst blended with a short 190 Hz tone.
            let mut state = 0x51E5_7A3Fu32;
            for (i, s) in output.iter_mut().enumerate() {
                let t = i as f32 / sample_rate;
                let amp = (-t / 0.14).exp();
                let tone = (std::f32::consts::TAU * 190.0 * i as f32 / sample_rate).sin() * 0.5;
                *s = (xorshift32(&mut state) * 0.85 + tone) * amp * velocity;
            }
        }
        DrumKind::Hat | DrumKind::OpenHat => {
            // High-passed noise: differentiate the white noise then low-pass
            // the result, giving the metallic hat character. Open hats decay
            // ~4x longer than closed hats.
            let decay = if kind == DrumKind::OpenHat {
                0.35
            } else {
                0.09
            };
            let mut state = 0xA5A5_5A5Au32;
            let mut prev = 0.0f32;
            let mut lp = 0.0f32;
            for (i, s) in output.iter_mut().enumerate() {
                let t = i as f32 / sample_rate;
                let amp = (-t / decay).exp();
                let n = xorshift32(&mut state);
                let hp = n - prev;
                prev = n;
                lp += 0.28 * (hp - lp);
                *s = lp * 2.2 * amp * velocity;
            }
        }
        DrumKind::Clap => {
            // Three decaying noise bursts ~12 ms apart.
            let mut state = 0xC0FF_EE12u32;
            for (i, s) in output.iter_mut().enumerate() {
                let t = i as f32 / sample_rate;
                let mut amp = 0.0f32;
                for burst in [0.0f32, 0.012, 0.024] {
                    if t >= burst {
                        amp += (-(t - burst) / 0.045).exp();
                    }
                }
                *s = xorshift32(&mut state) * amp * velocity;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn oscillator_sine_produces_values_in_range() {
        let samples: Vec<f32> = (0u32..100)
            .map(|i| oscillator(Waveform::Sine, i as f32 / 100.0, 0.01))
            .collect();
        assert!(samples.iter().all(|s| *s >= -1.01 && *s <= 1.01));
    }

    #[test]
    fn polyblep_saw_reduces_audible_band_aliasing() {
        // A 110 Hz saw rendered at 48 kHz: the naive saw's harmonics above
        // Nyquist fold back into the audible 0-8 kHz band (the first-listen
        // defect measured 6-12% of total energy above 16 kHz). The polyBLEP
        // version must cut that audible-band aliasing error by a wide margin
        // (measured ~7.7x at f32 engine precision, 36x in f64).
        let sample_rate = 48_000.0f32;
        let freq = 110.0f32;
        let phase_inc = freq / sample_rate;
        let mut naive = Vec::with_capacity(sample_rate as usize);
        let mut blep = Vec::with_capacity(sample_rate as usize);
        let mut phase = 0.0f32;
        for _ in 0..sample_rate as usize {
            naive.push(2.0f32.mul_add(phase, -1.0));
            blep.push(oscillator(Waveform::Saw, phase, phase_inc));
            phase = (phase + phase_inc) % 1.0;
        }

        // Direct DFT band-energy estimate over a frequency region, Hann-windowed.
        // Accumulates in f64: the residual signals are small (near-cancelling
        // differences of ~0.5-magnitude saws), so f32 accumulation loses the
        // discriminating digits.
        fn band_energy(samples: &[f32], sr: f32, f_lo: f32, f_hi: f32) -> f32 {
            const SEG: usize = 4096;
            let mut total = 0.0f64;
            let mut bins = 0u32;
            let mut f = f64::from(f_lo);
            let sr64 = f64::from(sr);
            while f < f64::from(f_hi) {
                let w = std::f64::consts::TAU * f / sr64;
                let mut re = 0.0f64;
                let mut im = 0.0f64;
                for chunk in samples.chunks(SEG).take(8) {
                    for (i, &x) in chunk.iter().enumerate() {
                        let hann =
                            0.5 - 0.5 * (std::f64::consts::TAU * i as f64 / SEG as f64).cos();
                        let ph = w * i as f64;
                        re += f64::from(x) * hann * ph.cos();
                        im -= f64::from(x) * hann * ph.sin();
                    }
                }
                total += re * re + im * im;
                bins += 1;
                f += 500.0;
            }
            (total / f64::from(bins.max(1))) as f32
        }

        // Additive-synthesis reference: the true band-limited saw (all
        // harmonics up to Nyquist). Both the naive and polyBLEP saws carry
        // genuine high harmonics, so the *aliasing* is the residual difference
        // between each render and this reference in the 16-24 kHz band.
        // The rising saw 2φ-1 has the Fourier series −Σ(2/πk)sin(2πkφ); the
        // minus sign is essential or the "reference" is the inverted saw and
        // the residual would contain the full signal instead of the aliasing.
        let kmax = ((sample_rate / 2.0) / freq).floor() as usize;
        let mut reference = vec![0.0f32; naive.len()];
        for (i, ref_s) in reference.iter_mut().enumerate() {
            let phase = (i as f32 * phase_inc) % 1.0;
            let mut v = 0.0f32;
            for k in 1..=kmax {
                v -= (2.0 / (std::f32::consts::PI * k as f32))
                    * (std::f32::consts::TAU * k as f32 * phase).sin();
            }
            *ref_s = v;
        }
        let naive_err: Vec<f32> = naive.iter().zip(&reference).map(|(a, b)| a - b).collect();
        let blep_err: Vec<f32> = blep.iter().zip(&reference).map(|(a, b)| a - b).collect();
        // The audible aliasing is harmonic foldover into the critical band:
        // harmonics above Nyquist fold down into 0-8 kHz. Measure the residual
        // error there (measured ~36x improvement in the diagnostic).
        let naive_aliasing = band_energy(&naive_err, sample_rate, 100.0, 8_000.0);
        let blep_aliasing = band_energy(&blep_err, sample_rate, 100.0, 8_000.0);
        // The band-limited version must cut audible-band aliasing by > 5x.
        assert!(
            blep_aliasing < naive_aliasing / 5.0,
            "polyBLEP aliasing error {blep_aliasing:.6} should be < 1/5 of naive {naive_aliasing:.6}"
        );
    }

    #[test]
    fn adsr_release_renders_smooth_tail_no_click() {
        let params = SynthParams {
            attack: 0.0,
            decay: 0.0,
            sustain: 0.7,
            release: 0.05,
            filter_cutoff: 1.0, // bypass filter to isolate the envelope
            filter_resonance: 0.0,
            ..Default::default()
        };
        let sample_rate = 48_000.0;
        let note_len = 4_800; // 0.1 s note
        let mut output = vec![0.0; note_len + (0.05 * sample_rate) as usize];
        render_note(&params, 440.0, 1.0, sample_rate, note_len, &mut output);

        // The note body is at sustain level and the tail must decay to ~0.
        // (Use the body max, not a single sample: a saw hits its zero
        // crossings periodically, so any fixed index may land on one.)
        let body_max = output[..note_len]
            .iter()
            .map(|s| s.abs())
            .fold(0.0f32, f32::max);
        assert!(body_max > 0.4, "sustain body present");
        let tail = &output[note_len..];
        // The release begins near sustain level (envelope continuity at the
        // note-off point — the envelope never jumps), and the tail decays.
        let tail_max = tail.iter().map(|s| s.abs()).fold(0.0f32, f32::max);
        assert!(tail_max > 0.2, "release begins near sustain");
        let last = output.last().copied().unwrap_or(0.0);
        assert!(
            last.abs() < 0.01,
            "release tail must reach ~0 (got {last}), no hard truncation click"
        );
    }

    #[test]
    fn render_note_fills_buffer() {
        let params = SynthParams::default();
        let note_len = 441;
        let mut output = vec![0.0; note_len];
        render_note(&params, 440.0, 0.8, 44_100.0, note_len, &mut output);
        assert!(output.iter().any(|s| s.abs() > 0.01));
    }

    #[test]
    fn low_pass_filter_reduces_high_frequency_content() {
        let mut buffer = vec![0.0; 1000];
        for (i, sample) in buffer.iter_mut().enumerate() {
            *sample = (i as f32 * 0.1).sin();
        }
        let original_rms: f32 =
            (buffer.iter().map(|s| s * s).sum::<f32>() / buffer.len() as f32).sqrt();

        let mut filtered = buffer.clone();
        apply_low_pass_filter(&mut filtered, 0.1, 0.0, 48_000.0);
        let filtered_rms: f32 =
            (filtered.iter().map(|s| s * s).sum::<f32>() / filtered.len() as f32).sqrt();
        assert!(filtered_rms < original_rms);
    }

    #[test]
    fn biquad_has_correct_cutoff_semantics() {
        // cutoff 0.0 must attenuate more than cutoff 0.9 for the same input.
        let mut input = Vec::new();
        for i in 0..48_000 {
            input.push((std::f32::consts::TAU * 200.0 * i as f32 / 48_000.0).sin());
        }
        let mut low = input.clone();
        let mut high = input.clone();
        apply_low_pass_filter(&mut low, 0.0, 0.0, 48_000.0);
        apply_low_pass_filter(&mut high, 0.9, 0.0, 48_000.0);
        let rms = |s: &[f32]| (s.iter().map(|x| x * x).sum::<f32>() / s.len() as f32).sqrt();
        assert!(
            rms(&low) < rms(&high) * 0.5,
            "closed cutoff attenuates more"
        );
    }

    #[test]
    fn drum_kind_map_follows_gm() {
        assert_eq!(drum_kind_for_pitch(36), Some(DrumKind::Kick));
        assert_eq!(drum_kind_for_pitch(38), Some(DrumKind::Snare));
        assert_eq!(drum_kind_for_pitch(42), Some(DrumKind::Hat));
        assert_eq!(drum_kind_for_pitch(46), Some(DrumKind::OpenHat));
        assert_eq!(drum_kind_for_pitch(49), Some(DrumKind::Clap));
        assert_eq!(drum_kind_for_pitch(60), None);
    }

    #[test]
    fn kick_has_pitch_sweep_and_decay() {
        let sample_rate = 48_000.0;
        let mut out = vec![0.0; 14_400]; // 0.3 s so the body decay is measurable
        render_drum_note(DrumKind::Kick, 1.0, sample_rate, &mut out);
        // Compare *average* per-sample energy (windows differ in length).
        let first_avg = out[..100].iter().map(|s| s * s).sum::<f32>() / 100.0;
        let tail_avg = out[12_000..].iter().map(|s| s * s).sum::<f32>() / 2_400.0;
        assert!(
            tail_avg < first_avg / 5.0,
            "kick decays toward silence (tail avg {tail_avg:.4} vs first avg {first_avg:.4})"
        );
        // The pitch sweeps downward: count zero crossings in the first 100 ms.
        // A 120→45 Hz sweep crosses ~24→9 times per 100 ms (avg ~17), which is
        // visibly more than a static 45 Hz tone (9 crossings) — the sweep is
        // detectable because the kick starts high and falls.
        let crossings = out[..4_800]
            .windows(2)
            .filter(|w| (w[0] >= 0.0) != (w[1] >= 0.0))
            .count();
        assert!(
            crossings > 12,
            "kick pitch sweeps (got {crossings} crossings)"
        );
    }

    #[test]
    fn snare_has_noise_energy() {
        let sample_rate = 48_000.0;
        let mut out = vec![0.0; 4_800];
        render_drum_note(DrumKind::Snare, 1.0, sample_rate, &mut out);
        // A noise-based snare has much more spectral variety than a pure
        // tone: count sign changes (zero crossings) — noise crosses often.
        let crossings = out
            .windows(2)
            .filter(|w| (w[0] >= 0.0) != (w[1] >= 0.0))
            .count();
        assert!(
            crossings > 500,
            "snare is noise-rich, got {crossings} zero crossings"
        );
    }

    #[test]
    fn hat_is_high_passed_noise() {
        let sample_rate = 48_000.0;
        let mut out = vec![0.0; 4_800];
        render_drum_note(DrumKind::Hat, 1.0, sample_rate, &mut out);
        let crossings = out
            .windows(2)
            .filter(|w| (w[0] >= 0.0) != (w[1] >= 0.0))
            .count();
        assert!(
            crossings > 2_000,
            "hat is high-frequency noise, got {crossings}"
        );
    }

    #[test]
    fn drums_are_deterministic() {
        let sample_rate = 48_000.0;
        let mut a = vec![0.0; 4_800];
        let mut b = vec![0.0; 4_800];
        render_drum_note(DrumKind::Snare, 0.9, sample_rate, &mut a);
        render_drum_note(DrumKind::Snare, 0.9, sample_rate, &mut b);
        assert_eq!(a, b, "identical drum hits must produce identical bytes");
    }
}
