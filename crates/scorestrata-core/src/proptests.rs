//! Property-based tests for core domain types.
//!
//! These tests verify invariants that must hold for all inputs,
//! complementing the unit tests with randomized input generation.

use proptest::prelude::*;

use crate::domain::{Duration, MidiNote, PPQ, Tick, Velocity};

proptest! {
    /// Tick round-trip: from_bar_beat_tick -> to_bar_beat_tick should be lossless.
    #[test]
    fn tick_round_trip(bar in 0u32..1000, beat in 0u32..4u32, sub_tick in 0u32..PPQ) {
        let ts = 4u32; // 4/4 time
        let tick = Tick::from_bar_beat_tick(bar, beat, sub_tick, ts);
        let (bar2, beat2, sub_tick2) = tick.to_bar_beat_tick(ts);
        prop_assert_eq!((bar2, beat2, sub_tick2), (bar, beat, sub_tick));
    }

    /// MidiNote::new clamps to 0..127.
    #[test]
    fn midi_note_clamps(val in 0u32..256) {
        #[expect(clippy::cast_possible_truncation, reason = "Proptest range is bounded")]
        let note = MidiNote::new(val as u8);
        prop_assert!(note.0 <= 127);
    }

    /// Velocity::new clamps to 0..127.
    #[test]
    fn velocity_clamps(val in 0u32..256u32) {
        #[expect(clippy::cast_possible_truncation, reason = "Proptest range is bounded")]
        let vel = Velocity::new(val as u8);
        prop_assert!(vel.0 <= 127);
    }

    /// Velocity::as_f32 is always in 0.0..=1.0.
    #[test]
    fn velocity_normalized(val in 0u8..128u8) {
        let vel = Velocity::new(val);
        let normalized = vel.as_f32();
        prop_assert!((0.0..=1.0).contains(&normalized));
    }

    /// Tick round-trip in 3/4 time.
    #[test]
    fn tick_round_trip_34(bar in 0u32..500, beat in 0u32..3u32, sub_tick in 0u32..PPQ) {
        let ts = 3u32;
        let tick = Tick::from_bar_beat_tick(bar, beat, sub_tick, ts);
        let (bar2, beat2, sub_tick2) = tick.to_bar_beat_tick(ts);
        prop_assert_eq!((bar2, beat2, sub_tick2), (bar, beat, sub_tick));
    }

    /// Tick round-trip in 5/4 time.
    #[test]
    fn tick_round_trip_54(bar in 0u32..200, beat in 0u32..5u32, sub_tick in 0u32..PPQ) {
        let ts = 5u32;
        let tick = Tick::from_bar_beat_tick(bar, beat, sub_tick, ts);
        let (bar2, beat2, sub_tick2) = tick.to_bar_beat_tick(ts);
        prop_assert_eq!((bar2, beat2, sub_tick2), (bar, beat, sub_tick));
    }

    /// Tick round-trip in 7/8 time (7 beats of 3/8 subdivisions).
    #[test]
    fn tick_round_trip_78(bar in 0u32..100, beat in 0u32..7u32, sub_tick in 0u32..PPQ) {
        let ts = 7u32;
        let tick = Tick::from_bar_beat_tick(bar, beat, sub_tick, ts);
        let (bar2, beat2, sub_tick2) = tick.to_bar_beat_tick(ts);
        prop_assert_eq!((bar2, beat2, sub_tick2), (bar, beat, sub_tick));
    }

    /// MidiNote pitch_class is always in 0..12.
    #[test]
    fn midi_note_pitch_class_in_bounds(val in 0u8..128u8) {
        let note = MidiNote::new(val);
        prop_assert!(note.pitch_class() < 12);
    }

    /// MidiNote octave is in range -1..=9.
    #[test]
    fn midi_note_octave_in_range(val in 0u8..128u8) {
        let note = MidiNote::new(val);
        prop_assert!((-1..=9).contains(&note.octave()));
    }

}

#[cfg(test)]
mod unit_tests {
    use super::*;

    #[test]
    fn tick_round_trip_44() {
        let ts = 4u32;
        let tick = Tick::from_bar_beat_tick(2, 3, 120, ts);
        let (bar, beat, sub) = tick.to_bar_beat_tick(ts);
        assert_eq!((bar, beat, sub), (2, 3, 120));
    }

    #[test]
    fn tick_round_trip_34() {
        let ts = 3u32;
        let tick = Tick::from_bar_beat_tick(1, 2, 60, ts);
        let (bar, beat, sub) = tick.to_bar_beat_tick(ts);
        assert_eq!((bar, beat, sub), (1, 2, 60));
    }

    #[test]
    fn midi_note_name_returns_string() {
        let note = MidiNote::new(60);
        assert_eq!(note.name(), "C4");
    }

    #[test]
    fn duration_constants_correct() {
        assert_eq!(Duration::whole().0, 1920);
        assert_eq!(Duration::half().0, 960);
        assert_eq!(Duration::quarter().0, 480);
        assert_eq!(Duration::eighth().0, 240);
        assert_eq!(Duration::sixteenth().0, 120);
    }

    #[test]
    fn duration_ordering() {
        assert!(Duration::whole().0 > Duration::half().0);
        assert!(Duration::half().0 > Duration::quarter().0);
        assert!(Duration::quarter().0 > Duration::eighth().0);
        assert!(Duration::eighth().0 > Duration::sixteenth().0);
    }

    #[test]
    fn duration_ratios() {
        assert_eq!(Duration::whole().0, Duration::half().0 * 2);
        assert_eq!(Duration::half().0, Duration::quarter().0 * 2);
        assert_eq!(Duration::quarter().0, Duration::eighth().0 * 2);
        assert_eq!(Duration::eighth().0, Duration::sixteenth().0 * 2);
    }
}
