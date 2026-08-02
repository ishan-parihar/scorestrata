//! Musical intervals.

use serde::{Deserialize, Serialize};

/// Named interval with semitone count.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Interval {
    /// Semitone count.
    pub semitones: u8,
}

impl Interval {
    /// Create an interval from semitones.
    #[must_use]
    pub const fn new(semitones: u8) -> Self {
        Self { semitones }
    }

    /// Get the interval name.
    #[must_use]
    pub fn name(&self) -> &'static str {
        match self.semitones % 12 {
            0 => "unison",
            1 => "minor 2nd",
            2 => "major 2nd",
            3 => "minor 3rd",
            4 => "major 3rd",
            5 => "perfect 4th",
            6 => "tritone",
            7 => "perfect 5th",
            8 => "minor 6th",
            9 => "major 6th",
            10 => "minor 7th",
            11 => "major 7th",
            _ => unreachable!(),
        }
    }

    /// Get the quality (perfect, major, minor, augmented, diminished).
    #[must_use]
    pub fn quality(&self) -> &'static str {
        match self.semitones % 12 {
            0 | 5 | 7 => "perfect",
            1 | 3 | 8 | 10 => "minor",
            2 | 4 | 9 | 11 => "major",
            6 => "augmented/diminished",
            _ => unreachable!(),
        }
    }

    /// Check if this interval is consonant.
    #[must_use]
    pub const fn is_consonant(&self) -> bool {
        matches!(self.semitones % 12, 0 | 3 | 4 | 5 | 7 | 8 | 9)
    }

    /// Invert this interval.
    #[must_use]
    pub const fn invert(&self) -> Self {
        Self::new((12 - self.semitones) % 12)
    }
}

/// Common intervals.
pub mod common {
    use super::Interval;

    /// Unison (0 semitones).
    #[must_use]
    pub const fn unison() -> Interval {
        Interval::new(0)
    }
    /// Minor second (1 semitone).
    #[must_use]
    pub const fn minor_second() -> Interval {
        Interval::new(1)
    }
    /// Major second (2 semitones).
    #[must_use]
    pub const fn major_second() -> Interval {
        Interval::new(2)
    }
    /// Minor third (3 semitones).
    #[must_use]
    pub const fn minor_third() -> Interval {
        Interval::new(3)
    }
    /// Major third (4 semitones).
    #[must_use]
    pub const fn major_third() -> Interval {
        Interval::new(4)
    }
    /// Perfect fourth (5 semitones).
    #[must_use]
    pub const fn perfect_fourth() -> Interval {
        Interval::new(5)
    }
    /// Tritone (6 semitones).
    #[must_use]
    pub const fn tritone() -> Interval {
        Interval::new(6)
    }
    /// Perfect fifth (7 semitones).
    #[must_use]
    pub const fn perfect_fifth() -> Interval {
        Interval::new(7)
    }
    /// Minor sixth (8 semitones).
    #[must_use]
    pub const fn minor_sixth() -> Interval {
        Interval::new(8)
    }
    /// Major sixth (9 semitones).
    #[must_use]
    pub const fn major_sixth() -> Interval {
        Interval::new(9)
    }
    /// Minor seventh (10 semitones).
    #[must_use]
    pub const fn minor_seventh() -> Interval {
        Interval::new(10)
    }
    /// Major seventh (11 semitones).
    #[must_use]
    pub const fn major_seventh() -> Interval {
        Interval::new(11)
    }
    /// Octave (12 semitones).
    #[must_use]
    pub const fn octave() -> Interval {
        Interval::new(12)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn interval_names() {
        assert_eq!(Interval::new(0).name(), "unison");
        assert_eq!(Interval::new(7).name(), "perfect 5th");
        assert_eq!(Interval::new(12).name(), "unison"); // octave wraps
    }

    #[test]
    fn interval_inversion() {
        assert_eq!(common::minor_third().invert(), common::major_sixth());
        assert_eq!(common::perfect_fifth().invert(), common::perfect_fourth());
    }
}
