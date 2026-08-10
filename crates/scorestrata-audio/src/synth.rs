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
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    reason = "DSP code uses integer-to-float and float-to-index casts for sample indexing; buffer sizes are bounded and precision loss is acceptable for f32 audio math"
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
///
/// The engine is a **layered subtractive voice**: a primary oscillator, an
/// optional detuned second oscillator, a sine sub-oscillator an octave
/// below, a deterministic noise blend, a drive (tanh saturation) stage, a
/// resonant low-pass with an optional cutoff envelope (the "wow"), and an
/// optional vibrato LFO. All new fields default to their neutral value
/// (0.0 / no-op) so existing voices and serialized params render
/// byte-identically to before.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SynthParams {
    /// Primary oscillator waveform.
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
    /// Mix level of the second oscillator (0.0 = off, 1.0 = full). The
    /// second oscillator uses the same waveform detuned by
    /// [`detune_cents`](Self::detune_cents) — the classic "thick" two-osc
    /// sound that single-oscillator renders lack.
    #[serde(default)]
    pub osc2_mix: f32,
    /// Detune of the second oscillator relative to the primary, in cents.
    /// 8–14 cents is the classic analog-warmth spread.
    #[serde(default)]
    pub detune_cents: f32,
    /// Sub-oscillator mix (0.0 = off): a pure sine one octave below the
    /// primary, adding weight/body to bass and leads.
    #[serde(default)]
    pub sub_mix: f32,
    /// White-noise blend (0.0 = off) for air/texture (strings, breath,
    /// distorted-guitar fizz). Deterministic (seeded per note).
    #[serde(default)]
    pub noise_mix: f32,
    /// Drive amount (0.0 = off, 1.0 = heavy): tanh soft saturation before
    /// the filter adds the harmonic richness a bare oscillator lacks.
    #[serde(default)]
    pub drive: f32,
    /// Filter-envelope depth (0.0 = static cutoff): how far the cutoff
    /// sweeps closed from its open position over
    /// [`filter_env_decay`](Self::filter_env_decay) seconds — the "wow" of
    /// most synth patches. 1.0 sweeps fully to the base cutoff.
    #[serde(default)]
    pub filter_env_amount: f32,
    /// Filter-envelope decay in seconds (how fast the cutoff closes).
    #[serde(default)]
    pub filter_env_decay: f32,
    /// Vibrato LFO rate in Hz (0.0 = off).
    #[serde(default)]
    pub vibrato_rate_hz: f32,
    /// Vibrato LFO depth in cents of pitch modulation (0.0 = off;
    /// 5–10 cents is a subtle human vibrato).
    #[serde(default)]
    pub vibrato_depth: f32,
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
            osc2_mix: 0.0,
            detune_cents: 0.0,
            sub_mix: 0.0,
            noise_mix: 0.0,
            drive: 0.0,
            filter_env_amount: 0.0,
            filter_env_decay: 0.0,
            vibrato_rate_hz: 0.0,
            vibrato_depth: 0.0,
        }
    }
}

/// The General-MIDI drum voice for a pitch (36 = kick, 38 = snare, …).
///
/// The set grew past the Phase-A five (kick/snare/hat/open-hat/clap) to the
/// full kit a real rock/disco track needs: three toms, crash and ride. Every
/// variant has a synth fallback in [`render_drum_note`] so the bankless path
/// (R1) renders real drum sounds for any of them, and the sampled `sf2` kit
/// overrides each with a recorded one-shot.
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
    /// Floor / low tom (pitched drum, low body).
    TomLow,
    /// Rack / mid tom (pitched drum, mid body).
    TomMid,
    /// High tom (pitched drum, high body).
    TomHigh,
    /// Crash cymbal (long bright noise wash).
    Crash,
    /// Ride cymbal (metallic ring + medium noise decay).
    Ride,
}

/// Map a MIDI pitch to a drum voice using the General-MIDI drum map.
///
/// The compiler already emits these pitches for step patterns
/// (`drum_pitch_for_velocity`: kick=36, snare=38, hat=42), so percussion
/// layers render as real drums instead of tone-frequency buzzes. Note-based
/// drum layers can emit any GM pitch: open hat 46, clap 39, toms 41–50,
/// crash 57 and ride 59. Pitches 49/51 keep the legacy clap mapping (the
/// Phase-I demos write clap notes there); the kit's crash lives at 57 and
/// ride at 59 so nothing collides.
#[must_use]
pub const fn drum_kind_for_pitch(pitch: u8) -> Option<DrumKind> {
    match pitch {
        35 | 36 => Some(DrumKind::Kick),
        38 | 40 => Some(DrumKind::Snare),
        39 | 49 | 51 => Some(DrumKind::Clap),
        42 | 44 => Some(DrumKind::Hat),
        46 => Some(DrumKind::OpenHat),
        41 | 43 => Some(DrumKind::TomLow),
        45 | 47 => Some(DrumKind::TomMid),
        48 | 50 => Some(DrumKind::TomHigh),
        57 => Some(DrumKind::Crash),
        59 => Some(DrumKind::Ride),
        _ => None,
    }
}

/// `PolyBLEP` residual for a discontinuity at phase `t` given phase increment `dt`.
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
        2.0f32.mul_add(x, x * x) + 1.0
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
            2.0f32.mul_add(saw.abs(), -1.0)
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
    let q = resonance.clamp(0.0, 1.0).mul_add(7.5, 0.5);
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

/// A stateful RBJ low-pass ready for per-sample processing with a
/// time-varying cutoff (the filter envelope). Coefficients are recomputed by
/// [`set_cutoff`](Self::set_cutoff) whenever the cutoff moves.
struct DynamicLowPass {
    x1: f32,
    x2: f32,
    y1: f32,
    y2: f32,
    b0: f32,
    b1: f32,
    b2: f32,
    a1: f32,
    a2: f32,
    q: f32,
    sample_rate: f32,
}

impl DynamicLowPass {
    const fn new(q: f32, sample_rate: f32) -> Self {
        Self {
            x1: 0.0,
            x2: 0.0,
            y1: 0.0,
            y2: 0.0,
            b0: 1.0,
            b1: 0.0,
            b2: 0.0,
            a1: 0.0,
            a2: 0.0,
            q,
            sample_rate,
        }
    }

    /// Recompute the normalized RBJ coefficients for a new cutoff in Hz.
    fn set_cutoff(&mut self, cutoff_hz: f32) {
        let w0 = std::f32::consts::TAU * cutoff_hz / self.sample_rate.max(1.0);
        let alpha = w0.sin() / (2.0 * self.q);
        let cos_w0 = w0.cos();
        let a0 = 1.0 + alpha;
        self.b0 = (1.0 - cos_w0) * 0.5 / a0;
        self.b1 = (1.0 - cos_w0) / a0;
        self.b2 = self.b0;
        self.a1 = -2.0 * cos_w0 / a0;
        self.a2 = (1.0 - alpha) / a0;
    }

    /// Process one sample through the biquad (Direct Form I, matching
    /// [`apply_low_pass_filter`] sample-for-sample).
    fn process(&mut self, x: f32) -> f32 {
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

/// The layered voice renderer.
///
/// Primary + detuned second oscillator + sine sub-oscillator + deterministic
/// noise, through a drive stage, a resonant low-pass whose cutoff can sweep
/// with an envelope, and a vibrato LFO on the pitch. The amp ADSR is applied
/// at the very end (VCA).
///
/// When every new parameter is at its neutral value (0.0) the note is
/// rendered through [`render_note_legacy`] — the exact pre-upgrade path —
/// so existing voices and golden renders stay byte-identical.
#[expect(
    clippy::too_many_lines,
    reason = "Layered voice DSP composes oscillators, drive, filter, LFO and ADSR; splitting risks subtle state bugs"
)]
pub fn render_note(
    params: &SynthParams,
    frequency: f32,
    velocity: f32,
    sample_rate: f32,
    note_duration_samples: usize,
    output: &mut [f32],
) {
    let enhanced = params.osc2_mix > 0.0
        || params.sub_mix > 0.0
        || params.noise_mix > 0.0
        || params.drive > 0.0
        || params.filter_env_amount > 0.0
        || params.vibrato_depth > 0.0;
    if !enhanced {
        render_note_legacy(
            params,
            frequency,
            velocity,
            sample_rate,
            note_duration_samples,
            output,
        );
        return;
    }

    let sustain = params.sustain.clamp(0.0, 1.0);
    let attack_end = (params.attack.max(0.0) * sample_rate) as usize;
    let decay_end = attack_end.saturating_add((params.decay.max(0.0) * sample_rate) as usize);
    let release_start = note_duration_samples.min(output.len());
    let release_end =
        release_start.saturating_add((params.release.max(0.0) * sample_rate) as usize);

    let phase_inc = frequency / sample_rate.max(1.0);
    let detune_ratio = (params.detune_cents / 1200.0).exp2();
    // Deterministic noise seed derived from the note's frequency so identical
    // inputs produce identical bytes (per §17 determinism rules). Forced odd
    // so xorshift32 never sees a zero state.
    let mut noise_state = (frequency.to_bits() ^ 0x9E37_79B9) | 1;

    let q = params.filter_resonance.clamp(0.0, 1.0).mul_add(7.5, 0.5);
    let filter_bypass = params.filter_cutoff >= 0.99 && params.filter_env_amount <= 0.0;
    let base_cutoff_hz = if filter_bypass {
        20_000.0
    } else {
        20.0 * 1000.0f32.powf(params.filter_cutoff.clamp(0.0, 0.99))
    };
    // Filter envelope sweeps from an open cutoff down to the base cutoff over
    // `filter_env_decay` seconds — the classic synth "wow".
    let nyquist = sample_rate * 0.4;
    let open_cutoff_hz = base_cutoff_hz
        + params
            .filter_env_amount
            .clamp(0.0, 1.0)
            .mul_add(nyquist - base_cutoff_hz, 0.0)
            .max(0.0);
    let env_decay = params.filter_env_decay.max(0.001);

    let drive_gain = params.drive.clamp(0.0, 1.0).mul_add(8.0, 1.0);
    let drive_norm = drive_gain.tanh().max(1e-6);
    let vib_ratio = params.vibrato_depth / 1200.0;
    let vib_rate = params.vibrato_rate_hz.max(0.001);

    // 2× oversampling around the nonlinear drive: the tanh generates
    // harmonics above Nyquist that would otherwise fold back into the
    // audible band (measured 6-8% aliasing on driven guitars in the listen
    // test). Rendering the oscillator + drive at 2× the rate, low-passing
    // at the decimation stage, then keeping the (linear) resonant filter at
    // the output rate removes that foldback. The oversampled path is only
    // used when `drive > 0` so non-driven voices stay exactly as before.
    let oversample = params.drive > 0.0;
    let os_rate = sample_rate * 2.0;
    let os_inc_scale = 0.5;
    let mut os_filter = DynamicLowPass::new(0.707, os_rate);
    os_filter.set_cutoff(sample_rate * 0.45);

    let mut phase = 0.0f32;
    let mut phase2 = 0.0f32;
    let mut sub_phase = 0.0f32;
    let mut filter = DynamicLowPass::new(q, sample_rate);

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
                (1.0 - sustain).mul_add(
                    -((i - attack_end) as f32 / (decay_end - attack_end) as f32),
                    1.0,
                )
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

        let t = i as f32 / sample_rate;
        // Vibrato: sine LFO modulating pitch in cents (deterministic).
        let vib = if params.vibrato_depth > 0.0 {
            (std::f32::consts::TAU * vib_rate * t).sin() * vib_ratio
        } else {
            0.0
        };
        let inc = phase_inc * vib.exp2();

        let mut x = if oversample {
            // Two oversampled sub-samples per output sample; keep the second
            // after the anti-alias low-pass (decimation by 2).
            let mut decimated = 0.0f32;
            for _ in 0..2 {
                let sub_x = drive_sample(
                    params,
                    phase,
                    phase2,
                    sub_phase,
                    &mut noise_state,
                    inc * os_inc_scale,
                    detune_ratio,
                    drive_gain,
                    drive_norm,
                );
                // Anti-alias before decimation: removes harmonics above the
                // output Nyquist that the drive created.
                decimated = os_filter.process(sub_x);
                phase = (phase + inc * os_inc_scale) % 1.0;
                phase2 = (inc * os_inc_scale).mul_add(detune_ratio, phase2) % 1.0;
                sub_phase = (inc * os_inc_scale).mul_add(0.5, sub_phase) % 1.0;
            }
            decimated
        } else {
            let sub_x = drive_sample(
                params,
                phase,
                phase2,
                sub_phase,
                &mut noise_state,
                inc,
                detune_ratio,
                drive_gain,
                drive_norm,
            );
            phase = (phase + inc) % 1.0;
            phase2 = (phase2 + inc * detune_ratio) % 1.0;
            sub_phase = (sub_phase + inc * 0.5) % 1.0;
            sub_x
        };

        // Filter envelope: recompute the cutoff each sample while it sweeps.
        if !filter_bypass {
            if params.filter_env_amount > 0.0 {
                let env = (-t / env_decay).exp();
                let cutoff_hz = (open_cutoff_hz - base_cutoff_hz).mul_add(env, base_cutoff_hz);
                filter.set_cutoff(cutoff_hz);
            }
            x = filter.process(x);
        }

        *sample = x * envelope * velocity;
    }
}

/// Compute one mixed + driven oscillator-stack sample (shared by the direct
/// and oversampled paths). Pure function of phase state + params.
#[allow(clippy::too_many_arguments)]
fn drive_sample(
    params: &SynthParams,
    phase: f32,
    phase2: f32,
    sub_phase: f32,
    noise_state: &mut u32,
    inc: f32,
    detune_ratio: f32,
    drive_gain: f32,
    drive_norm: f32,
) -> f32 {
    // Oscillator stack (normalized so the layer never overdrives).
    let o1 = oscillator(params.waveform, phase, inc);
    let o2 = if params.osc2_mix > 0.0 {
        oscillator(params.waveform, phase2, inc * detune_ratio)
    } else {
        0.0
    };
    let sub = if params.sub_mix > 0.0 {
        (std::f32::consts::TAU * sub_phase).sin()
    } else {
        0.0
    };
    let noise = if params.noise_mix > 0.0 {
        xorshift32(noise_state)
    } else {
        0.0
    };
    let mix_sum = 1.0 + params.osc2_mix + params.sub_mix + params.noise_mix;
    let mut x = params.noise_mix.mul_add(
        noise,
        params.sub_mix.mul_add(sub, params.osc2_mix.mul_add(o2, o1)),
    ) / mix_sum.max(1.0);

    // Drive: tanh soft saturation adds the harmonic richness a bare
    // oscillator lacks (normalized so drive ≈ unity at low levels).
    if params.drive > 0.0 {
        x = (drive_gain * x).tanh() / drive_norm;
    }
    x
}

/// The original single-oscillator render path — kept byte-identical so the
/// pre-upgrade voices, tests, and golden renders are unchanged.
fn render_note_legacy(
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
                (1.0 - sustain).mul_add(
                    -((i - attack_end) as f32 / (decay_end - attack_end) as f32),
                    1.0,
                )
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
#[expect(
    clippy::too_many_lines,
    reason = "Full kit synthesis composes ten distinct voice recipes; splitting risks subtle state bugs"
)]
pub fn render_drum_note(kind: DrumKind, velocity: f32, sample_rate: f32, output: &mut [f32]) {
    match kind {
        DrumKind::Kick => {
            // Layered kick: a sine body with an exponential pitch sweep
            // (120 → 45 Hz) plus a short "beater click" transient (2 ms of
            // high-frequency noise) so the attack is audible on small
            // speakers, then a gentle saturation for punch.
            let mut phase = 0.0f32;
            let mut click_state = 0xBEAD_5EEDu32;
            for (i, s) in output.iter_mut().enumerate() {
                let t = i as f32 / sample_rate;
                let freq = 75.0f32.mul_add((-t / 0.08).exp(), 45.0);
                phase = (phase + freq / sample_rate) % 1.0;
                let amp = (-t / 0.22).exp();
                let body = (phase * std::f32::consts::TAU).sin() * amp;
                // Beater click: first ~3 ms, high-passed noise burst.
                let click = if t < 0.003 {
                    xorshift32(&mut click_state) * (1.0 - t / 0.003) * 0.9
                } else {
                    0.0
                };
                // Soft saturation rounds the click off and adds body harmonics.
                *s = (body * 1.6 + click).tanh() * 0.85 * velocity;
            }
        }
        DrumKind::Snare => {
            // Layered snare: a pitched 190 Hz body tone + a band-passed
            // noise crack (differentiated noise, like the hat but longer),
            // which gives the "crack" while the tone gives the body.
            let mut state = 0x51E5_7A3Fu32;
            let mut prev = 0.0f32;
            let mut hp = 0.0f32;
            for (i, s) in output.iter_mut().enumerate() {
                let t = i as f32 / sample_rate;
                let amp = (-t / 0.16).exp();
                let tone = (std::f32::consts::TAU * 190.0 * i as f32 / sample_rate).sin() * 0.4;
                let n = xorshift32(&mut state);
                let diff = n - prev;
                prev = n;
                hp = 0.4f32.mul_add(diff - hp, hp);
                *s = (hp * 2.4).mul_add(amp, tone) * velocity;
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
                lp = 0.28f32.mul_add(hp - lp, lp);
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
        DrumKind::TomLow | DrumKind::TomMid | DrumKind::TomHigh => {
            // Pitched tom: a sine body with a fast pitch sweep (like the
            // kick but tuned up and with a shorter decay) plus a soft skin
            // noise transient. Body roots: floor ~110 Hz, rack ~155 Hz,
            // high ~205 Hz, each sweeping down a fourth.
            let (f0, f1, decay): (f32, f32, f32) = match kind {
                DrumKind::TomLow => (110.0, 78.0, 0.30),
                DrumKind::TomMid => (155.0, 108.0, 0.24),
                _ => (205.0, 142.0, 0.18),
            };
            let mut phase = 0.0f32;
            let mut state = 0x70E7_EE0Fu32;
            for (i, s) in output.iter_mut().enumerate() {
                let t = i as f32 / sample_rate;
                let freq = f0.mul_add((-t / 0.05).exp(), f1 - f0);
                phase = (phase + freq / sample_rate) % 1.0;
                let amp = (-t / decay).exp();
                let body = (phase * std::f32::consts::TAU).sin() * amp;
                // Skin transient: first ~4 ms of low-passed noise.
                let n = if t < 0.004 {
                    xorshift32(&mut state) * 0.5
                } else {
                    0.0
                };
                *s = body.mul_add(1.3, n).tanh() * 0.9 * velocity;
            }
        }
        DrumKind::Crash => {
            // Crash: a long, bright noise wash with a fast attack and a
            // ~1.2 s decay — the "explosion" of a cymbal. High-passed noise
            // keeps it sizzly rather than rumbling.
            let mut state = 0xC84A_5C12u32;
            let mut prev = 0.0f32;
            let mut hp = 0.0f32;
            for (i, s) in output.iter_mut().enumerate() {
                let t = i as f32 / sample_rate;
                let amp = (-t / 1.2).exp();
                let n = xorshift32(&mut state);
                let diff = n - prev;
                prev = n;
                hp = 0.35f32.mul_add(diff - hp, hp);
                *s = hp * 2.6 * amp * velocity;
            }
        }
        DrumKind::Ride => {
            // Ride: a metallic ring (sine around 235 Hz with a fast decay)
            // plus a moderate noise decay — the "tick" of a ride bell over
            // a ~0.8 s body.
            let mut state = 0x81DE_B0A7u32;
            let mut prev = 0.0f32;
            let mut hp = 0.0f32;
            for (i, s) in output.iter_mut().enumerate() {
                let t = i as f32 / sample_rate;
                let amp = (-t / 0.8).exp();
                let ring = (std::f32::consts::TAU * 235.0 * i as f32 / sample_rate).sin()
                    * (-t / 0.35).exp()
                    * 0.5;
                let n = xorshift32(&mut state);
                let diff = n - prev;
                prev = n;
                hp = 0.4f32.mul_add(diff - hp, hp);
                *s = hp.mul_add(1.8, ring) * amp * velocity;
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
        #[expect(
            clippy::items_after_statements,
            reason = "Nested test helper reads naturally in place"
        )]
        fn band_energy(samples: &[f32], sr: f32, f_lo: f32, f_hi: f32) -> f32 {
            const SEG: usize = 4096;
            let mut total = 0.0f64;
            let mut bins = 0u32;
            let mut f = f64::from(f_lo);
            let sr64 = f64::from(sr);
            #[expect(
                clippy::while_float,
                reason = "Frequency sweep in fixed 500 Hz steps is intentionally float-driven"
            )]
            while f < f64::from(f_hi) {
                let w = std::f64::consts::TAU * f / sr64;
                let mut re = 0.0f64;
                let mut im = 0.0f64;
                for chunk in samples.chunks(SEG).take(8) {
                    for (i, &x) in chunk.iter().enumerate() {
                        let hann = 0.5f64
                            .mul_add(-(std::f64::consts::TAU * i as f64 / SEG as f64).cos(), 0.5);
                        let ph = w * i as f64;
                        re = (f64::from(x) * hann).mul_add(ph.cos(), re);
                        im = (f64::from(x) * hann).mul_add(-ph.sin(), im);
                    }
                }
                total += im.mul_add(im, re * re);
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
                v = (2.0 / (std::f32::consts::PI * k as f32))
                    .mul_add(-(std::f32::consts::TAU * k as f32 * phase).sin(), v);
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

    /// Deterministic single-bin DFT magnitude at `frequency` Hz (Hann-windowed
    /// over the whole buffer) — the test proxy for harmonic content.
    fn dft_magnitude(buf: &[f32], sample_rate: f32, frequency: f32) -> f32 {
        let w = std::f32::consts::TAU * frequency / sample_rate;
        let n = buf.len().max(1) as f32;
        let mut re = 0.0f32;
        let mut im = 0.0f32;
        for (i, &x) in buf.iter().enumerate() {
            // Hann window over the buffer length (one full window period),
            // NOT at the probe frequency — a probe-frequency window would
            // cancel the very bin it is trying to measure.
            let win = 0.5f32.mul_add(-(std::f32::consts::TAU * i as f32 / n).cos(), 0.5);
            let ph = w * i as f32;
            re = (x * win).mul_add(ph.cos(), re);
            im = (x * win).mul_add(-ph.sin(), im);
        }
        re.hypot(im)
    }

    /// RMS of a buffer — the flat-ADSR envelope proxy for the beating test.
    fn rms(buf: &[f32]) -> f32 {
        (buf.iter().map(|s| s * s).sum::<f32>() / buf.len().max(1) as f32).sqrt()
    }

    #[test]
    fn detuned_pair_produces_beating_amplitude_modulation() {
        // Two detuned oscillators interfere at the difference frequency,
        // producing audible amplitude beating — the "thick" analog sound.
        // With a flat ADSR the single oscillator's envelope is constant while
        // the detuned pair's peak envelope oscillates.
        let sample_rate = 48_000.0;
        let note_len = (sample_rate * 1.0) as usize; // 1 s → several beat cycles
        let flat = SynthParams {
            attack: 0.0,
            decay: 0.0,
            sustain: 1.0,
            release: 0.0,
            filter_cutoff: 1.0,
            filter_resonance: 0.0,
            ..Default::default()
        };
        let mut single = vec![0.0; note_len];
        render_note(&flat, 220.0, 1.0, sample_rate, note_len, &mut single);

        let mut pair = flat;
        pair.osc2_mix = 1.0;
        pair.detune_cents = 20.0; // ~1.15% → ~2.6 Hz beating
        let mut output = vec![0.0; note_len];
        render_note(&pair, 220.0, 1.0, sample_rate, note_len, &mut output);

        // RMS over 10 ms windows: a constant tone has near-constant RMS
        // while the beating pair's RMS oscillates at the beat rate.
        let envelope_cv = |buf: &[f32]| -> f32 {
            let step = (0.01 * sample_rate) as usize;
            let values: Vec<f32> = buf.chunks(step).map(rms).collect();
            let mean = values.iter().sum::<f32>() / values.len().max(1) as f32;
            let var = values.iter().map(|v| (v - mean) * (v - mean)).sum::<f32>()
                / values.len().max(1) as f32;
            var.sqrt() / mean.max(1e-6)
        };
        let single_cv = envelope_cv(&single);
        let pair_cv = envelope_cv(&output);
        assert!(
            pair_cv > single_cv * 3.0,
            "detuned pair beats (pair cv {pair_cv:.3} vs single cv {single_cv:.3})"
        );
    }

    #[test]
    fn drive_adds_harmonic_energy() {
        // A pure sine driven through tanh saturation gains odd harmonics:
        // energy must appear at 3× the fundamental where the clean sine has
        // (essentially) none.
        let sample_rate = 48_000.0;
        let note_len = (sample_rate * 0.5) as usize;
        let mut params = SynthParams {
            waveform: Waveform::Sine,
            filter_cutoff: 1.0, // bypass filter: isolate the drive
            filter_resonance: 0.0,
            attack: 0.0,
            decay: 0.0,
            sustain: 1.0,
            release: 0.0,
            ..Default::default()
        };
        let mut clean = vec![0.0; note_len];
        render_note(&params, 110.0, 1.0, sample_rate, note_len, &mut clean);
        params.drive = 0.9;
        let mut driven = vec![0.0; note_len];
        render_note(&params, 110.0, 1.0, sample_rate, note_len, &mut driven);

        let ratio = |buf: &[f32]| {
            dft_magnitude(buf, sample_rate, 330.0) / dft_magnitude(buf, sample_rate, 110.0)
        };
        assert!(
            ratio(&driven) > ratio(&clean) * 5.0,
            "drive adds 3rd harmonic (driven {:.4} vs clean {:.4})",
            ratio(&driven),
            ratio(&clean)
        );
    }

    #[test]
    fn filter_envelope_sweeps_high_frequency_content_down() {
        // The filter envelope opens the cutoff at note start and closes it
        // over the decay: the head must carry far more energy at a high
        // harmonic (10th, 2.2 kHz) than the tail.
        let sample_rate = 48_000.0;
        let note_len = (sample_rate * 0.3) as usize;
        let params = SynthParams {
            waveform: Waveform::Saw,
            filter_cutoff: 0.25,
            filter_resonance: 0.1,
            attack: 0.0,
            decay: 0.0,
            sustain: 1.0,
            release: 0.0,
            filter_env_amount: 0.8,
            filter_env_decay: 0.1,
            ..Default::default()
        };
        let mut out = vec![0.0; note_len];
        render_note(&params, 220.0, 1.0, sample_rate, note_len, &mut out);

        // Equal-length windows so the DFT totals are comparable. Probe the
        // 20th harmonic (4.4 kHz): the open cutoff (head, 0–50 ms) must pass
        // far more of it than the closed cutoff (tail, 150–200 ms). (2.2 kHz
        // is a poor probe here because the fast early cutoff sweep smears
        // energy across bins while the tail is quasi-static.)
        let head = &out[..(0.05 * sample_rate) as usize];
        let tail = &out[(0.15 * sample_rate) as usize..(0.2 * sample_rate) as usize];
        let head_energy = dft_magnitude(head, sample_rate, 4_400.0);
        let tail_energy = dft_magnitude(tail, sample_rate, 4_400.0);
        assert!(
            head_energy > tail_energy * 1.5,
            "filter env opens then closes (head {head_energy:.4} vs tail {tail_energy:.4})"
        );
    }

    #[test]
    fn sub_oscillator_adds_low_end_weight() {
        // The sub-oscillator is a sine an octave below; enabling it must add
        // energy at half the fundamental's frequency.
        let sample_rate = 48_000.0;
        let note_len = (sample_rate * 0.5) as usize;
        let base = SynthParams {
            waveform: Waveform::Sine,
            filter_cutoff: 1.0,
            filter_resonance: 0.0,
            attack: 0.0,
            decay: 0.0,
            sustain: 1.0,
            release: 0.0,
            sub_mix: 0.0,
            ..Default::default()
        };
        let mut plain = vec![0.0; note_len];
        render_note(&base, 220.0, 1.0, sample_rate, note_len, &mut plain);
        let mut with_sub = base;
        with_sub.sub_mix = 1.0;
        let mut out = vec![0.0; note_len];
        render_note(&with_sub, 220.0, 1.0, sample_rate, note_len, &mut out);

        let sub_share = |buf: &[f32]| {
            dft_magnitude(buf, sample_rate, 110.0) / dft_magnitude(buf, sample_rate, 220.0)
        };
        assert!(
            sub_share(&out) > sub_share(&plain) * 4.0,
            "sub adds an octave-down partial (with {:.4} vs without {:.4})",
            sub_share(&out),
            sub_share(&plain)
        );
    }

    #[test]
    fn enhanced_voice_is_deterministic() {
        // The layered path (noise + vibrato + drive) must stay deterministic:
        // identical params render identical bytes.
        let sample_rate = 48_000.0;
        let note_len = (sample_rate * 0.2) as usize;
        let params = SynthParams {
            waveform: Waveform::Saw,
            filter_cutoff: 0.5,
            filter_resonance: 0.2,
            attack: 0.01,
            decay: 0.1,
            sustain: 0.7,
            release: 0.1,
            osc2_mix: 0.6,
            detune_cents: 9.0,
            sub_mix: 0.4,
            noise_mix: 0.1,
            drive: 0.4,
            filter_env_amount: 0.3,
            filter_env_decay: 0.08,
            vibrato_rate_hz: 5.0,
            vibrato_depth: 4.0,
        };
        let mut a = vec![0.0; note_len];
        let mut b = vec![0.0; note_len];
        render_note(&params, 330.0, 0.9, sample_rate, note_len, &mut a);
        render_note(&params, 330.0, 0.9, sample_rate, note_len, &mut b);
        assert_eq!(a, b, "layered voice must be deterministic");
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
        assert_eq!(drum_kind_for_pitch(39), Some(DrumKind::Clap));
        assert_eq!(
            drum_kind_for_pitch(49),
            Some(DrumKind::Clap),
            "legacy clap pitch"
        );
        assert_eq!(drum_kind_for_pitch(43), Some(DrumKind::TomLow));
        assert_eq!(drum_kind_for_pitch(45), Some(DrumKind::TomMid));
        assert_eq!(drum_kind_for_pitch(48), Some(DrumKind::TomHigh));
        assert_eq!(drum_kind_for_pitch(57), Some(DrumKind::Crash));
        assert_eq!(drum_kind_for_pitch(59), Some(DrumKind::Ride));
        assert_eq!(drum_kind_for_pitch(60), None);
    }

    #[test]
    fn full_kit_fallbacks_render_audible() {
        // Every extended voice must produce real sound through the synth
        // fallback — the bankless path must never render silence for a
        // note-based drum layer.
        let sample_rate = 48_000.0;
        for kind in [
            DrumKind::TomLow,
            DrumKind::TomMid,
            DrumKind::TomHigh,
            DrumKind::Crash,
            DrumKind::Ride,
        ] {
            let mut out = vec![0.0f32; 14_400];
            render_drum_note(kind, 1.0, sample_rate, &mut out);
            let peak = out.iter().map(|s| s.abs()).fold(0.0f32, f32::max);
            assert!(
                peak > 0.05,
                "{kind:?} fallback renders audible (peak {peak})"
            );
        }
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
