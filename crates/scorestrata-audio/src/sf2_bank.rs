//! Production-grade instrument bank (Phase H) — recorded `SoundFont` voices.
//!
//! Loads the one-shot WAVs extracted from a real `SoundFont` by
//! `extract_sf2` (`rustysynth`-rendered `FluidR3_GM` programs: Distortion
//! Guitar, Picked Bass, E.Piano) through [`multisample::InstrumentBank`]
//! with the same SFZ-style zone manifest as the synthetic bank.
//!
//! The WAVs are the *rendered* instrument — the font's own envelopes,
//! filters, and velocity layers are baked in, so the one-shot sounds like
//! the recorded instrument actually played, not a raw sample slice.
//!
//! The bank is embedded via `include_bytes!` (R7) and fully deterministic
//! (R2): identical bytes in → identical samples out. Runtime renders of
//! this bank are byte-exact; the committed one-shot set is the only source
//! the engine sees.
//!
//! Instruments (the same names/zones `render.rs` routes to):
//! - `guitar` — GM 30 Distortion Guitar, 5 key zones × 2 velocity layers
//! - `bass`   — GM 34 Picked Bass, 4 zones × 2 layers
//! - `epiano` — GM 4 E.Piano, 3 zones, sustain loop detected at load

// Loop-point math converts fixed 48 kHz sample counts; assets are
// seconds-scale so truncation is lossless.
#![expect(
    clippy::cast_precision_loss,
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    reason = "Loop-point math converts fixed 48 kHz sample counts; assets are seconds-scale so truncation is lossless"
)]

use crate::multisample::{InstrumentBank, MultiSampleInstrument, Sample, SampleZone};

/// Bank target rate (assets are rendered at 48 kHz).
pub const BANK_RATE: u32 = 48_000;

/// Build the SF2-extracted instrument bank.
///
/// The asset loader is deterministic and infallible: the WAVs ship in the
/// crate and the zone manifest is static, so failures here are programming
/// errors, not runtime conditions.
#[must_use]
#[expect(
    clippy::too_many_lines,
    reason = "Bank assembly registers a dozen instruments as one flat zone manifest"
)]
pub fn sf2_instrument_bank() -> InstrumentBank {
    let mut bank = InstrumentBank::new();

    // Guitar: C2/A2/E3/B3/E4 roots, soft + hard velocity layers. Key ranges
    // tile the full rock guitar register (MIDI 32–72). Three round-robin
    // takes per layer (velocity-stepped renders from `extract_sf2`) so the
    // engine's RR picker cycles real takes on repeated chugs instead of
    // machine-gunning one identical sample.
    for (root, key_min, key_max, t) in [
        (36u8, 32, 40, GUITAR_C2_TAKES),
        (45, 41, 48, GUITAR_A2_TAKES),
        (52, 49, 55, GUITAR_E3_TAKES),
        (59, 56, 62, GUITAR_B3_TAKES),
        (66, 63, 72, GUITAR_E4_TAKES),
    ] {
        push_zones(
            &mut bank,
            "guitar",
            vec![
                SampleZone {
                    key_min,
                    key_max,
                    vel_min: 0.0,
                    vel_max: 0.62,
                    samples: vec![
                        load_zone(root, t[0]),
                        load_zone(root, t[1]),
                        load_zone(root, t[2]),
                    ],
                },
                SampleZone {
                    key_min,
                    key_max,
                    vel_min: 0.62,
                    vel_max: 1.0,
                    samples: vec![
                        load_zone(root, t[3]),
                        load_zone(root, t[4]),
                        load_zone(root, t[5]),
                    ],
                },
            ],
        );
    }

    // Bass: C1/A1/C2/G2 roots, soft + hard layers (MIDI 12–47), two
    // round-robin takes per layer.
    for (root, key_min, key_max, t) in [
        (24u8, 12, 26, BASS_C1_TAKES),
        (29, 27, 32, BASS_A1_TAKES),
        (36, 33, 39, BASS_C2_TAKES),
        (43, 40, 47, BASS_G2_TAKES),
    ] {
        push_zones(
            &mut bank,
            "bass",
            vec![
                SampleZone {
                    key_min,
                    key_max,
                    vel_min: 0.0,
                    vel_max: 0.62,
                    samples: vec![load_zone(root, t[0]), load_zone(root, t[1])],
                },
                SampleZone {
                    key_min,
                    key_max,
                    vel_min: 0.62,
                    vel_max: 1.0,
                    samples: vec![load_zone(root, t[2]), load_zone(root, t[3])],
                },
            ],
        );
    }

    // E-piano: C3/C4/C5 roots, looped sustain. The extractor renders a long
    // held note; the loop detector finds the stable sustain region (after
    // the attack transient, before the release tail) so long notes sustain.
    for (root, key_min, key_max, wav) in [
        (48u8, 36, 54, EPIANO_C3),
        (60, 55, 66, EPIANO_C4),
        (72, 67, 84, EPIANO_C5),
    ] {
        register_sustained(&mut bank, "epiano", &[(root, key_min, key_max, wav)]);
    }

    // Clean / acoustic guitar family (Phase I): 3 key zones × 2 velocity
    // layers × 2 RR takes. Steel-string (25) and clean electric (27) covers
    // funk stabs, verses and arpeggios the distortion guitar can't do.
    register_plucked(
        &mut bank,
        "acoustic_guitar",
        &[
            (48u8, 36, 53, ACOUSTIC_C3),
            (60, 54, 65, ACOUSTIC_C4),
            (72, 66, 84, ACOUSTIC_C5),
        ],
    );
    register_plucked(
        &mut bank,
        "clean_guitar",
        &[
            (48u8, 36, 53, CLEAN_C3),
            (60, 54, 65, CLEAN_C4),
            (72, 66, 84, CLEAN_C5),
        ],
    );

    // Sustained voices (Phase I): loop-detected long one-shots for the
    // orchestral / synth families — strings, organ, brass, flute, saw lead,
    // warm pad.
    register_sustained(
        &mut bank,
        "strings",
        &[
            (48u8, 36, 53, STRINGS_C3),
            (60, 54, 65, STRINGS_C4),
            (72, 66, 84, STRINGS_C5),
        ],
    );
    register_sustained(
        &mut bank,
        "organ",
        &[
            (48u8, 36, 53, ORGAN_C3),
            (60, 54, 65, ORGAN_C4),
            (72, 66, 84, ORGAN_C5),
        ],
    );
    register_sustained(
        &mut bank,
        "brass",
        &[
            (48u8, 36, 53, BRASS_C3),
            (60, 54, 65, BRASS_C4),
            (72, 66, 84, BRASS_C5),
        ],
    );
    register_sustained(
        &mut bank,
        "flute",
        &[
            (60u8, 48, 65, FLUTE_C4),
            (72, 66, 77, FLUTE_C5),
            (84, 78, 96, FLUTE_C6),
        ],
    );
    register_sustained(
        &mut bank,
        "leadsaw",
        &[
            (48u8, 36, 53, LEADSAW_C3),
            (60, 54, 65, LEADSAW_C4),
            (72, 66, 84, LEADSAW_C5),
        ],
    );
    register_sustained(
        &mut bank,
        "warmpad",
        &[
            (36u8, 24, 41, WARMPAD_C2),
            (48, 42, 53, WARMPAD_C3),
            (60, 54, 72, WARMPAD_C4),
        ],
    );

    bank
}

/// Register a plucked instrument: 3 zones, each with 2 velocity layers and
/// 2 round-robin takes per layer (`[soft1, soft2, hard1, hard2]`).
fn register_plucked(
    bank: &mut InstrumentBank,
    name: &str,
    zones: &[(u8, u8, u8, [&'static [u8]; 4])],
) {
    for (root, key_min, key_max, takes) in zones {
        push_zones(
            bank,
            name,
            vec![
                SampleZone {
                    key_min: *key_min,
                    key_max: *key_max,
                    vel_min: 0.0,
                    vel_max: 0.62,
                    samples: vec![load_zone(*root, takes[0]), load_zone(*root, takes[1])],
                },
                SampleZone {
                    key_min: *key_min,
                    key_max: *key_max,
                    vel_min: 0.62,
                    vel_max: 1.0,
                    samples: vec![load_zone(*root, takes[2]), load_zone(*root, takes[3])],
                },
            ],
        );
    }
}

/// Register a sustained instrument: 1–3 zones, each a single long one-shot
/// whose stable sustain region becomes a loop (attack + release excluded).
fn register_sustained(
    bank: &mut InstrumentBank,
    name: &str,
    zones: &[(u8, u8, u8, &'static [u8])],
) {
    for (root, key_min, key_max, wav) in zones {
        let mut sample = load_zone(*root, wav);
        sample.loop_start = Some(detect_loop_start(&sample.data));
        sample.loop_end = Some(detect_loop_end(&sample.data));
        push_zones(
            bank,
            name,
            vec![SampleZone {
                key_min: *key_min,
                key_max: *key_max,
                vel_min: 0.0,
                vel_max: 1.0,
                samples: vec![sample],
            }],
        );
    }
}

/// Append zones to a (possibly already-present) instrument, creating it if
/// needed (mirrors the `multisample` registration pattern).
fn push_zones(bank: &mut InstrumentBank, name: &str, zones: Vec<SampleZone>) {
    if let Some(instrument) = bank.get_mut(name) {
        instrument.zones.extend(zones);
    } else {
        bank.register(MultiSampleInstrument {
            name: name.to_string(),
            zones,
        });
    }
}

/// Decode an embedded WAV one-shot into a [`Sample`].
///
/// `include_bytes!` data is `'static`; the crate's own reader parses it
/// in-memory. Assets are 24-bit PCM mono at 48 kHz — exactly `BANK_RATE` —
/// so no rate conversion is needed.
fn load_zone(root_note: u8, wav: &'static [u8]) -> Sample {
    #[expect(
        clippy::expect_used,
        reason = "Embedded assets are built by the crate's own extractor; a parse failure is a build-time error"
    )]
    let (data, rate) = crate::wav::read_wav_mono(wav).expect("SF2 bank WAV must parse");
    let data = crate::wav::convert_rate(&data, rate, BANK_RATE);
    Sample {
        root_note,
        data,
        sample_rate: BANK_RATE,
        loop_start: None,
        loop_end: None,
    }
}

/// Detect the sustain-loop start: the first frame where the short-window
/// envelope reaches ~90% of its peak and stays there (attack transient
/// done). Windowed RMS over 20 ms; deterministic — pure function of `data`.
fn detect_loop_start(data: &[f32]) -> usize {
    let win = (0.02 * BANK_RATE as f32) as usize;
    if data.len() < win * 2 {
        return 0;
    }
    let peak = windowed_rms_peak(data, win);
    if peak <= 1e-6 {
        return 0;
    }
    let mut stable_from = 0usize;
    for (i, rms) in windowed_rms(data, win).enumerate() {
        if rms >= peak * 0.9 {
            stable_from = i * win;
            break;
        }
    }
    stable_from.min(data.len().saturating_sub(win * 2))
}

/// Detect the sustain-loop end: the first window after the loop start whose
/// RMS drops below ~85% of the sustain level (the release tail begins).
fn detect_loop_end(data: &[f32]) -> usize {
    let win = (0.02 * BANK_RATE as f32) as usize;
    let start = detect_loop_start(data);
    if data.len() < win * 2 {
        return data.len();
    }
    let sustain_rms = windowed_rms(data, win)
        .skip(start / win)
        .take(20)
        .fold(0.0f32, f32::max);
    let mut end = data.len();
    for (i, rms) in windowed_rms(data, win).enumerate().skip(start / win) {
        if rms < sustain_rms * 0.85 {
            end = i * win;
            break;
        }
    }
    // Clamp keeps the loop window sane: never shrink below one window past
    // the start, never run past the buffer. The guard above guarantees the
    // clamp range is non-empty (min <= max).
    end.clamp(start + win, data.len().saturating_sub(win))
}

/// Sliding-window RMS over the buffer (windowed at `win` samples).
fn windowed_rms(data: &[f32], win: usize) -> impl Iterator<Item = f32> + '_ {
    let win = win.max(1);
    data.chunks(win).map(|chunk| {
        let sum: f32 = chunk.iter().map(|s| s * s).sum();
        (sum / chunk.len() as f32).sqrt()
    })
}

/// Peak of the windowed RMS envelope.
fn windowed_rms_peak(data: &[f32], win: usize) -> f32 {
    windowed_rms(data, win).fold(0.0f32, |acc, r| if r > acc { r } else { acc })
}

// ── Embedded assets (generated by `extract_sf2`) ───────────────────────
// Take 1 keeps the bare legacy name (`guitar_45_soft.wav`); takes 2/3 are
// suffixed `_2`/`_3`. Arrays index [soft1, soft2, soft3, hard1, hard2, hard3].

const GUITAR_C2_TAKES: [&[u8]; 6] = [
    include_bytes!("../assets/instruments_sf2/guitar_36_soft.wav"),
    include_bytes!("../assets/instruments_sf2/guitar_36_soft_2.wav"),
    include_bytes!("../assets/instruments_sf2/guitar_36_soft_3.wav"),
    include_bytes!("../assets/instruments_sf2/guitar_36_hard.wav"),
    include_bytes!("../assets/instruments_sf2/guitar_36_hard_2.wav"),
    include_bytes!("../assets/instruments_sf2/guitar_36_hard_3.wav"),
];
const GUITAR_A2_TAKES: [&[u8]; 6] = [
    include_bytes!("../assets/instruments_sf2/guitar_45_soft.wav"),
    include_bytes!("../assets/instruments_sf2/guitar_45_soft_2.wav"),
    include_bytes!("../assets/instruments_sf2/guitar_45_soft_3.wav"),
    include_bytes!("../assets/instruments_sf2/guitar_45_hard.wav"),
    include_bytes!("../assets/instruments_sf2/guitar_45_hard_2.wav"),
    include_bytes!("../assets/instruments_sf2/guitar_45_hard_3.wav"),
];
const GUITAR_E3_TAKES: [&[u8]; 6] = [
    include_bytes!("../assets/instruments_sf2/guitar_52_soft.wav"),
    include_bytes!("../assets/instruments_sf2/guitar_52_soft_2.wav"),
    include_bytes!("../assets/instruments_sf2/guitar_52_soft_3.wav"),
    include_bytes!("../assets/instruments_sf2/guitar_52_hard.wav"),
    include_bytes!("../assets/instruments_sf2/guitar_52_hard_2.wav"),
    include_bytes!("../assets/instruments_sf2/guitar_52_hard_3.wav"),
];
const GUITAR_B3_TAKES: [&[u8]; 6] = [
    include_bytes!("../assets/instruments_sf2/guitar_59_soft.wav"),
    include_bytes!("../assets/instruments_sf2/guitar_59_soft_2.wav"),
    include_bytes!("../assets/instruments_sf2/guitar_59_soft_3.wav"),
    include_bytes!("../assets/instruments_sf2/guitar_59_hard.wav"),
    include_bytes!("../assets/instruments_sf2/guitar_59_hard_2.wav"),
    include_bytes!("../assets/instruments_sf2/guitar_59_hard_3.wav"),
];
const GUITAR_E4_TAKES: [&[u8]; 6] = [
    include_bytes!("../assets/instruments_sf2/guitar_66_soft.wav"),
    include_bytes!("../assets/instruments_sf2/guitar_66_soft_2.wav"),
    include_bytes!("../assets/instruments_sf2/guitar_66_soft_3.wav"),
    include_bytes!("../assets/instruments_sf2/guitar_66_hard.wav"),
    include_bytes!("../assets/instruments_sf2/guitar_66_hard_2.wav"),
    include_bytes!("../assets/instruments_sf2/guitar_66_hard_3.wav"),
];

const BASS_C1_TAKES: [&[u8]; 4] = [
    include_bytes!("../assets/instruments_sf2/bass_24_soft.wav"),
    include_bytes!("../assets/instruments_sf2/bass_24_soft_2.wav"),
    include_bytes!("../assets/instruments_sf2/bass_24_hard.wav"),
    include_bytes!("../assets/instruments_sf2/bass_24_hard_2.wav"),
];
const BASS_A1_TAKES: [&[u8]; 4] = [
    include_bytes!("../assets/instruments_sf2/bass_29_soft.wav"),
    include_bytes!("../assets/instruments_sf2/bass_29_soft_2.wav"),
    include_bytes!("../assets/instruments_sf2/bass_29_hard.wav"),
    include_bytes!("../assets/instruments_sf2/bass_29_hard_2.wav"),
];
const BASS_C2_TAKES: [&[u8]; 4] = [
    include_bytes!("../assets/instruments_sf2/bass_36_soft.wav"),
    include_bytes!("../assets/instruments_sf2/bass_36_soft_2.wav"),
    include_bytes!("../assets/instruments_sf2/bass_36_hard.wav"),
    include_bytes!("../assets/instruments_sf2/bass_36_hard_2.wav"),
];
const BASS_G2_TAKES: [&[u8]; 4] = [
    include_bytes!("../assets/instruments_sf2/bass_43_soft.wav"),
    include_bytes!("../assets/instruments_sf2/bass_43_soft_2.wav"),
    include_bytes!("../assets/instruments_sf2/bass_43_hard.wav"),
    include_bytes!("../assets/instruments_sf2/bass_43_hard_2.wav"),
];

const EPIANO_C3: &[u8] = include_bytes!("../assets/instruments_sf2/epiano_48_sustain.wav");
const EPIANO_C4: &[u8] = include_bytes!("../assets/instruments_sf2/epiano_60_sustain.wav");
const EPIANO_C5: &[u8] = include_bytes!("../assets/instruments_sf2/epiano_72_sustain.wav");

// Plucked clean family (take 1 keeps the unsuffixed name). Index order per
// zone: [soft1, soft2, hard1, hard2].
const ACOUSTIC_C3: [&[u8]; 4] = [
    include_bytes!("../assets/instruments_sf2/acoustic_48_soft1.wav"),
    include_bytes!("../assets/instruments_sf2/acoustic_48_soft2.wav"),
    include_bytes!("../assets/instruments_sf2/acoustic_48_hard1.wav"),
    include_bytes!("../assets/instruments_sf2/acoustic_48_hard2.wav"),
];
const ACOUSTIC_C4: [&[u8]; 4] = [
    include_bytes!("../assets/instruments_sf2/acoustic_60_soft1.wav"),
    include_bytes!("../assets/instruments_sf2/acoustic_60_soft2.wav"),
    include_bytes!("../assets/instruments_sf2/acoustic_60_hard1.wav"),
    include_bytes!("../assets/instruments_sf2/acoustic_60_hard2.wav"),
];
const ACOUSTIC_C5: [&[u8]; 4] = [
    include_bytes!("../assets/instruments_sf2/acoustic_72_soft1.wav"),
    include_bytes!("../assets/instruments_sf2/acoustic_72_soft2.wav"),
    include_bytes!("../assets/instruments_sf2/acoustic_72_hard1.wav"),
    include_bytes!("../assets/instruments_sf2/acoustic_72_hard2.wav"),
];
const CLEAN_C3: [&[u8]; 4] = [
    include_bytes!("../assets/instruments_sf2/clean_48_soft1.wav"),
    include_bytes!("../assets/instruments_sf2/clean_48_soft2.wav"),
    include_bytes!("../assets/instruments_sf2/clean_48_hard1.wav"),
    include_bytes!("../assets/instruments_sf2/clean_48_hard2.wav"),
];
const CLEAN_C4: [&[u8]; 4] = [
    include_bytes!("../assets/instruments_sf2/clean_60_soft1.wav"),
    include_bytes!("../assets/instruments_sf2/clean_60_soft2.wav"),
    include_bytes!("../assets/instruments_sf2/clean_60_hard1.wav"),
    include_bytes!("../assets/instruments_sf2/clean_60_hard2.wav"),
];
const CLEAN_C5: [&[u8]; 4] = [
    include_bytes!("../assets/instruments_sf2/clean_72_soft1.wav"),
    include_bytes!("../assets/instruments_sf2/clean_72_soft2.wav"),
    include_bytes!("../assets/instruments_sf2/clean_72_hard1.wav"),
    include_bytes!("../assets/instruments_sf2/clean_72_hard2.wav"),
];

// Sustained voices — one long render per zone, loop-detected at load.
const STRINGS_C3: &[u8] = include_bytes!("../assets/instruments_sf2/strings_48_sustain.wav");
const STRINGS_C4: &[u8] = include_bytes!("../assets/instruments_sf2/strings_60_sustain.wav");
const STRINGS_C5: &[u8] = include_bytes!("../assets/instruments_sf2/strings_72_sustain.wav");
const ORGAN_C3: &[u8] = include_bytes!("../assets/instruments_sf2/organ_48_sustain.wav");
const ORGAN_C4: &[u8] = include_bytes!("../assets/instruments_sf2/organ_60_sustain.wav");
const ORGAN_C5: &[u8] = include_bytes!("../assets/instruments_sf2/organ_72_sustain.wav");
const BRASS_C3: &[u8] = include_bytes!("../assets/instruments_sf2/brass_48_sustain.wav");
const BRASS_C4: &[u8] = include_bytes!("../assets/instruments_sf2/brass_60_sustain.wav");
const BRASS_C5: &[u8] = include_bytes!("../assets/instruments_sf2/brass_72_sustain.wav");
const FLUTE_C4: &[u8] = include_bytes!("../assets/instruments_sf2/flute_60_sustain.wav");
const FLUTE_C5: &[u8] = include_bytes!("../assets/instruments_sf2/flute_72_sustain.wav");
const FLUTE_C6: &[u8] = include_bytes!("../assets/instruments_sf2/flute_84_sustain.wav");
const LEADSAW_C3: &[u8] = include_bytes!("../assets/instruments_sf2/leadsaw_48_sustain.wav");
const LEADSAW_C4: &[u8] = include_bytes!("../assets/instruments_sf2/leadsaw_60_sustain.wav");
const LEADSAW_C5: &[u8] = include_bytes!("../assets/instruments_sf2/leadsaw_72_sustain.wav");
const WARMPAD_C2: &[u8] = include_bytes!("../assets/instruments_sf2/warmpad_36_sustain.wav");
const WARMPAD_C3: &[u8] = include_bytes!("../assets/instruments_sf2/warmpad_48_sustain.wav");
const WARMPAD_C4: &[u8] = include_bytes!("../assets/instruments_sf2/warmpad_60_sustain.wav");

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sf2_bank_loads_all_instruments() {
        let bank = sf2_instrument_bank();
        let names = bank.names();
        assert!(names.contains(&"guitar".to_string()));
        assert!(names.contains(&"bass".to_string()));
        assert!(names.contains(&"epiano".to_string()));
        #[expect(clippy::expect_used, reason = "Test assertions")]
        let guitar = bank.get("guitar").expect("guitar");
        #[expect(clippy::expect_used, reason = "Test assertions")]
        let bass = bank.get("bass").expect("bass");
        #[expect(clippy::expect_used, reason = "Test assertions")]
        let epiano = bank.get("epiano").expect("epiano");
        assert_eq!(guitar.zones.len(), 10);
        assert_eq!(bass.zones.len(), 8);
        assert_eq!(epiano.zones.len(), 3);
        for name in [
            "acoustic_guitar",
            "clean_guitar",
            "strings",
            "organ",
            "brass",
            "flute",
            "leadsaw",
            "warmpad",
        ] {
            #[expect(clippy::expect_used, reason = "Test assertions")]
            let inst = bank.get(name).expect(name);
            assert!(!inst.zones.is_empty(), "{name} has zones");
        }
        assert!(bank.total_frames() > 0, "bank carries sample frames");
    }

    #[test]
    fn new_instruments_are_audible_and_loop() {
        // The Phase-I families must render real sound (not silence) and the
        // sustained ones must hold through a long note via their loops.
        let bank = sf2_instrument_bank();
        let n = BANK_RATE as usize * 3;
        for (name, pitch) in [
            ("acoustic_guitar", 60),
            ("clean_guitar", 60),
            ("strings", 60),
            ("organ", 60),
            ("brass", 60),
            ("flute", 72),
            ("leadsaw", 60),
            ("warmpad", 48),
        ] {
            let mut out = vec![0.0f32; n];
            bank.render_note(name, pitch, 0.8, 7, &mut out);
            let peak = out.iter().map(|s| s.abs()).fold(0.0f32, f32::max);
            assert!(peak > 0.02, "{name} renders audible (peak {peak})");
            assert!(peak <= 1.0, "{name} does not clip (peak {peak})");
            // Sustained families must still be sounding at the end of a
            // 3-second note (loop engaged); plucks decay naturally.
            let sustained = matches!(
                name,
                "strings" | "organ" | "brass" | "flute" | "leadsaw" | "warmpad"
            );
            let tail_peak = out[out.len() - 200..]
                .iter()
                .map(|s| s.abs())
                .fold(0.0f32, f32::max);
            if sustained {
                assert!(
                    tail_peak > 0.01,
                    "{name} sustains through a 3 s note (tail peak {tail_peak})"
                );
            }
        }
    }

    #[test]
    fn sf2_bank_is_deterministic() {
        let a = sf2_instrument_bank();
        let b = sf2_instrument_bank();
        assert_eq!(a.names(), b.names());
        #[expect(clippy::expect_used, reason = "Test assertions")]
        let guitar = a.get("guitar").expect("guitar");
        #[expect(clippy::expect_used, reason = "Test assertions")]
        let guitar_b = b.get("guitar").expect("guitar");
        for (zone, zone_b) in guitar.zones.iter().zip(&guitar_b.zones) {
            for (sample, sample_b) in zone.samples.iter().zip(&zone_b.samples) {
                assert_eq!(sample.data, sample_b.data, "deterministic bytes (R2)");
            }
        }
    }

    #[test]
    fn sf2_bank_covers_the_planner_registers() {
        // Same coverage contract as the synthetic bank: bass MIDI 12–36,
        // guitar 36–59 must all land in a zone or layers go silent.
        let bank = sf2_instrument_bank();
        #[expect(clippy::expect_used, reason = "Test assertions")]
        let guitar = bank.get("guitar").expect("guitar");
        #[expect(clippy::expect_used, reason = "Test assertions")]
        let bass = bank.get("bass").expect("bass");
        for pitch in 12..=36 {
            assert!(
                bass.zone_for(pitch, 0.8).is_some(),
                "bass covers MIDI {pitch}"
            );
        }
        for pitch in 36..=59 {
            assert!(
                guitar.zone_for(pitch, 0.8).is_some(),
                "guitar covers MIDI {pitch}"
            );
        }
    }

    #[test]
    fn new_plucked_instruments_alternate_round_robin() {
        // Two identical hits must cycle the RR takes (different bytes), and
        // the same seed must stay byte-identical (R2) — same contract as the
        // distortion guitar.
        let bank = sf2_instrument_bank();
        let mut a = vec![0.0f32; 4000];
        let mut b = vec![0.0f32; 4000];
        bank.render_note("acoustic_guitar", 60, 0.8, 0, &mut a);
        bank.render_note("acoustic_guitar", 60, 0.8, 0, &mut b);
        assert_eq!(a, b, "same RR seed stays deterministic (R2)");
        let mut c = vec![0.0f32; 4000];
        bank.render_note("acoustic_guitar", 60, 0.8, 1, &mut c);
        assert_ne!(a, c, "different RR seed cycles the alternate take");
    }

    #[test]
    fn sf2_guitar_differs_from_synthetic_guitar() {
        // The whole point of Phase H: the SF2 bank is NOT the synthetic
        // voice. Same note, same buffer length — different bytes proves the
        // recorded instrument is actually in the mix.
        let sf2 = sf2_instrument_bank();
        let n = BANK_RATE as usize / 2;
        let mut a = vec![0.0; n];
        let mut b = vec![0.0; n];
        sf2.render_note("guitar", 57, 0.8, 7, &mut a);
        crate::synth::render_note(
            &crate::render::voice_params_for_instrument("guitar"),
            crate::render::midi_to_frequency(57),
            0.8,
            BANK_RATE as f32,
            n,
            &mut b,
        );
        assert_ne!(a, b, "SF2 guitar must differ from the synthetic pluck");
        let peak = a.iter().map(|s| s.abs()).fold(0.0f32, f32::max);
        assert!(peak > 0.02, "SF2 guitar produces sound (peak {peak})");
        assert!(peak <= 1.0, "no clipping (peak {peak})");
    }

    #[test]
    fn sf2_epiano_loops_for_a_full_note() {
        let bank = sf2_instrument_bank();
        // Note outlives the extracted sustain sample (which is ~6 s + tail);
        // render 8 s so the loop must engage after the source is exhausted.
        let n = (BANK_RATE as f32 * 8.0) as usize;
        let mut out = vec![0.0; n];
        bank.render_note("epiano", 60, 0.7, 3, &mut out);
        let tail = &out[out.len() - 200..];
        let tail_peak = tail.iter().map(|s| s.abs()).fold(0.0f32, f32::max);
        assert!(
            tail_peak > 0.01,
            "epiano sustains through an 8 s note (tail peak {tail_peak})"
        );
    }

    #[test]
    fn loop_detector_handles_short_and_silent_buffers() {
        // Guard regression: buffers under two windows (or silent) must not
        // panic the clamp in `detect_loop_end` (min > max).
        let short = vec![0.1f32; 100];
        let (start, end) = (detect_loop_start(&short), detect_loop_end(&short));
        assert!(end >= start, "short buffer: end {end} >= start {start}");
        assert!(end <= short.len(), "short buffer end in range");

        let silent = vec![0.0f32; BANK_RATE as usize * 2];
        assert_eq!(detect_loop_start(&silent), 0, "silent buffer start");
        let end = detect_loop_end(&silent);
        assert!(end <= silent.len(), "silent buffer end in range: {end}");
    }

    #[test]
    fn loop_detector_finds_a_sane_sustain_region() {
        // Synthetic constant tone with a 50 ms attack ramp and a release
        // tail: the detector must land inside the flat body.
        let rate = BANK_RATE as usize;
        let mut data = vec![0.0f32; rate * 4];
        for (i, s) in data.iter_mut().enumerate() {
            let t = i as f32 / rate as f32;
            let attack = (t / 0.05).clamp(0.0, 1.0);
            let release = if t > 3.0 {
                (4.0 - t).clamp(0.0, 1.0)
            } else {
                1.0
            };
            *s = (std::f32::consts::TAU * 440.0 * t).sin() * 0.5 * attack * release;
        }
        let start = detect_loop_start(&data);
        let end = detect_loop_end(&data);
        // The windowed RMS sees mostly-sustain samples ~40 ms in (the 20 ms
        // window crosses the tail of the 50 ms attack), so require only that
        // the loop lands inside the body, not in the first 20 ms.
        assert!(start >= rate / 50, "loop starts after the attack: {start}");
        assert!(end > start, "loop end after start: {end} > {start}");
        // The 20 ms window granularity fires ~0.16 s into the 1 s release
        // ramp (RMS crosses 85%% there), so allow the window width +
        // ramp-crossing slack — still clearly before the 4 s sample end.
        assert!(
            end <= rate * 3 + rate / 5,
            "loop ends before the release: {end}"
        );
    }
}
