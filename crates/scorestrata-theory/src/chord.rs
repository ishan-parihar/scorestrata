//! Chord types and chord construction.

use crate::interval::{self, Interval};
use crate::scale::PitchClass;
use scorestrata_core::domain::MidiNote;
use serde::{Deserialize, Serialize};

/// A chord quality.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum ChordQuality {
    /// Major triad.
    Major,
    /// Minor triad.
    Minor,
    /// Diminished triad.
    Diminished,
    /// Augmented triad.
    Augmented,
    /// Dominant 7th.
    Dominant7,
    /// Major 7th.
    Major7,
    /// Minor 7th.
    Minor7,
    /// Diminished 7th.
    Diminished7,
    /// Half-diminished 7th.
    HalfDiminished7,
    /// Suspended 2nd.
    Sus2,
    /// Suspended 4th.
    Sus4,
}

impl ChordQuality {
    /// Get the intervals that make up this chord quality.
    #[must_use]
    pub fn intervals(&self) -> Vec<Interval> {
        match self {
            Self::Major => vec![
                interval::common::unison(),
                interval::common::major_third(),
                interval::common::perfect_fifth(),
            ],
            Self::Minor => vec![
                interval::common::unison(),
                interval::common::minor_third(),
                interval::common::perfect_fifth(),
            ],
            Self::Diminished => vec![
                interval::common::unison(),
                interval::common::minor_third(),
                interval::common::tritone(),
            ],
            Self::Augmented => vec![
                interval::common::unison(),
                interval::common::major_third(),
                interval::common::minor_sixth(),
            ],
            Self::Dominant7 => vec![
                interval::common::unison(),
                interval::common::major_third(),
                interval::common::perfect_fifth(),
                interval::common::minor_seventh(),
            ],
            Self::Major7 => vec![
                interval::common::unison(),
                interval::common::major_third(),
                interval::common::perfect_fifth(),
                interval::common::major_seventh(),
            ],
            Self::Minor7 => vec![
                interval::common::unison(),
                interval::common::minor_third(),
                interval::common::perfect_fifth(),
                interval::common::minor_seventh(),
            ],
            Self::Diminished7 => vec![
                interval::common::unison(),
                interval::common::minor_third(),
                interval::common::tritone(),
                interval::common::minor_sixth(),
            ],
            Self::HalfDiminished7 => vec![
                interval::common::unison(),
                interval::common::minor_third(),
                interval::common::tritone(),
                interval::common::minor_seventh(),
            ],
            Self::Sus2 => vec![
                interval::common::unison(),
                interval::common::major_second(),
                interval::common::perfect_fifth(),
            ],
            Self::Sus4 => vec![
                interval::common::unison(),
                interval::common::perfect_fourth(),
                interval::common::perfect_fifth(),
            ],
        }
    }

    /// Symbol representation.
    #[must_use]
    pub const fn symbol(&self) -> &'static str {
        match self {
            Self::Major => "",
            Self::Minor => "m",
            Self::Diminished => "dim",
            Self::Augmented => "aug",
            Self::Dominant7 => "7",
            Self::Major7 => "maj7",
            Self::Minor7 => "m7",
            Self::Diminished7 => "dim7",
            Self::HalfDiminished7 => "m7b5",
            Self::Sus2 => "sus2",
            Self::Sus4 => "sus4",
        }
    }
}

/// A chord with root and quality.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Chord {
    /// Root pitch class.
    pub root: u8,
    /// Chord quality.
    pub quality: ChordQuality,
}

impl Chord {
    /// Get the pitch classes in this chord.
    #[must_use]
    pub fn pitch_classes(&self) -> Vec<u8> {
        self.quality
            .intervals()
            .iter()
            .map(|i| (self.root + i.semitones) % 12)
            .collect()
    }

    /// Check if a pitch class is in this chord.
    #[must_use]
    pub fn contains_pc(&self, pc: u8) -> bool {
        self.pitch_classes().contains(&(pc % 12))
    }

    /// Check if a MIDI note is in this chord.
    #[must_use]
    pub fn contains_note(&self, note: &MidiNote) -> bool {
        self.contains_pc(note.pitch_class())
    }

    /// Get the chord name (e.g. "Cm", "G7").
    #[must_use]
    pub fn name(&self) -> String {
        let root_name = PitchClass::from_u8(self.root)
            .map_or_else(|| format!("PC{}", self.root), |pc| pc.name().to_string());
        format!("{}{}", root_name, self.quality.symbol())
    }

    /// Get the notes in this chord within a MIDI range.
    #[must_use]
    pub fn notes_in_range(&self, low: MidiNote, high: MidiNote) -> Vec<MidiNote> {
        let pcs = self.pitch_classes();
        let mut notes = Vec::new();
        for n in low.0..=high.0 {
            let note = MidiNote::new(n);
            if pcs.contains(&note.pitch_class()) {
                notes.push(note);
            }
        }
        notes
    }
}

/// A chord progression.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChordProgression {
    /// Chords in order.
    pub chords: Vec<Chord>,
    /// Duration of each chord in beats.
    pub beats_per_chord: Vec<u32>,
}

impl ChordProgression {
    /// Check if a note at a given beat is in the current chord.
    #[must_use]
    pub fn note_in_chord_at_beat(&self, note: &MidiNote, beat: u32) -> bool {
        let mut accumulated = 0u32;
        for (chord, &beats) in self.chords.iter().zip(&self.beats_per_chord) {
            if beat < accumulated + beats {
                return chord.contains_note(note);
            }
            accumulated += beats;
        }
        // If beat is past the progression, use the last chord
        self.chords.last().is_some_and(|c| c.contains_note(note))
    }

    /// Total beats in the progression.
    #[must_use]
    pub fn total_beats(&self) -> u32 {
        self.beats_per_chord.iter().sum()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn c_major_chord_contains_expected_notes() {
        let chord = Chord {
            root: 0, // C
            quality: ChordQuality::Major,
        };
        assert!(chord.contains_pc(0)); // C
        assert!(chord.contains_pc(4)); // E
        assert!(chord.contains_pc(7)); // G
        assert!(!chord.contains_pc(1)); // C#
    }

    #[test]
    fn chord_name_formatting() {
        let chord = Chord {
            root: 7, // G
            quality: ChordQuality::Dominant7,
        };
        assert_eq!(chord.name(), "G7");
    }
}
