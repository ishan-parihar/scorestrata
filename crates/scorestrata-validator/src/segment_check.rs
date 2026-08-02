//! Segment-level validation checks.

use scorestrata_core::diagnostic::{Diagnostic, MetricScope};
use scorestrata_core::domain::Segment;
use scorestrata_core::error::Severity;
use scorestrata_core::ids::DiagnosticId;

/// Check that a segment has at least one layer.
#[must_use]
pub fn check_segment_has_layers(segment: &Segment) -> Option<Diagnostic> {
    if segment.layers.is_empty() {
        Some(Diagnostic {
            id: DiagnosticId::new(),
            code: "SEGMENT_EMPTY".to_string(),
            metric: "structure.segment_layers".to_string(),
            severity: Severity::Error,
            scope: MetricScope {
                segment_id: Some(segment.id),
                layer_id: None,
                bar_range: None,
            },
            evidence_uris: vec![format!("project://segments/{}", segment.id)],
            message: format!("Segment '{}' has no layers", segment.name),
            root_causes: vec!["no layers defined in segment".to_string()],
            suggested_patches: vec![],
            expected_delta: None,
            confidence: 1.0,
        })
    } else {
        None
    }
}

/// Check that a segment has a reasonable bar count.
#[must_use]
pub fn check_segment_bar_count(
    segment: &Segment,
    min_bars: u32,
    max_bars: u32,
) -> Option<Diagnostic> {
    if segment.bars < min_bars || segment.bars > max_bars {
        Some(Diagnostic {
            id: DiagnosticId::new(),
            code: "SEGMENT_BAR_COUNT_INVALID".to_string(),
            metric: "structure.segment_bars".to_string(),
            severity: Severity::Warning,
            scope: MetricScope {
                segment_id: Some(segment.id),
                layer_id: None,
                bar_range: None,
            },
            evidence_uris: vec![],
            message: format!(
                "Segment '{}' has {} bars (expected {}–{})",
                segment.name, segment.bars, min_bars, max_bars
            ),
            root_causes: vec!["segment duration outside acceptable range".to_string()],
            suggested_patches: vec![],
            expected_delta: None,
            confidence: 0.9,
        })
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use scorestrata_core::domain::{Layer, SegmentRole};
    use scorestrata_core::ids::{LayerId, SegmentId};

    #[test]
    fn empty_segment_produces_diagnostic() {
        let segment = Segment {
            id: SegmentId::default(),
            name: "test".to_string(),
            role: SegmentRole::Verse,
            start_bar: 0,
            bars: 8,
            layers: Vec::new(),
        };
        assert!(check_segment_has_layers(&segment).is_some());
    }

    #[test]
    fn segment_with_layers_passes() {
        let segment = Segment {
            id: SegmentId::default(),
            name: "test".to_string(),
            role: SegmentRole::Verse,
            start_bar: 0,
            bars: 8,
            layers: vec![Layer {
                id: LayerId::default(),
                name: "drums".to_string(),
                instrument: "drum_sampler".to_string(),
                pattern_instances: Vec::new(),
            }],
        };
        assert!(check_segment_has_layers(&segment).is_none());
    }
}
