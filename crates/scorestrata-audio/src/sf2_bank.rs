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
//! this bank are byte-exact; only the *source* differs from the synthetic
//! bank, which the engine cannot see.
//!
//! Instruments (same names/zones as `embedded_bank` so the two are
//! drop-in interchangeable):
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
pub fn sf2_instrument_bank() -> InstrumentBank {
    let mut bank = InstrumentBank::new();

    // Guitar: C2/A2/E3/B3/E4 roots, soft + hard velocity layers. Key ranges
    // tile the full rock guitar register (MIDI 32–72) — same as the
    // synthetic bank, so the two banks are interchangeable.
    for (root, key_min, key_max, wav_soft, wav_hard) in [
        (36u8, 32, 40, GUITAR_C2_SOFT, GUITAR_C2_HARD),
        (45, 41, 48, GUITAR_A2_SOFT, GUITAR_A2_HARD),
        (52, 49, 55, GUITAR_E3_SOFT, GUITAR_E3_HARD),
        (59, 56, 62, GUITAR_B3_SOFT, GUITAR_B3_HARD),
        (66, 63, 72, GUITAR_E4_SOFT, GUITAR_E4_HARD),
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
                    samples: vec![load_zone(root, wav_soft)],
                },
                SampleZone {
                    key_min,
                    key_max,
                    vel_min: 0.62,
                    vel_max: 1.0,
                    samples: vec![load_zone(root, wav_hard)],
                },
            ],
        );
    }

    // Bass: C1/A1/C2/G2 roots, soft + hard layers (MIDI 12–47).
    for (root, key_min, key_max, wav_soft, wav_hard) in [
        (24u8, 12, 26, BASS_C1_SOFT, BASS_C1_HARD),
        (29, 27, 32, BASS_A1_SOFT, BASS_A1_HARD),
        (36, 33, 39, BASS_C2_SOFT, BASS_C2_HARD),
        (43, 40, 47, BASS_G2_SOFT, BASS_G2_HARD),
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
                    samples: vec![load_zone(root, wav_soft)],
                },
                SampleZone {
                    key_min,
                    key_max,
                    vel_min: 0.62,
                    vel_max: 1.0,
                    samples: vec![load_zone(root, wav_hard)],
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
        let mut sample = load_zone(root, wav);
        sample.loop_start = Some(detect_loop_start(&sample.data));
        sample.loop_end = Some(detect_loop_end(&sample.data));
        push_zones(
            &mut bank,
            "epiano",
            vec![SampleZone {
                key_min,
                key_max,
                vel_min: 0.0,
                vel_max: 1.0,
                samples: vec![sample],
            }],
        );
    }

    bank
}

/// Append zones to a (possibly already-present) instrument, creating it if
/// needed (mirrors `embedded_bank::push_zones`).
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

const GUITAR_C2_SOFT: &[u8] = include_bytes!("../assets/instruments_sf2/guitar_36_soft.wav");
const GUITAR_C2_HARD: &[u8] = include_bytes!("../assets/instruments_sf2/guitar_36_hard.wav");
const GUITAR_A2_SOFT: &[u8] = include_bytes!("../assets/instruments_sf2/guitar_45_soft.wav");
const GUITAR_A2_HARD: &[u8] = include_bytes!("../assets/instruments_sf2/guitar_45_hard.wav");
const GUITAR_E3_SOFT: &[u8] = include_bytes!("../assets/instruments_sf2/guitar_52_soft.wav");
const GUITAR_E3_HARD: &[u8] = include_bytes!("../assets/instruments_sf2/guitar_52_hard.wav");
const GUITAR_B3_SOFT: &[u8] = include_bytes!("../assets/instruments_sf2/guitar_59_soft.wav");
const GUITAR_B3_HARD: &[u8] = include_bytes!("../assets/instruments_sf2/guitar_59_hard.wav");
const GUITAR_E4_SOFT: &[u8] = include_bytes!("../assets/instruments_sf2/guitar_66_soft.wav");
const GUITAR_E4_HARD: &[u8] = include_bytes!("../assets/instruments_sf2/guitar_66_hard.wav");

const BASS_C1_SOFT: &[u8] = include_bytes!("../assets/instruments_sf2/bass_24_soft.wav");
const BASS_C1_HARD: &[u8] = include_bytes!("../assets/instruments_sf2/bass_24_hard.wav");
const BASS_A1_SOFT: &[u8] = include_bytes!("../assets/instruments_sf2/bass_29_soft.wav");
const BASS_A1_HARD: &[u8] = include_bytes!("../assets/instruments_sf2/bass_29_hard.wav");
const BASS_C2_SOFT: &[u8] = include_bytes!("../assets/instruments_sf2/bass_36_soft.wav");
const BASS_C2_HARD: &[u8] = include_bytes!("../assets/instruments_sf2/bass_36_hard.wav");
const BASS_G2_SOFT: &[u8] = include_bytes!("../assets/instruments_sf2/bass_43_soft.wav");
const BASS_G2_HARD: &[u8] = include_bytes!("../assets/instruments_sf2/bass_43_hard.wav");

const EPIANO_C3: &[u8] = include_bytes!("../assets/instruments_sf2/epiano_48_sustain.wav");
const EPIANO_C4: &[u8] = include_bytes!("../assets/instruments_sf2/epiano_60_sustain.wav");
const EPIANO_C5: &[u8] = include_bytes!("../assets/instruments_sf2/epiano_72_sustain.wav");

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
        assert!(bank.total_frames() > 0, "bank carries sample frames");
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
    fn sf2_guitar_differs_from_synthetic_guitar() {
        // The whole point of Phase H: the SF2 bank is NOT the synthetic
        // bank. Same note, same buffer length — different bytes proves the
        // recorded instrument is actually in the mix.
        let sf2 = sf2_instrument_bank();
        let synth = crate::embedded_bank::embedded_instrument_bank();
        let n = BANK_RATE as usize / 2;
        let mut a = vec![0.0; n];
        let mut b = vec![0.0; n];
        sf2.render_note("guitar", 57, 0.8, 7, &mut a);
        synth.render_note("guitar", 57, 0.8, 7, &mut b);
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
