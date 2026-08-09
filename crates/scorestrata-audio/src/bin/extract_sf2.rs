//! Extract production-grade instrument one-shots from a `SoundFont` (SF2).
//!
//! # Why this exists
//!
//! The Phase G bank (`gen_instrument_bank`) is *synthetic* — `Karplus-Strong`
//! plucks and additive tones. It is deterministic and license-free, but it
//! does not sound like a recorded instrument (audit `voice-sample-audit.md`:
//! "no layer is like rock"). `SoundFont`s *are* recorded multi-sampled
//! instruments — key zones, velocity layers, sustain loops — which is
//! exactly the model our `multisample.rs` engine already plays.
//!
//! This binary **renders** the `SoundFont` through
//! [rustysynth](https://crates.io/crates/rustysynth) (MIT, pure Rust) into
//! dry 48 kHz mono one-shot WAVs — *not* the raw samples. Rendering bakes in
//! the font's own envelopes, filters, and velocity layers, so the extracted
//! one-shot is what the instrument would actually sound like played at that
//! key/velocity. The output feeds the same zone manifest as the synthetic
//! bank, so the engine, FX chain, and determinism contract are unchanged.
//!
//! # Determinism (R2)
//!
//! `rustysynth`'s DSP is deterministic (no hidden randomness), and this tool
//! fixes every render parameter (program, key, velocity, duration, sample
//! rate), so identical `SF2` bytes produce identical WAV bytes. The extracted
//! bank is committed to the repo; runtime renders of it stay byte-exact.
//!
//! # Asset posture (R7)
//!
//! Only the *rendered one-shots* are committed (~19 small WAVs). The 100 MB
//! `SoundFont` itself is **not** embedded — it is a build-time input fetched
//! separately (`FluidR3_GM` is MIT-licensed; see `voice-sample-audit.md` §3).
//!
//! # Usage
//!
//! ```sh
//! cargo run -p scorestrata-audio --bin extract_sf2 -- /path/to/FluidR3_GM.sf2
//! ```
//!
//! Writes `assets/instruments_sf2/*.wav` (same names/zones as the synthetic
//! bank so the two banks are drop-in interchangeable).
//!
//! The `FluidR3_GM.sf2` file is expected at the path given as the first
//! argument (default `/tmp/FluidR3_GM.sf2`).

// Build-time DSP tool: bounded casts, and expect/panic surface tool bugs
// loudly (the generator is the only writer of these files).
#![expect(
    clippy::cast_possible_truncation,
    clippy::cast_precision_loss,
    clippy::cast_sign_loss,
    clippy::expect_used,
    reason = "Build-time asset extractor: fixed 48 kHz rates, bounded casts, and panics surface tool bugs loudly"
)]

use std::path::PathBuf;
use std::sync::Arc;

use rustysynth::{SoundFont, Synthesizer, SynthesizerSettings};

const RATE: u32 = 48_000;
const REVERB_OFF: bool = false;

/// GM program numbers to extract, per bank instrument.
/// - guitar → 30 (Distortion Guitar) — the rock chug.
/// - bass   → 34 (Electric Bass, picked) — punchy rock bass.
/// - epiano → 4  (E.Piano 1) — classic Rhodes-ish.
const GUITAR_PROGRAM: i32 = 30;
const BASS_PROGRAM: i32 = 34;
const EPIANO_PROGRAM: i32 = 4;

fn main() {
    let sf2_path = std::env::args()
        .nth(1)
        .unwrap_or_else(|| "/tmp/FluidR3_GM.sf2".to_string());
    let out_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("assets/instruments_sf2");
    std::fs::create_dir_all(&out_dir).expect("create sf2 assets dir");

    let mut file =
        std::fs::File::open(&sf2_path).unwrap_or_else(|e| panic!("open {sf2_path}: {e}"));
    let font = Arc::new(SoundFont::new(&mut file).expect("parse SoundFont"));

    // ── Guitar: GM 30, roots C2/A2/E3/B3/E4, soft + hard velocity layers.
    let guitar_roots = [36u8, 45, 52, 59, 66];
    for root in guitar_roots {
        render_note(
            &font,
            GUITAR_PROGRAM,
            root,
            45,
            1.7,
            &out_dir,
            &format!("guitar_{root}_soft.wav"),
        );
        render_note(
            &font,
            GUITAR_PROGRAM,
            root,
            110,
            1.7,
            &out_dir,
            &format!("guitar_{root}_hard.wav"),
        );
    }

    // ── Bass: GM 34, roots C1/A1/C2/G2, soft + hard.
    let bass_roots = [24u8, 29, 36, 43];
    for root in bass_roots {
        render_note(
            &font,
            BASS_PROGRAM,
            root,
            45,
            2.4,
            &out_dir,
            &format!("bass_{root}_soft.wav"),
        );
        render_note(
            &font,
            BASS_PROGRAM,
            root,
            110,
            2.4,
            &out_dir,
            &format!("bass_{root}_hard.wav"),
        );
    }

    // ── E-piano: GM 4, roots C3/C4/C5, single layer, long sustain for the
    // loop-detector in `sf2_bank.rs` (the engine loops the stable region).
    let epiano_roots = [48u8, 60, 72];
    for root in epiano_roots {
        render_note(
            &font,
            EPIANO_PROGRAM,
            root,
            90,
            6.0,
            &out_dir,
            &format!("epiano_{root}_sustain.wav"),
        );
    }

    println!("Extracted SF2 instrument bank -> {}", out_dir.display());
}

/// Render one note through rustysynth and write it as a mono 24-bit WAV.
///
/// The synth renders stereo (pan centered, reverb/chorus disabled for a dry
/// one-shot — the engine's FX chain adds space). The mono fold is taken as
/// `(L+R)/2`, then DC-blocked and RMS-normalized to the same bank-wide level
/// plan as the synthetic bank (`finalize`): −18 dB for hard/sustain layers,
/// −24 dB for soft layers, so velocity response and inter-family levels stay
/// consistent whichever bank is used.
fn render_note(
    font: &Arc<SoundFont>,
    program: i32,
    key: u8,
    velocity: i32,
    seconds: f32,
    out_dir: &std::path::Path,
    file_name: &str,
) {
    let mut settings = SynthesizerSettings::new(RATE.cast_signed());
    settings.enable_reverb_and_chorus = REVERB_OFF;
    let mut synth = Synthesizer::new(font, &settings).expect("create synthesizer");
    synth.set_master_volume(1.0);
    // GM program change on channel 0.
    synth.process_midi_message(0, 0xC0, program, 0);
    synth.note_on(0, i32::from(key), velocity);

    let n = (seconds * RATE as f32) as usize;
    let mut left = vec![0.0f32; n];
    let mut right = vec![0.0f32; n];
    synth.render(&mut left, &mut right);
    synth.note_off(0, i32::from(key));
    // Small release tail so the one-shot decays naturally (plucks ring out).
    let tail = (0.35 * RATE as f32) as usize;
    let mut tl = vec![0.0f32; tail];
    let mut tr = vec![0.0f32; tail];
    synth.render(&mut tl, &mut tr);

    let mut mono: Vec<f32> = left
        .iter()
        .zip(&right)
        .map(|(l, r)| (l + r) * 0.5)
        .collect();
    mono.extend(tl.iter().zip(&tr).map(|(l, r)| (l + r) * 0.5));

    // Velocity layers: soft renders target −24 dB, hard/sustain −18 dB
    // (same level plan as the synthetic bank, audit F1/F2).
    let target_db = if file_name.contains("_soft.") {
        -24.0
    } else {
        -18.0
    };
    let mono = finalize(mono, target_db);

    write_wav_file(out_dir, file_name, &mono);
}

/// Finalize a one-shot: DC-block then RMS-normalize to `target_db`
/// (mirrors the synthetic bank's level plan so both banks are
/// interchangeable in the mix).
fn finalize(mut buf: Vec<f32>, target_db: f32) -> Vec<f32> {
    dc_block(&mut buf);
    let rms = (buf.iter().map(|s| s * s).sum::<f32>() / buf.len() as f32)
        .sqrt()
        .max(1e-9);
    let gain = 10f32.powf(target_db / 20.0) / rms;
    for s in &mut buf {
        *s = (*s * gain).clamp(-0.92, 0.92);
    }
    buf
}

/// One-pole high-pass DC blocker (~20 Hz).
fn dc_block(buf: &mut [f32]) {
    let rc = 1.0 / (std::f32::consts::TAU * 20.0);
    let alpha = rc / (rc + 1.0 / RATE as f32);
    let mut prev_in = 0.0f32;
    let mut prev_out = 0.0f32;
    for s in buf.iter_mut() {
        let out = alpha * (prev_out + *s - prev_in);
        prev_in = *s;
        prev_out = out;
        *s = out;
    }
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
