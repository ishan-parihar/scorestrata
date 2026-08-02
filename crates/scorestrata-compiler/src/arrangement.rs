//! Arrangement expansion — converts project arrangement into segment boundaries.

use scorestrata_core::domain::{Arrangement, Segment};
use scorestrata_core::error::ScorestrataError;

/// Expanded segment boundary with absolute bar positions.
#[derive(Debug, Clone)]
pub struct ExpandedSegment {
    /// Segment reference.
    pub segment: Segment,
    /// Absolute start bar (0-indexed).
    pub start_bar: u32,
    /// Absolute end bar (exclusive).
    pub end_bar: u32,
}

/// Expand an arrangement into concrete segment boundaries.
///
/// Validates that segments don't overlap and cover the full duration.
///
/// # Errors
///
/// Returns an error if segments overlap, don't cover the full duration,
/// or have non-contiguous bar positions.
pub fn expand_arrangement(
    arrangement: &Arrangement,
) -> Result<Vec<ExpandedSegment>, ScorestrataError> {
    let mut expanded = Vec::new();
    let mut current_bar = 0u32;

    for segment in &arrangement.segments {
        if segment.start_bar != current_bar {
            return Err(ScorestrataError::CompilerError {
                pass: "arrangement_expansion".to_string(),
                message: format!(
                    "Segment '{}' starts at bar {} but expected bar {}",
                    segment.name, segment.start_bar, current_bar
                ),
            });
        }

        let end_bar = current_bar + segment.bars;
        if end_bar > arrangement.total_bars {
            return Err(ScorestrataError::CompilerError {
                pass: "arrangement_expansion".to_string(),
                message: format!(
                    "Segment '{}' extends to bar {} but arrangement is only {} bars",
                    segment.name, end_bar, arrangement.total_bars
                ),
            });
        }

        expanded.push(ExpandedSegment {
            segment: segment.clone(),
            start_bar: current_bar,
            end_bar,
        });

        current_bar = end_bar;
    }

    if current_bar != arrangement.total_bars {
        return Err(ScorestrataError::CompilerError {
            pass: "arrangement_expansion".to_string(),
            message: format!(
                "Arrangement segments cover {} bars but total_bars is {}",
                current_bar, arrangement.total_bars
            ),
        });
    }

    Ok(expanded)
}

#[cfg(test)]
mod tests {
    use super::*;
    use scorestrata_core::domain::{SegmentRole, TimeSignature};
    use scorestrata_core::ids::SegmentId;

    fn make_segment(name: &str, bars: u32, start_bar: u32) -> Segment {
        Segment {
            id: SegmentId::new(),
            name: name.to_string(),
            role: SegmentRole::Verse,
            start_bar,
            bars,
            layers: Vec::new(),
        }
    }

    #[test]
    fn valid_arrangement_expands_correctly() {
        let arrangement = Arrangement {
            total_bars: 16,
            time_signature: TimeSignature::default(),
            segments: vec![make_segment("verse", 8, 0), make_segment("chorus", 8, 8)],
            energy_curve: Vec::new(),
        };

        #[expect(clippy::unwrap_used, reason = "Test assertions")]
        let expanded = expand_arrangement(&arrangement).unwrap();
        assert_eq!(expanded.len(), 2);
        assert_eq!(expanded[0].start_bar, 0);
        assert_eq!(expanded[0].end_bar, 8);
        assert_eq!(expanded[1].start_bar, 8);
        assert_eq!(expanded[1].end_bar, 16);
    }

    #[test]
    fn overlapping_segments_fail() {
        let arrangement = Arrangement {
            total_bars: 16,
            time_signature: TimeSignature::default(),
            segments: vec![
                make_segment("verse", 8, 0),
                make_segment("chorus", 8, 4), // overlaps!
            ],
            energy_curve: Vec::new(),
        };

        assert!(expand_arrangement(&arrangement).is_err());
    }

    #[test]
    fn underfilled_arrangement_fails() {
        let arrangement = Arrangement {
            total_bars: 16,
            time_signature: TimeSignature::default(),
            segments: vec![make_segment("verse", 8, 0)], // only 8 of 16 bars
            energy_curve: Vec::new(),
        };

        assert!(expand_arrangement(&arrangement).is_err());
    }
}
