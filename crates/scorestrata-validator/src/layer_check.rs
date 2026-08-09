//! Layer-level validation checks (§22.2 `layer` scope).
//!
//! The layer plane sits between segments and patterns: it checks each
//! layer's own declaration (instrument, octave register, whether it can
//! actually produce sound) without compiling. Combined with
//! [`crate::register_check`] (which validates notes against registers at
//! the timeline plane), this scope keeps layer edits cheap to validate.

use scorestrata_core::diagnostic::{Diagnostic, MetricScope, SuggestedPatch};
use scorestrata_core::domain::{Layer, Project};
use scorestrata_core::error::Severity;
use scorestrata_core::ids::DiagnosticId;

/// Check a single layer's declaration.
///
/// Returns `None` if the layer is well-formed.
#[must_use]
pub fn check_layer(layer: &Layer) -> Option<Diagnostic> {
    // Every layer must declare an instrument so the renderer knows which
    // voice family (or sample set) to use.
    if layer.instrument.trim().is_empty() {
        return Some(Diagnostic {
            id: DiagnosticId::new(),
            code: "LAYER_NO_INSTRUMENT".to_string(),
            metric: "structure.layer_instrument".to_string(),
            severity: Severity::Error,
            scope: MetricScope {
                segment_id: None,
                layer_id: Some(layer.id),
                bar_range: None,
            },
            evidence_uris: vec![format!("project://layers/{}", layer.id)],
            message: format!("Layer '{}' has no instrument declared", layer.name),
            root_causes: vec!["instrument field is empty".to_string()],
            suggested_patches: vec![SuggestedPatch {
                op: "set_layer_instrument".to_string(),
                params: serde_json::json!({ "layer_id": layer.id, "instrument": "synth" }),
                expected_delta: Some("+0.30".to_string()),
                confidence: 0.9,
            }],
            expected_delta: None,
            confidence: 0.9,
        });
    }

    // A declared register must be sane: min <= max (the discipline the
    // compiler enforces by clamping notes into the band).
    if let Some(register) = &layer.register
        && register.min_octave > register.max_octave
    {
        return Some(Diagnostic {
            id: DiagnosticId::new(),
            code: "LAYER_INVALID_REGISTER".to_string(),
            metric: "structure.layer_register".to_string(),
            severity: Severity::Error,
            scope: MetricScope {
                segment_id: None,
                layer_id: Some(layer.id),
                bar_range: None,
            },
            evidence_uris: vec![format!("project://layers/{}", layer.id)],
            message: format!(
                "Layer '{}' declares an inverted register ({} > {})",
                layer.name, register.min_octave, register.max_octave
            ),
            root_causes: vec!["register min exceeds max".to_string()],
            suggested_patches: vec![SuggestedPatch {
                op: "set_layer_register".to_string(),
                params: serde_json::json!({
                    "layer_id": layer.id,
                    "min_octave": register.max_octave,
                    "max_octave": register.max_octave,
                }),
                expected_delta: Some("+0.20".to_string()),
                confidence: 0.95,
            }],
            expected_delta: None,
            confidence: 0.95,
        });
    }

    // A layer with neither pattern instances nor automation is silent —
    // it occupies a track but contributes nothing to the render.
    if layer.pattern_instances.is_empty() && layer.automation.is_empty() {
        return Some(Diagnostic {
            id: DiagnosticId::new(),
            code: "LAYER_SILENT".to_string(),
            metric: "structure.layer_content".to_string(),
            severity: Severity::Warning,
            scope: MetricScope {
                segment_id: None,
                layer_id: Some(layer.id),
                bar_range: None,
            },
            evidence_uris: vec![format!("project://layers/{}", layer.id)],
            message: format!(
                "Layer '{}' has no pattern instances or automation — it will render silent",
                layer.name
            ),
            root_causes: vec!["layer is empty".to_string()],
            suggested_patches: vec![SuggestedPatch {
                op: "place_pattern_in_segment".to_string(),
                params: serde_json::json!({ "layer_id": layer.id }),
                expected_delta: Some("+0.25".to_string()),
                confidence: 0.7,
            }],
            expected_delta: None,
            confidence: 0.7,
        });
    }

    None
}

/// Validate every layer across all segments of a project.
///
/// Returns the union of all per-layer diagnostics (`LAYER_*` codes).
#[must_use]
pub fn check_layers(project: &Project) -> Vec<Diagnostic> {
    let mut diagnostics = Vec::new();
    for segment in &project.arrangement.segments {
        for layer in &segment.layers {
            if let Some(d) = check_layer(layer) {
                diagnostics.push(d);
            }
        }
    }
    diagnostics
}

#[cfg(test)]
#[expect(
    clippy::expect_used,
    reason = "Tests unwrap fixture layers and lookups"
)]
mod tests {
    use super::*;
    use scorestrata_core::domain::{
        Arrangement, Brief, DeliveryTarget, Layer, PatternInstance, Register, Segment, SegmentRole,
        Tick, TimeSignature,
    };
    use scorestrata_core::ids::{LayerId, PatternId, SegmentId};

    fn project_with_layers(layers: Vec<Layer>) -> Project {
        use scorestrata_core::ids::ProjectId;
        Project {
            id: ProjectId::new(),
            name: "test".to_string(),
            created_at: chrono::Utc::now(),
            modified_at: chrono::Utc::now(),
            brief: Brief {
                raw_text: "test".to_string(),
                genre: None,
                mood: vec![],
                references: vec![],
                delivery_target: Some(DeliveryTarget::Streaming),
                constraints: vec![],
                confidence: 0.5,
            },
            config: scorestrata_core::domain::ProjectConfig {
                name: "test".to_string(),
                genre: "techno".to_string(),
                bpm: 124,
                key: "C minor".to_string(),
                time_signature: TimeSignature::default(),
                duration_bars: 8,
                delivery_target: DeliveryTarget::Streaming,
                mood: vec![],
                references: vec![],
            },
            arrangement: Arrangement {
                total_bars: 8,
                time_signature: TimeSignature::default(),
                segments: vec![Segment {
                    id: SegmentId::new(),
                    name: "loop".to_string(),
                    role: SegmentRole::Chorus,
                    start_bar: 0,
                    bars: 8,
                    layers,
                }],
                energy_curve: Vec::new(),
            },
            tracks: vec![],
            patterns: vec![],
            spectral_reference: None,
            mixer: scorestrata_core::domain::MixerState::default(),
            status: scorestrata_core::domain::ProjectStatus::ConfigApproved,
        }
    }

    fn layer_named(name: &str) -> Layer {
        Layer {
            id: LayerId::new(),
            name: name.to_string(),
            instrument: "synth".to_string(),
            pattern_instances: vec![PatternInstance {
                id: scorestrata_core::ids::PatternInstanceId::new(),
                pattern_id: PatternId::new(),
                start_tick: Tick::new(0),
                transpose: 0,
                velocity_multiplier: 1.0,
            }],
            register: None,
            automation: vec![],
            fx: None,
            mix_gain_db: None,
            mix_pan: None,
        }
    }

    #[test]
    fn layer_without_instrument_is_diagnosed() {
        let mut layer = layer_named("bass");
        layer.instrument = String::new();
        let d = check_layer(&layer).expect("diagnoses");
        assert_eq!(d.code, "LAYER_NO_INSTRUMENT");
        assert_eq!(d.suggested_patches[0].op, "set_layer_instrument");
    }

    #[test]
    fn inverted_register_is_diagnosed() {
        let mut layer = layer_named("lead");
        layer.register = Some(Register {
            min_octave: 7,
            max_octave: 3,
        });
        let d = check_layer(&layer).expect("diagnoses");
        assert_eq!(d.code, "LAYER_INVALID_REGISTER");
        // Fix collapses to the max band so it is deterministic.
        assert_eq!(
            d.suggested_patches[0].params["min_octave"],
            serde_json::json!(3)
        );
        assert_eq!(
            d.suggested_patches[0].params["max_octave"],
            serde_json::json!(3)
        );
    }

    #[test]
    fn silent_layer_is_diagnosed() {
        let mut layer = layer_named("pad");
        layer.pattern_instances = Vec::new();
        layer.automation = vec![];
        let d = check_layer(&layer).expect("diagnoses");
        assert_eq!(d.code, "LAYER_SILENT");
    }

    #[test]
    fn well_formed_layer_passes() {
        assert!(check_layer(&layer_named("lead")).is_none());
    }

    #[test]
    fn check_layers_aggregates_across_segments() {
        let good = layer_named("lead");
        let mut bad = layer_named("bass");
        bad.instrument = String::new();
        let project = project_with_layers(vec![good, bad]);
        let diagnostics = check_layers(&project);
        assert!(diagnostics.iter().any(|d| d.code == "LAYER_NO_INSTRUMENT"));
        assert_eq!(diagnostics.len(), 1);
    }
}
