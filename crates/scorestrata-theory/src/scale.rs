//! Musical scales and scale operations.

use scorestrata_core::domain::MidiNote;
use serde::{Deserialize, Serialize};

/// The 12 pitch classes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum PitchClass {
    /// C (0)
    C,
    /// C# (1)
    Cs,
    /// D (2)
    D,
    /// D# (3)
    Ds,
    /// E (4)
    E,
    /// F (5)
    F,
    /// F# (6)
    Fs,
    /// G (7)
    G,
    /// G# (8)
    Gs,
    /// A (9)
    A,
    /// A# (10)
    As,
    /// B (11)
    B,
}

impl PitchClass {
    /// Convert to a numeric value (0–11).
    #[must_use]
    pub const fn as_u8(self) -> u8 {
        match self {
            Self::C => 0,
            Self::Cs => 1,
            Self::D => 2,
            Self::Ds => 3,
            Self::E => 4,
            Self::F => 5,
            Self::Fs => 6,
            Self::G => 7,
            Self::Gs => 8,
            Self::A => 9,
            Self::As => 10,
            Self::B => 11,
        }
    }

    /// Create from a numeric value (0–11).
    #[must_use]
    pub const fn from_u8(n: u8) -> Option<Self> {
        match n % 12 {
            0 => Some(Self::C),
            1 => Some(Self::Cs),
            2 => Some(Self::D),
            3 => Some(Self::Ds),
            4 => Some(Self::E),
            5 => Some(Self::F),
            6 => Some(Self::Fs),
            7 => Some(Self::G),
            8 => Some(Self::Gs),
            9 => Some(Self::A),
            10 => Some(Self::As),
            11 => Some(Self::B),
            _ => None,
        }
    }

    /// Get the name as a string.
    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::C => "C",
            Self::Cs => "C#",
            Self::D => "D",
            Self::Ds => "D#",
            Self::E => "E",
            Self::F => "F",
            Self::Fs => "F#",
            Self::G => "G",
            Self::Gs => "G#",
            Self::A => "A",
            Self::As => "A#",
            Self::B => "B",
        }
    }
}

/// Interval pattern for a scale (semitone steps from root).
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct ScalePattern {
    /// Name of the scale.
    pub name: String,
    /// Semitone intervals from root.
    pub intervals: Vec<u8>,
}

impl ScalePattern {
    /// Get the set of pitch classes in this scale.
    #[must_use]
    pub fn pitch_classes(&self) -> Vec<u8> {
        let mut pcs = vec![0u8];
        let mut current = 0u8;
        for &interval in &self.intervals {
            current = (current + interval) % 12;
            if !pcs.contains(&current) {
                pcs.push(current);
            }
        }
        pcs.sort_unstable();
        pcs.dedup();
        pcs
    }

    /// Check if a pitch class is in this scale.
    #[must_use]
    pub fn contains(&self, pitch_class: u8) -> bool {
        self.pitch_classes().contains(&(pitch_class % 12))
    }

    /// Get the number of notes in the scale.
    #[must_use]
    pub fn note_count(&self) -> usize {
        self.pitch_classes().len()
    }
}

/// A scale with a root note.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Scale {
    /// Root pitch class (0–11).
    pub root: u8,
    /// Scale pattern.
    pub pattern: ScalePattern,
}

impl Scale {
    /// Check if a MIDI note is in this scale.
    #[must_use]
    pub fn contains_note(&self, note: &MidiNote) -> bool {
        self.pattern.contains(note.pitch_class())
    }

    /// Get the scale degree of a note (0 = root, 1 = second, etc.).
    /// Returns None if the note is not in the scale.
    #[must_use]
    pub fn degree_of(&self, note: &MidiNote) -> Option<u8> {
        let pcs = self.pattern.pitch_classes();
        let target = (note.pitch_class() + 12 - self.root) % 12;
        pcs.iter().position(|&pc| pc == target).map(|i: usize| {
            #[expect(
                clippy::cast_possible_truncation,
                reason = "Scale has at most 12 notes, fits in u8"
            )]
            let deg = i as u8;
            deg
        })
    }

    /// Quantize a note to the nearest scale tone.
    #[must_use]
    pub fn quantize(&self, note: &MidiNote) -> MidiNote {
        if self.contains_note(note) {
            return *note;
        }
        let pc = note.pitch_class();
        let pcs = self.pattern.pitch_classes();
        // Find nearest pitch class
        let mut best_dist = 12u8;
        let mut best_pc = pc;
        for &scale_pc in &pcs {
            #[expect(
                clippy::cast_possible_truncation,
                reason = "Interval is bounded 0..12, fits in u8"
            )]
            let dist = (i16::from(pc) - i16::from(scale_pc)).unsigned_abs() as u8;
            let dist_rev = 12 - dist;
            let d = dist.min(dist_rev);
            if d < best_dist {
                best_dist = d;
                best_pc = scale_pc;
            }
        }
        // Preserve octave as much as possible
        let diff = i16::from(best_pc) - i16::from(pc);
        #[expect(
            clippy::cast_sign_loss,
            reason = "Clamped to 0..127 for MIDI, sign is discarded safely"
        )]
        let new_note = (i16::from(note.0) + diff).clamp(0, 127) as u8;
        MidiNote::new(new_note)
    }

    /// Get all notes in the scale within a MIDI range.
    #[must_use]
    pub fn notes_in_range(&self, low: MidiNote, high: MidiNote) -> Vec<MidiNote> {
        let mut notes = Vec::new();
        for n in low.0..=high.0 {
            let note = MidiNote::new(n);
            if self.contains_note(&note) {
                notes.push(note);
            }
        }
        notes
    }
}

/// Common scale patterns.
pub mod patterns {
    use super::ScalePattern;

    /// Major scale (Ionian mode).
    #[must_use]
    pub fn major() -> ScalePattern {
        ScalePattern {
            name: "major".to_string(),
            intervals: vec![2, 2, 1, 2, 2, 2, 1],
        }
    }

    /// Natural minor scale (Aeolian mode).
    #[must_use]
    pub fn minor() -> ScalePattern {
        ScalePattern {
            name: "minor".to_string(),
            intervals: vec![2, 1, 2, 2, 1, 2, 2],
        }
    }

    /// Dorian mode.
    #[must_use]
    pub fn dorian() -> ScalePattern {
        ScalePattern {
            name: "dorian".to_string(),
            intervals: vec![2, 1, 2, 2, 2, 1, 2],
        }
    }

    /// Mixolydian mode.
    #[must_use]
    pub fn mixolydian() -> ScalePattern {
        ScalePattern {
            name: "mixolydian".to_string(),
            intervals: vec![2, 2, 1, 2, 2, 1, 2],
        }
    }

    /// Pentatonic major.
    #[must_use]
    pub fn pentatonic_major() -> ScalePattern {
        ScalePattern {
            name: "pentatonic_major".to_string(),
            intervals: vec![2, 2, 3, 2, 3],
        }
    }

    /// Pentatonic minor.
    #[must_use]
    pub fn pentatonic_minor() -> ScalePattern {
        ScalePattern {
            name: "pentatonic_minor".to_string(),
            intervals: vec![3, 2, 2, 3, 2],
        }
    }

    /// Blues scale.
    #[must_use]
    pub fn blues() -> ScalePattern {
        ScalePattern {
            name: "blues".to_string(),
            intervals: vec![3, 2, 1, 1, 3, 2],
        }
    }

    /// Harmonic minor.
    #[must_use]
    pub fn harmonic_minor() -> ScalePattern {
        ScalePattern {
            name: "harmonic_minor".to_string(),
            intervals: vec![2, 1, 2, 2, 1, 3, 1],
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn c_major_contains_expected_notes() {
        let scale = Scale {
            root: 0, // C
            pattern: patterns::major(),
        };
        assert!(scale.contains_note(&MidiNote::new(60))); // C4
        assert!(scale.contains_note(&MidiNote::new(64))); // E4
        assert!(scale.contains_note(&MidiNote::new(67))); // G4
        assert!(!scale.contains_note(&MidiNote::new(61))); // C#4
        assert!(!scale.contains_note(&MidiNote::new(66))); // F#4
    }

    #[test]
    fn quantize_moves_note_to_nearest_scale_tone() {
        let scale = Scale {
            root: 0, // C
            pattern: patterns::major(),
        };
        // C#4 (61) should quantize to C4 (60) or D4 (62)
        let quantized = scale.quantize(&MidiNote::new(61));
        assert!(scale.contains_note(&quantized));
    }

    #[test]
    fn degree_of_returns_correct_degree() {
        let scale = Scale {
            root: 0, // C
            pattern: patterns::major(),
        };
        assert_eq!(scale.degree_of(&MidiNote::new(60)), Some(0)); // C = root
        assert_eq!(scale.degree_of(&MidiNote::new(64)), Some(2)); // E = 3rd
        assert_eq!(scale.degree_of(&MidiNote::new(67)), Some(4)); // G = 5th
    }
}

#[cfg(test)]
mod proptests {
    use super::*;
    use proptest::prelude::*;

    proptest! {
        #[test]
        fn quantize_always_returns_in_scale(note_val in 0u8..128u8) {
            let scale = Scale { root: 0, pattern: patterns::major() };
            let note = MidiNote::new(note_val);
            let quantized = scale.quantize(&note);
            prop_assert!(
                scale.contains_note(&quantized),
                "quantized {:?} not in scale", quantized
            );
        }
    }
}
