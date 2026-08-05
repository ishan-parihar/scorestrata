//! Segment-level validation checks.

use scorestrata_core::diagnostic::{Diagnostic, MetricScope, SuggestedPatch};
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
            suggested_patches: vec![SuggestedPatch {
                op: "add_segment_layer".to_string(),
                params: serde_json::json!({ "segment_id": segment.id }),
                expected_delta: Some("+0.25".to_string()),
                confidence: 0.9,
            }],
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
            suggested_patches: vec![SuggestedPatch {
                op: "set_segment_bars".to_string(),
                params: serde_json::json!({
                    "segment_id": segment.id,
                    // Clamp to the nearest valid bound so the suggestion is
                    // deterministic and self-healing.
                    "bars": segment.bars.clamp(min_bars, max_bars),
                }),
                expected_delta: Some("+0.10".to_string()),
                confidence: 0.8,
            }],
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
        let diag = check_segment_has_layers(&segment).expect("empty segment diagnoses");
        assert_eq!(diag.code, "SEGMENT_EMPTY");
        // The fix must be surfaced so suggest_fixes can offer it.
        let patch = &diag.suggested_patches[0];
        assert_eq!(patch.op, "add_segment_layer");
        assert_eq!(patch.params["segment_id"], serde_json::json!(segment.id));
        assert!(patch.expected_delta.is_some());
    }

    #[test]
    fn bar_count_out_of_range_suggests_resize() {
        let segment = Segment {
            id: SegmentId::default(),
            name: "test".to_string(),
            role: SegmentRole::Verse,
            start_bar: 0,
            bars: 128, // way over the acceptable 4–64 range
            layers: Vec::new(),
        };
        let diag = check_segment_bar_count(&segment, 4, 64).expect("out-of-range diagnoses");
        assert_eq!(diag.code, "SEGMENT_BAR_COUNT_INVALID");
        let patch = &diag.suggested_patches[0];
        assert_eq!(patch.op, "set_segment_bars");
        // The suggestion clamps to the nearest valid bound (64), so applying
        // it deterministically clears the diagnostic.
        assert_eq!(patch.params["bars"], serde_json::json!(64));
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
                register: None,
            }],
        };
        assert!(check_segment_has_layers(&segment).is_none());
    }
}
