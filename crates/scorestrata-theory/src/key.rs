//! Musical key representation (root + mode).

use crate::scale::{PitchClass, Scale, ScalePattern, patterns};
use scorestrata_core::domain::MidiNote;
use serde::{Deserialize, Serialize};

/// The mode of a key.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Mode {
    /// Major (Ionian).
    Major,
    /// Minor (Aeolian).
    Minor,
    /// Dorian.
    Dorian,
    /// Mixolydian.
    Mixolydian,
}

impl Mode {
    /// Get the scale pattern for this mode.
    #[must_use]
    pub fn scale_pattern(&self) -> ScalePattern {
        match self {
            Self::Major => patterns::major(),
            Self::Minor => patterns::minor(),
            Self::Dorian => patterns::dorian(),
            Self::Mixolydian => patterns::mixolydian(),
        }
    }
}

/// A musical key (root pitch class + mode).
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Key {
    /// Root pitch class (0–11).
    pub root: u8,
    /// Mode.
    pub mode: Mode,
}

impl Key {
    /// Create a new key.
    #[must_use]
    pub const fn new(root: u8, mode: Mode) -> Self {
        Self { root, mode }
    }

    /// Parse a key string like "C minor" or "G Major".
    #[must_use]
    pub fn parse(s: &str) -> Option<Self> {
        let parts: Vec<&str> = s.split_whitespace().collect();
        if parts.len() != 2 {
            return None;
        }
        let root = parse_root(parts[0])?;
        let mode = match parts[1].to_lowercase().as_str() {
            "major" | "maj" | "ionian" => Mode::Major,
            "minor" | "min" | "aeolian" => Mode::Minor,
            "dorian" => Mode::Dorian,
            "mixolydian" | "mix" => Mode::Mixolydian,
            _ => return None,
        };
        Some(Self { root, mode })
    }

    /// Get the scale for this key.
    #[must_use]
    pub fn to_scale(&self) -> Scale {
        Scale {
            root: self.root,
            pattern: self.mode.scale_pattern(),
        }
    }

    /// Check if a MIDI note is in this key.
    #[must_use]
    pub fn contains_note(&self, note: &MidiNote) -> bool {
        self.to_scale().contains_note(note)
    }

    /// Get the key name (e.g. "C minor").
    #[must_use]
    pub fn name(&self) -> String {
        let root_name = PitchClass::from_u8(self.root)
            .map_or_else(|| format!("PC{}", self.root), |pc| pc.name().to_string());
        let mode_name = match self.mode {
            Mode::Major => "Major",
            Mode::Minor => "minor",
            Mode::Dorian => "Dorian",
            Mode::Mixolydian => "Mixolydian",
        };
        format!("{root_name} {mode_name}")
    }
}

fn parse_root(s: &str) -> Option<u8> {
    match s {
        "C" | "c" => Some(0),
        "C#" | "c#" | "Db" | "db" => Some(1),
        "D" | "d" => Some(2),
        "D#" | "d#" | "Eb" | "eb" => Some(3),
        "E" | "e" => Some(4),
        "F" | "f" => Some(5),
        "F#" | "f#" | "Gb" | "gb" => Some(6),
        "G" | "g" => Some(7),
        "G#" | "g#" | "Ab" | "ab" => Some(8),
        "A" | "a" => Some(9),
        "A#" | "a#" | "Bb" | "bb" => Some(10),
        "B" | "b" => Some(11),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    #[allow(clippy::unwrap_used, reason = "Test should panic if parsing fails")]
    fn parse_c_minor() {
        let key = Key::parse("C minor").unwrap();
        assert_eq!(key.root, 0);
        assert_eq!(key.mode, Mode::Minor);
    }

    #[test]
    fn key_contains_expected_notes() {
        let key = Key::new(0, Mode::Minor); // C minor
        assert!(key.contains_note(&MidiNote::new(60))); // C4
        assert!(key.contains_note(&MidiNote::new(63))); // Eb4
        assert!(!key.contains_note(&MidiNote::new(61))); // C#4
    }
}
