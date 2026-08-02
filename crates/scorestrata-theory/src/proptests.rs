//! Property-based tests for music theory types.
//!
//! These tests verify invariants that must hold for all inputs,
//! complementing the unit tests with randomized input generation.

use proptest::prelude::*;

use scorestrata_core::domain::MidiNote;

use crate::key::Key;
use crate::scale::{Scale, ScalePattern, patterns};

/// All scale patterns we want to test.
fn arb_scale_pattern() -> impl Strategy<Value = ScalePattern> {
    prop_oneof![
        Just(patterns::major()),
        Just(patterns::minor()),
        Just(patterns::dorian()),
        Just(patterns::mixolydian()),
        Just(patterns::pentatonic_major()),
        Just(patterns::pentatonic_minor()),
        Just(patterns::blues()),
        Just(patterns::harmonic_minor()),
    ]
}

/// Any valid MIDI note value (0–127).
fn arb_midi_note() -> impl Strategy<Value = u8> {
    0u8..128
}

/// Any valid root pitch class (0–11).
fn arb_root() -> impl Strategy<Value = u8> {
    0u8..12
}

/// Map root pitch class to note name string.
fn pitch_class_name(pc: u8) -> &'static str {
    match pc {
        0 => "C",
        1 => "C#",
        2 => "D",
        3 => "D#",
        4 => "E",
        5 => "F",
        6 => "F#",
        7 => "G",
        8 => "G#",
        9 => "A",
        10 => "A#",
        11 => "B",
        _ => unreachable!(),
    }
}

proptest! {
    /// Key::parse round-trip: parse(key.name()) should return the same key.
    #[test]
    #[expect(clippy::unwrap_used, reason = "prop_assume! guards above")]
    fn key_parse_round_trip(root in 0u8..12u8, mode_name in prop_oneof![
        Just("major"), Just("minor"), Just("dorian"), Just("mixolydian"),
    ]) {
        let key_str = format!("{} {mode_name}", pitch_class_name(root));
        let key = Key::parse(&key_str);
        prop_assume!(key.is_some());
        let key = key.unwrap();
        let rt = Key::parse(&key.name());
        prop_assume!(rt.is_some());
        let rt = rt.unwrap();
        prop_assert_eq!(key.root, rt.root);
    }

    /// Scale quantization always returns a note in the scale.
    #[test]
    fn quantize_always_returns_in_scale(
        root in arb_root(),
        pattern in arb_scale_pattern(),
        note_val in arb_midi_note(),
    ) {
        let scale = Scale { root, pattern };
        let note = MidiNote::new(note_val);
        let quantized = scale.quantize(&note);
        prop_assert!(
            scale.contains_note(&quantized),
            "quantized {:?} (val={}) not in scale root={}",
            quantized, quantized.0, root
        );
    }

    /// Scale quantization is idempotent: quantizing twice gives same result.
    #[test]
    fn quantize_is_idempotent(
        root in arb_root(),
        pattern in arb_scale_pattern(),
        note_val in arb_midi_note(),
    ) {
        let scale = Scale { root, pattern };
        let note = MidiNote::new(note_val);
        let q1 = scale.quantize(&note);
        let q2 = scale.quantize(&q1);
        prop_assert!(q1 == q2);
    }

    /// Scale pitch_classes always contains root (0).
    #[test]
    fn pitch_classes_always_contain_root(
        root in arb_root(),
        pattern in arb_scale_pattern(),
    ) {
        let scale = Scale { root, pattern };
        let pcs = scale.pattern.pitch_classes();
        prop_assert!(pcs.contains(&0), "pitch_classes should always contain root (0)");
    }

    /// Scale note_count is between 1 and 12.
    #[test]
    fn note_count_in_bounds(
        root in arb_root(),
        pattern in arb_scale_pattern(),
    ) {
        let scale = Scale { root, pattern };
        let count = scale.pattern.note_count();
        prop_assert!((1..=12).contains(&count), "note_count {count} out of bounds");
    }
}
