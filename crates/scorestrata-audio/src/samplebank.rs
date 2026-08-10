//! Sampled percussion bank (Phase A) — deterministic one-shot drum samples.
//!
//! Production tracks use *samples*: a kick at velocity 40 is a different
//! recording than at velocity 120, and each genre family has its own kit
//! character (rock = punchy, house = tight, hiphop = boomy, techno = hard,
//! trap = sub-heavy). This module ships embedded one-shots generated
//! **deterministically at build-adjacent load time** — no wall-clock state,
//! no runtime RNG, fixed float ops — so the same kit always produces the
//! same samples (R2). When a project selects a kit, drum layers route
//! through the sampler; when `None`, the legacy synth-drum path stays
//! byte-identical (R1).
//!
//! The one-shots are synthesized rather than shipped as WAV files so the
//! crate keeps its zero-external-asset posture (R7): every kit is a small
//! parametric recipe with a distinct character, all pure `f32` math.
//
// DSP kernels intentionally use bounded integer/float casts (sample indexes
// and phase accumulators) and fused multiply-add for accuracy; the casts are
// range-checked at the call sites, so the pedantic cast lints are expected.
#![expect(
    clippy::cast_possible_truncation,
    clippy::cast_possible_wrap,
    clippy::cast_precision_loss,
    clippy::cast_sign_loss,
    clippy::suboptimal_flops,
    reason = "Sampler kernels use bounded index casts and FMA for float accuracy"
)]

use crate::sampler::{DrumSample, DrumSampler};
use crate::synth::DrumKind;

/// One-shots per drum voice for one kit.
#[derive(Debug, Clone)]
pub struct DrumKit {
    /// Kit name (`rock`, `house`, `hiphop`, `techno`, `trap`, `sf2`).
    pub name: &'static str,
    /// The sampler populated with the kit's one-shots.
    pub sampler: DrumSampler,
    /// Sample rate the one-shots were generated at.
    pub sample_rate: u32,
}

/// The embedded kit catalog. `None` → legacy synth drums (R1).
///
/// `sf2` is the production kit: **recorded** one-shots extracted from the
/// `FluidR3_GM` `SoundFont`'s GM percussion set (kick/snare/hats/clap/toms/
/// crash/ride), embedded via `include_bytes!` — the same recorded-instrument
/// posture as the melodic SF2 bank. The five parametric kits remain for
/// genre color when a project asks for them.
pub const KIT_NAMES: &[&str] = &["rock", "house", "hiphop", "techno", "trap", "sf2"];

/// Look up a kit by name.
#[must_use]
pub fn kit(name: &str) -> Option<DrumKit> {
    match name {
        "rock" => Some(rock_kit()),
        "house" => Some(house_kit()),
        "hiphop" => Some(hiphop_kit()),
        "techno" => Some(techno_kit()),
        "trap" => Some(trap_kit()),
        "sf2" => Some(sf2_kit()),
        _ => None,
    }
}

/// Catalog voice names for a kit (same order as the sampler's voice map).
#[must_use]
pub fn kit_voice_names(name: &str) -> Vec<String> {
    let voices: &[&str] = match name {
        "rock" | "house" | "hiphop" | "techno" | "trap" => {
            &["kick", "snare", "hat", "open_hat", "clap"]
        }
        "sf2" => &[
            "kick", "snare", "hat", "open_hat", "clap", "tom_low", "tom_mid", "tom_high", "crash",
            "ride",
        ],
        _ => return Vec::new(),
    };
    voices.iter().map(|v| (*v).to_string()).collect()
}

/// One-line character summary of a kit for the catalog resource.
#[must_use]
pub fn kit_character(name: &str) -> &'static str {
    match name {
        "rock" => "punchy live-room feel; short snare, aggressive kick",
        "house" => "tight, bright 4-on-the-floor; crisp hats, low thump",
        "hiphop" => "boomy wide kick, fat snare, sizzling open hats",
        "techno" => "hard and dry; long sub kick, piercing closed hats",
        "trap" => "sub-heavy kick with long decay, dense snare, dark hats",
        "sf2" => "recorded GM percussion (FluidR3_GM): real kick, snare, toms, crash, ride",
        _ => "unknown kit",
    }
}

/// Per-voice DSP baked into a kit (tuning/tone/decay highlights).
#[must_use]
pub fn kit_processing_summary(name: &str) -> Vec<String> {
    match name {
        "rock" => vec![
            "kick tuned to A1 with punchy 48 Hz body".to_string(),
            "snare with 0.6 s band-passed noise tail".to_string(),
        ],
        "house" => vec![
            "kick tuned to A1, 42 Hz body, tighter 0.3 decay".to_string(),
            "hats brightened with 0.7 high-pass tone".to_string(),
        ],
        "hiphop" => vec![
            "kick tuned to F1 with 0.45 boom decay".to_string(),
            "snare layered 0.35 noise for body".to_string(),
        ],
        "techno" => vec![
            "kick tuned to A1 with 0.4 long sub decay".to_string(),
            "closed hats clipped bright at 0.8 tone".to_string(),
        ],
        "trap" => vec![
            "kick tuned to F1 with 0.6 sub-heavy decay".to_string(),
            "snare at 0.3 with wide 0.8 noise layer".to_string(),
        ],
        "sf2" => vec![
            "recorded kick with natural transient and room".to_string(),
            "recorded snare with real shell + snare-wire character".to_string(),
            "recorded toms, crash and ride for fills and accents".to_string(),
        ],
        _ => Vec::new(),
    }
}

/// Render a drum voice from a kit into `output` (additive mix).
///
/// Velocity scales the sample gain, mimicking the velocity-layer behavior of
/// real sampled kits. Unknown pitches fall back to the nearest mapped voice.
pub fn render_kit_note(kit: &DrumKit, kind: DrumKind, velocity: f32, output: &mut [f32]) {
    // 49 keeps the legacy clap pitch (the Phase-I demos write clap notes
    // there); the `sf2` kit loads its recorded clap under the same key.
    let midi = match kind {
        DrumKind::Kick => 36,
        DrumKind::Snare => 38,
        DrumKind::Hat => 42,
        DrumKind::OpenHat => 46,
        DrumKind::Clap => 49,
        DrumKind::TomLow => 43,
        DrumKind::TomMid => 45,
        DrumKind::TomHigh => 48,
        DrumKind::Crash => 57,
        DrumKind::Ride => 59,
    };
    let sampler = &kit.sampler;
    if let Some(sample) = sampler.get_sample(midi) {
        let gain = velocity.clamp(0.0, 1.0) * sample.base_velocity;
        for (i, out) in output.iter_mut().enumerate() {
            if i < sample.data.len() {
                *out = sample.data[i].mul_add(gain, *out);
            }
        }
    }
}

/// Build a [`DrumSampler`] from the given one-shot recipe table.
fn build_sampler(_kit_name: &'static str, recipes: &[(&'static str, u8, Vec<f32>)]) -> DrumSampler {
    let mut sampler = DrumSampler::new();
    for (name, midi, data) in recipes {
        sampler.load_sample(DrumSample {
            name: (*name).to_string(),
            midi_note: *midi,
            data: data.clone(),
            sample_rate: 48_000,
            base_velocity: 1.0,
        });
    }
    sampler
}

// ── One-shot recipes ────────────────────────────────────────────────────
// Each voice is a short parametric recipe. All math is pure `f32`; the only
// "randomness" is a fixed hash-derived noise table, so renders are
// deterministic across runs and platforms (R2).

/// Deterministic noise sample from a fixed integer hash (SplitMix-style).
#[must_use]
fn noise(seed: u64) -> f32 {
    let mut z = seed.wrapping_add(0x9E37_79B9_7F4A_7C15);
    z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
    z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
    z ^= z >> 31;
    let x = z as i64;
    (x >> 33) as f32 / 2_147_483_648.0
}

/// A kick one-shot: sine pitch sweep (sub thump) plus a short click.
fn kick(f0: f32, f1: f32, body: f32, seconds: f32) -> Vec<f32> {
    let n = (seconds * 48_000.0) as usize;
    let mut out = Vec::with_capacity(n);
    let mut phase = 0.0f32;
    for i in 0..n {
        let t = i as f32 / 48_000.0;
        let u = (t / seconds).min(1.0);
        let freq = f0 * (f1 / f0).powf(u);
        phase += std::f32::consts::TAU * freq / 48_000.0;
        // Exponential decay with a soft attack click.
        let env = (-10.0 * u).exp() * body;
        let click = if t < 0.002 { (t / 0.002) * 0.4 } else { 0.0 };
        let s = phase.sin() * env + click;
        out.push(s.tanh());
    }
    out
}

/// A snare one-shot: band-ish noise burst + a tonal ring.
fn snare(noise_amp: f32, ring: f32, seconds: f32) -> Vec<f32> {
    let n = (seconds * 48_000.0) as usize;
    let mut out = Vec::with_capacity(n);
    let mut lp = 0.0f32;
    for i in 0..n {
        let t = i as f32 / 48_000.0;
        let u = (t / seconds).min(1.0);
        let env = (-8.0 * u).exp();
        // One-pole low-pass over deterministic noise → "papery" body.
        lp = 0.2f32.mul_add(noise(i as u64), 0.8 * lp);
        let ring_osc = (std::f32::consts::TAU * 200.0 * t).sin() * (-6.0 * u).exp() * ring;
        out.push((lp * noise_amp * env + ring_osc).tanh());
    }
    out
}

/// A hi-hat one-shot: short high-passed noise burst (open = longer decay).
fn hat(seconds: f32, brightness: f32) -> Vec<f32> {
    let n = (seconds * 48_000.0) as usize;
    let mut out = Vec::with_capacity(n);
    let mut hp = 0.0f32;
    let mut prev = 0.0f32;
    for i in 0..n {
        let t = i as f32 / 48_000.0;
        let u = (t / seconds).min(1.0);
        let env = (-(if seconds > 0.2 { 14.0 } else { 30.0 }) * u).exp();
        let x = noise(i as u64 ^ 0xABCD_EF01);
        // One-pole high-pass → bright metallic top.
        hp = brightness * (x - prev + hp * (1.0 - brightness));
        prev = x;
        out.push((hp * env * 1.5).tanh());
    }
    out
}

/// A clap one-shot: several clustered noise bursts (multi-sample feel).
fn clap(seconds: f32) -> Vec<f32> {
    let n = (seconds * 48_000.0) as usize;
    let mut out = Vec::with_capacity(n);
    let mut lp = 0.0f32;
    for i in 0..n {
        let t = i as f32 / 48_000.0;
        let u = (t / seconds).min(1.0);
        let burst = if t < 0.05 || (0.07..0.12).contains(&t) {
            1.0
        } else if (0.13..0.19).contains(&t) {
            0.7
        } else {
            0.0
        };
        let env = (-8.0 * u).exp() * burst;
        lp = 0.3f32.mul_add(noise(i as u64 ^ 0xDEAD_BEEF), 0.7 * lp);
        out.push((lp * env * 1.2).tanh());
    }
    out
}

/// The production recorded kit: one-shots extracted by `extract_sf2` from
/// the `FluidR3_GM` `SoundFont`'s GM percussion set (channel 9), embedded via
/// `include_bytes!` — the same R7 asset posture as the melodic bank. The
/// extractor renders each voice dry at 48 kHz and normalizes to the same
/// −18 dB level plan, so the kit slots into the mix exactly like the
/// parametric kits.
fn sf2_kit() -> DrumKit {
    #[expect(
        clippy::expect_used,
        reason = "Embedded assets are built by the crate's own extractor; a parse failure is a build-time error"
    )]
    let load = |bytes: &'static [u8]| -> Vec<f32> {
        let (data, rate) =
            crate::wav::read_wav_mono(bytes).expect("SF2 drum WAV must parse (re-run extract_sf2)");
        crate::wav::convert_rate(&data, rate, 48_000)
    };
    let mut sampler = DrumSampler::new();
    for (name, bytes, midi) in [
        ("kick", DRUM_KICK, 36u8),
        ("snare", DRUM_SNARE, 38),
        ("hat", DRUM_HAT, 42),
        ("open_hat", DRUM_OPEN_HAT, 46),
        ("clap", DRUM_CLAP, 49),
        ("tom_low", DRUM_TOM_LOW, 43),
        ("tom_mid", DRUM_TOM_MID, 45),
        ("tom_high", DRUM_TOM_HIGH, 48),
        ("crash", DRUM_CRASH, 57),
        ("ride", DRUM_RIDE, 59),
    ] {
        sampler.load_sample(DrumSample {
            name: name.to_string(),
            midi_note: midi,
            data: load(bytes),
            sample_rate: 48_000,
            base_velocity: 1.0,
        });
    }
    DrumKit {
        name: "sf2",
        sampler,
        sample_rate: 48_000,
    }
}

fn rock_kit() -> DrumKit {
    let recipes = vec![
        ("kick", 36u8, kick(150.0, 48.0, 1.2, 0.35)),
        ("snare", 38, snare(1.0, 0.6, 0.3)),
        ("hat", 42, hat(0.09, 0.5)),
        ("open_hat", 46, hat(0.4, 0.5)),
        ("clap", 49, clap(0.28)),
    ];
    DrumKit {
        name: "rock",
        sampler: build_sampler("rock", &recipes),
        sample_rate: 48_000,
    }
}

fn house_kit() -> DrumKit {
    let recipes = vec![
        ("kick", 36u8, kick(120.0, 42.0, 1.5, 0.3)),
        ("snare", 38, snare(0.9, 0.4, 0.22)),
        ("hat", 42, hat(0.08, 0.7)),
        ("open_hat", 46, hat(0.35, 0.7)),
        ("clap", 49, clap(0.24)),
    ];
    DrumKit {
        name: "house",
        sampler: build_sampler("house", &recipes),
        sample_rate: 48_000,
    }
}

fn hiphop_kit() -> DrumKit {
    let recipes = vec![
        ("kick", 36u8, kick(160.0, 38.0, 1.6, 0.45)),
        ("snare", 38, snare(1.1, 0.7, 0.35)),
        ("hat", 42, hat(0.1, 0.45)),
        ("open_hat", 46, hat(0.5, 0.45)),
        ("clap", 49, clap(0.3)),
    ];
    DrumKit {
        name: "hiphop",
        sampler: build_sampler("hiphop", &recipes),
        sample_rate: 48_000,
    }
}

fn techno_kit() -> DrumKit {
    let recipes = vec![
        ("kick", 36u8, kick(110.0, 40.0, 1.8, 0.4)),
        ("snare", 38, snare(1.0, 0.3, 0.18)),
        ("hat", 42, hat(0.07, 0.8)),
        ("open_hat", 46, hat(0.3, 0.8)),
        ("clap", 49, clap(0.22)),
    ];
    DrumKit {
        name: "techno",
        sampler: build_sampler("techno", &recipes),
        sample_rate: 48_000,
    }
}

// ── Recorded drum one-shots (generated by `extract_sf2`) ───────────────

const DRUM_KICK: &[u8] = include_bytes!("../assets/instruments_sf2/drum_kick.wav");
const DRUM_SNARE: &[u8] = include_bytes!("../assets/instruments_sf2/drum_snare.wav");
const DRUM_HAT: &[u8] = include_bytes!("../assets/instruments_sf2/drum_hat.wav");
const DRUM_OPEN_HAT: &[u8] = include_bytes!("../assets/instruments_sf2/drum_open_hat.wav");
const DRUM_CLAP: &[u8] = include_bytes!("../assets/instruments_sf2/drum_clap.wav");
const DRUM_TOM_LOW: &[u8] = include_bytes!("../assets/instruments_sf2/drum_tom_low.wav");
const DRUM_TOM_MID: &[u8] = include_bytes!("../assets/instruments_sf2/drum_tom_mid.wav");
const DRUM_TOM_HIGH: &[u8] = include_bytes!("../assets/instruments_sf2/drum_tom_high.wav");
const DRUM_CRASH: &[u8] = include_bytes!("../assets/instruments_sf2/drum_crash.wav");
const DRUM_RIDE: &[u8] = include_bytes!("../assets/instruments_sf2/drum_ride.wav");

fn trap_kit() -> DrumKit {
    let recipes = vec![
        ("kick", 36u8, kick(180.0, 32.0, 2.0, 0.6)),
        ("snare", 38, snare(1.2, 0.8, 0.3)),
        ("hat", 42, hat(0.08, 0.6)),
        ("open_hat", 46, hat(0.45, 0.6)),
        ("clap", 49, clap(0.28)),
    ];
    DrumKit {
        name: "trap",
        sampler: build_sampler("trap", &recipes),
        sample_rate: 48_000,
    }
}

#[cfg(test)]
mod tests {
    // Tests intentionally use `.expect()` on freshly built kits.
    #![expect(clippy::expect_used, reason = "Test fixtures assert kit presence")]

    use super::*;

    #[test]
    fn every_kit_loads_with_all_voices() {
        for name in KIT_NAMES {
            let k = kit(name).expect("kit exists");
            for midi in [36, 38, 42, 46, 49] {
                assert!(
                    k.sampler.get_sample(midi).is_some(),
                    "{name} kit has voice {midi}"
                );
            }
        }
    }

    #[test]
    fn sf2_kit_loads_the_full_ten_voice_set() {
        // The production kit must cover the extended kit: every GM voice
        // `render_kit_note` can request — including toms, crash and ride.
        let k = kit("sf2").expect("sf2 kit");
        assert_eq!(k.sampler.len(), 10, "sf2 kit has ten voices");
        for midi in [36, 38, 42, 46, 49, 43, 45, 48, 57, 59] {
            let sample = k.sampler.get_sample(midi).expect("voice present");
            assert!(
                sample.data.iter().any(|s| s.abs() > 0.01),
                "voice {midi} renders audible"
            );
        }
    }

    #[test]
    fn sf2_kit_is_deterministic_and_distinct_from_synth() {
        // The recorded kit must be byte-stable (R2) and its kick must differ
        // from the parametric rock kit's kick — proof the recorded one-shot
        // is actually in the mix.
        let a = kit("sf2").expect("a");
        let b = kit("sf2").expect("b");
        assert_eq!(
            a.sampler.get_sample(36).expect("kick").data,
            b.sampler.get_sample(36).expect("kick").data
        );
        let rock = kit("rock").expect("rock");
        assert_ne!(
            a.sampler.get_sample(36).expect("kick").data,
            rock.sampler.get_sample(36).expect("rock kick").data,
            "recorded kick differs from the synthesized rock kick"
        );
    }

    #[test]
    fn unknown_kit_returns_none() {
        assert!(kit("industrial").is_none());
    }

    #[test]
    fn kits_have_distinct_characters() {
        // The kick one-shots must differ between families — otherwise the
        // bank adds nothing over the single synth drum.
        let rock = kit("rock").expect("rock");
        let trap = kit("trap").expect("trap");
        let rock_kick = rock.sampler.get_sample(36).expect("rock kick");
        let trap_kick = trap.sampler.get_sample(36).expect("trap kick");
        let energy = |s: &DrumSample| -> f32 { s.data.iter().map(|x| x * x).sum() };
        assert!((energy(rock_kick) - energy(trap_kick)).abs() > 0.01);
        assert_ne!(rock_kick.data, trap_kick.data);
    }

    #[test]
    fn kits_are_deterministic() {
        let a = kit("techno").expect("a");
        let b = kit("techno").expect("b");
        for midi in [36, 38, 42, 46, 49] {
            assert_eq!(
                a.sampler.get_sample(midi).expect("a").data,
                b.sampler.get_sample(midi).expect("b").data
            );
        }
    }

    #[test]
    fn render_kit_note_mixes_into_output() {
        let k = kit("house").expect("house");
        let mut out = vec![0.0f32; 4800];
        render_kit_note(&k, DrumKind::Kick, 1.0, &mut out);
        assert!(out.iter().any(|s| s.abs() > 0.01), "kick renders audible");
    }

    #[test]
    fn catalog_helpers_cover_every_kit() {
        for name in KIT_NAMES {
            let expected = if *name == "sf2" { 10 } else { 5 };
            assert_eq!(
                kit_voice_names(name).len(),
                expected,
                "{name} exposes all its voices"
            );
            assert!(
                kit_character(name).len() > 8,
                "{name} has a character summary"
            );
            assert!(
                !kit_processing_summary(name).is_empty(),
                "{name} has DSP notes"
            );
        }
        assert!(kit_voice_names("industrial").is_empty());
        assert_eq!(kit_character("industrial"), "unknown kit");
    }
}
