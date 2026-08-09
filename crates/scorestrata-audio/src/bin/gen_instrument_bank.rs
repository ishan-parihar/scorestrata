//! Generate the embedded melodic instrument bank (Phase G).
//!
//! Writes deterministic 48 kHz mono WAV one-shots for **guitar** (plucked
//! string), **bass** (plucked string, low register) and **e-piano** (looped
//! additive tone) into `crates/scorestrata-audio/assets/instruments/`. The
//! multi-sample engine (`multisample.rs`) loads them via `include_bytes!`
//! through the zone manifest in `embedded_bank.rs`.
//!
//! This binary is deliberately **self-contained** (its own minimal WAV
//! writer, no dependency on the library crate) so it can run *before* the
//! embedded assets exist — the `include_bytes!` in `embedded_bank.rs`
//! requires the generated files on disk.
//!
//! Run:
//! ```sh
//! cargo run -p scorestrata-audio --bin gen_instrument_bank
//! ```
//!
//! The bank is *asset-free* (R7): every one-shot is parametric DSP — a
//! Karplus-Strong plucked string (noise-excited delay line with one-zero
//! low-pass in the feedback loop + body resonance) — so renders are
//! deterministic and licensing-free. The loader is asset-agnostic: a real
//! CC0 WAV set can replace these files without code changes.

// Asset DSP uses bounded integer/float casts and SplitMix noise; the generator
// is a build-time tool so expect/panic on malformed output is intentional.
#![expect(
    clippy::cast_possible_truncation,
    clippy::cast_precision_loss,
    clippy::cast_sign_loss,
    clippy::expect_used,
    reason = "Build-time asset generator: fixed 48 kHz rates, bounded casts, and panics surface tool bugs loudly"
)]

use std::path::PathBuf;

const RATE: u32 = 48_000;

fn main() {
    let out_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("assets/instruments");
    std::fs::create_dir_all(&out_dir).expect("create assets dir");

    // ── Guitar ─────────────────────────────────────────────────────────
    // Zones at A2/E3/B3/E4, each with soft + hard velocity layers.
    let guitar_roots = [45u8, 52, 59, 66];
    for (i, root) in guitar_roots.iter().enumerate() {
        for (hard, name) in [(false, "soft"), (true, "hard")] {
            let pluck = plucked_string(
                midi_freq(*root),
                1.7,
                if hard { 0.85 } else { 0.45 },
                if hard { 0.35 } else { 0.55 },
                0.18,
            );
            write_wav_file(&out_dir, &format!("guitar_{root}_{name}.wav"), &pluck);
            let _ = i;
        }
    }

    // ── Bass ───────────────────────────────────────────────────────────
    // Zones at A1/C2/G2, soft + hard layers.
    let bass_roots = [29u8, 36, 43];
    for (i, root) in bass_roots.iter().enumerate() {
        for (hard, name) in [(false, "soft"), (true, "hard")] {
            let pluck = plucked_string(
                midi_freq(*root),
                2.4,
                if hard { 0.9 } else { 0.5 },
                if hard { 0.3 } else { 0.5 },
                0.12,
            );
            write_wav_file(&out_dir, &format!("bass_{root}_{name}.wav"), &pluck);
            let _ = i;
        }
    }

    // ── E-piano ────────────────────────────────────────────────────────
    // Zones at C3/C4/C5, single layer, looped sustain.
    let epiano_roots = [48u8, 60, 72];
    for (i, root) in epiano_roots.iter().enumerate() {
        let tone = epiano_tone(midi_freq(*root), 3.0);
        write_wav_file(&out_dir, &format!("epiano_{root}.wav"), &tone);
        let _ = i;
    }

    println!("Generated instrument bank -> {}", out_dir.display());
}

/// MIDI pitch → frequency in Hz.
fn midi_freq(midi: u8) -> f32 {
    440.0 * ((f32::from(midi) - 69.0) / 12.0).exp2()
}

/// Write a mono 24-bit PCM WAV (minimal local writer — keeps this binary
/// independent of the library crate so it can bootstrap the embedded assets).
fn write_wav_file(dir: &std::path::Path, name: &str, samples: &[f32]) {
    let mut wav = Vec::with_capacity(44 + samples.len() * 3);
    wav.extend_from_slice(b"RIFF");
    #[expect(clippy::cast_possible_truncation, reason = "sample count fits u32")]
    let data_size = samples.len() as u32 * 3;
    wav.extend_from_slice(&(36 + data_size).to_le_bytes());
    wav.extend_from_slice(b"WAVE");
    wav.extend_from_slice(b"fmt ");
    wav.extend_from_slice(&16u32.to_le_bytes());
    wav.extend_from_slice(&1u16.to_le_bytes()); // PCM
    wav.extend_from_slice(&1u16.to_le_bytes()); // mono
    wav.extend_from_slice(&RATE.to_le_bytes());
    wav.extend_from_slice(&(RATE * 3).to_le_bytes()); // byte rate
    wav.extend_from_slice(&3u16.to_le_bytes()); // block align
    wav.extend_from_slice(&24u16.to_le_bytes()); // bits
    wav.extend_from_slice(b"data");
    wav.extend_from_slice(&data_size.to_le_bytes());
    for &s in samples {
        #[expect(clippy::cast_possible_truncation, reason = "clamped f32 fits i24")]
        let v = (s.clamp(-1.0, 1.0) * 8_388_607.0).round() as i32;
        #[expect(clippy::cast_sign_loss, reason = "masked bytes are non-negative")]
        {
            wav.push((v & 0xFF) as u8);
            wav.push(((v >> 8) & 0xFF) as u8);
            wav.push(((v >> 16) & 0xFF) as u8);
        }
    }
    std::fs::write(dir.join(name), wav).expect("write wav");
}

/// Karplus-Strong plucked string with body resonance.
///
/// `freq` sets the delay-line length (fractional-delay tuning via linear
/// interpolation between the two nearest delay taps). `seconds` is the output
/// length. `pluck` in [0,1] is the excitation loudness/noise content (hard
/// pick = more noise, brighter). `damping` in [0,1] is the per-sample
/// feedback attenuation (higher = shorter decay). `body` is the body-resonance
/// mix (a damped resonant sine adds the "box" a bare delay line lacks).
///
/// Deterministic: the excitation is a fixed SplitMix-seeded noise burst, so
/// identical inputs produce identical samples.
#[must_use]
fn plucked_string(freq: f32, seconds: f32, pluck: f32, damping: f32, body: f32) -> Vec<f32> {
    let n = (seconds * RATE as f32) as usize;
    let delay_len = RATE as f32 / freq.max(1.0);
    let int_len = delay_len.floor() as usize;
    let frac = delay_len - int_len as f32;

    // Excitation: noise burst for the first ~2 ms (the "pick"), softer/quieter
    // for gentle plucks. SplitMix64-seeded from the frequency bits.
    let mut state = u64::from(freq.to_bits()) ^ 0x9E37_79B9_7F4A_7C15;
    let burst = ((0.002 * RATE as f32) as usize).max(2);
    let mut buf = vec![0.0f32; n + int_len + 2];
    for sample in buf.iter_mut().take(burst) {
        state = state
            .wrapping_add(0x9E37_79B9_7F4A_7C15)
            .wrapping_mul(0xBF58_476D_1CE4_E5B9);
        *sample = ((state >> 33) as f32) / 1_073_741_824.0 * pluck * 0.8;
    }

    let damping_amt = damping.mul_add(-0.35, 1.0);
    let mut out = Vec::with_capacity(n);
    let mut read = 0usize;
    let mut write = 0usize;
    let mut body_phase = 0.0f32;
    let body_freq = freq * 0.5; // body resonates at half the fundamental
    let mut body_amp = 0.0f32;

    let buf_len = buf.len();
    for i in 0..n {
        let pos = i as f32 / RATE as f32;
        // Delay read with fractional tuning: linear blend of two taps.
        let a = buf[read % buf_len];
        let b = buf[(read + 1) % buf_len];
        let y = a.mul_add(1.0 - frac, b * frac);
        // One-zero low-pass in the feedback loop (the KS string filter).
        let filtered = 0.5 * (y + buf[(read + buf_len - 1) % buf_len]);
        buf[write % buf_len] = filtered * damping_amt;
        read = (read + 1) % buf_len;
        write = (write + 1) % buf_len;

        // Body resonance: damped sine at half the fundamental, excited by the
        // pick transient (attack envelope).
        let body_env = (-pos / (seconds * 0.35)).exp();
        body_phase = (body_phase + std::f32::consts::TAU * body_freq / RATE as f32) % 1.0;
        body_amp = body_amp.mul_add(0.999, body_env * 0.001);
        let body_sample = (body_phase * std::f32::consts::TAU).sin() * body_amp * body;

        out.push(y * 0.9 + body_sample);
    }
    normalize(&mut out);
    out
}
/// Additive e-piano tone: inharmonic partials (the classic struck-tine
/// "Rhodes-ish" 1.0 / 2.0 / 3.0 / 3.9 ratio) with a fast attack and a
/// **constant sustain** (no decay during the body), plus a brief release
/// only in the final 400 ms.
///
/// The sustain segment is what makes looping work: the engine's loop points
/// (`embedded_bank.rs`) replay `[attack_end, release_start]` indefinitely, so
/// a sustained note must not decay inside that window. Partial levels drop
/// slightly across the body for a natural long-tone bloom.
#[must_use]
fn epiano_tone(freq: f32, seconds: f32) -> Vec<f32> {
    let n = (seconds * RATE as f32) as usize;
    let partials = [1.0f32, 2.0, 3.0, 3.9];
    let amps = [1.0f32, 0.42, 0.18, 0.09];
    let release_start = (seconds - 0.4) * RATE as f32;
    let mut out = Vec::with_capacity(n);
    let mut phases = [0.0f32; 4];
    for i in 0..n {
        let t = i as f32 / RATE as f32;
        let pos = i as f32;
        // Fast attack (5 ms) then constant sustain; release fades only in the
        // final 400 ms (the loop cuts this off, so it never sounds).
        let attack = (t / 0.005).min(1.0);
        let release = if pos >= release_start {
            ((pos - release_start) / (0.4 * RATE as f32)).mul_add(-1.0, 1.0)
        } else {
            1.0
        };
        let mut s = 0.0f32;
        for (k, &ratio) in partials.iter().enumerate() {
            phases[k] = (phases[k] + std::f32::consts::TAU * freq * ratio / RATE as f32) % 1.0;
            // Gentle sustain bloom: partial k settles to ~60% by 0.8 s.
            let bloom = (0.4 * (t / 0.8).min(1.0)).mul_add(-(0.5 + 0.5 * k as f32 / 3.0), 1.0);
            let env = attack * release * bloom;
            s = ((phases[k] * std::f32::consts::TAU).sin() * amps[k]).mul_add(env, s);
        }
        // Subtle deterministic tremolo (~5 Hz, 8%) for movement.
        let trem = 0.08f32.mul_add((std::f32::consts::TAU * 5.0 * t).sin(), 1.0);
        out.push(s * trem * 0.22);
    }
    normalize(&mut out);
    out
}

/// Peak-normalize to ~0.85 (headroom for the mix).
fn normalize(buf: &mut [f32]) {
    let peak = buf.iter().map(|s| s.abs()).fold(0.0f32, f32::max);
    if peak > 1e-6 {
        let gain = 0.85 / peak;
        for s in buf.iter_mut() {
            *s *= gain;
        }
    }
}
