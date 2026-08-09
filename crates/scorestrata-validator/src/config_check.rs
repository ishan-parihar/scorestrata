//! Project-config validation checks (§22.2 `config` scope).
//!
//! The config plane is the cheapest validation plane: it never touches the
//! timeline or audio, so an agent can sanity-check a drafted config before
//! spending a compile or a render. Every check emits a `CONFIG_*` diagnostic
//! with a suggested patch, so the repair loop (§13.2) can fix it directly.

use scorestrata_core::diagnostic::{Diagnostic, MetricScope, SuggestedPatch};
use scorestrata_core::domain::Project;
use scorestrata_core::error::Severity;
use scorestrata_core::ids::DiagnosticId;

fn diagnostic(
    code: &str,
    metric: &str,
    severity: Severity,
    message: String,
    patch_op: &str,
    patch_params: serde_json::Value,
    confidence: f32,
) -> Diagnostic {
    Diagnostic {
        id: DiagnosticId::new(),
        code: code.to_string(),
        metric: metric.to_string(),
        severity,
        scope: MetricScope::default(),
        evidence_uris: vec!["project://config".to_string()],
        message,
        root_causes: vec!["project config violates the expected range".to_string()],
        suggested_patches: vec![SuggestedPatch {
            op: patch_op.to_string(),
            params: patch_params,
            expected_delta: Some("+0.20".to_string()),
            confidence,
        }],
        expected_delta: None,
        confidence,
    }
}

/// Validate the project config against sane musical-production ranges.
///
/// Checks:
/// - BPM within 40–200 (typical electronic-music production range)
/// - key parses into a real scale
/// - time signature numerator is 1–16 and denominator is non-zero
/// - a genre is declared
/// - the brief declares a delivery target (the loudness calibration anchor)
///
/// Returns every failing check as a `CONFIG_*` diagnostic; an empty vec
/// means the config is well-formed enough to proceed to arrangement.
#[must_use]
pub fn check_config(project: &Project) -> Vec<Diagnostic> {
    let mut diagnostics = Vec::new();

    let bpm = project.config.bpm;
    if !(40..=200).contains(&bpm) {
        diagnostics.push(diagnostic(
            "CONFIG_BPM_OUT_OF_RANGE",
            "config.bpm",
            Severity::Warning,
            format!("BPM {bpm} is outside the typical 40–200 production range"),
            "set_bpm",
            serde_json::json!({ "bpm": bpm.clamp(40, 200) }),
            0.9,
        ));
    }

    if scorestrata_theory::key::Key::parse(&project.config.key).is_none() {
        diagnostics.push(diagnostic(
            "CONFIG_KEY_UNPARSEABLE",
            "config.key",
            Severity::Warning,
            format!(
                "Key '{}' does not parse into a known scale",
                project.config.key
            ),
            "set_key",
            serde_json::json!({ "key": "C minor" }),
            0.95,
        ));
    }

    let ts = project.config.time_signature;
    if ts.numerator == 0 || ts.denominator == 0 || ts.numerator > 16 {
        diagnostics.push(diagnostic(
            "CONFIG_TIME_SIGNATURE_INVALID",
            "config.time_signature",
            Severity::Warning,
            format!(
                "Time signature {}/{} is implausible (numerator must be 1–16, denominator non-zero)",
                ts.numerator, ts.denominator
            ),
            "set_time_signature",
            serde_json::json!({ "time_signature": { "numerator": 4, "denominator": 4 } }),
            0.9,
        ));
    }

    if project.config.genre.trim().is_empty() {
        diagnostics.push(diagnostic(
            "CONFIG_GENRE_MISSING",
            "config.genre",
            Severity::Info,
            "No genre declared — genre profiles and calibration cannot apply".to_string(),
            "set_genre",
            serde_json::json!({ "genre": "techno" }),
            0.85,
        ));
    }

    if project.brief.delivery_target.is_none() {
        diagnostics.push(diagnostic(
            "CONFIG_DELIVERY_TARGET_MISSING",
            "config.delivery_target",
            Severity::Info,
            "No delivery target in the brief — loudness calibration has no anchor".to_string(),
            "set_delivery_target",
            serde_json::json!({ "delivery_target": "streaming" }),
            0.85,
        ));
    }

    diagnostics
}

#[cfg(test)]
mod tests {
    use super::*;
    use scorestrata_core::domain::{
        Arrangement, Brief, DeliveryTarget, Layer, ProjectConfig, Segment, SegmentRole,
        TimeSignature,
    };
    use scorestrata_core::ids::{LayerId, ProjectId, SegmentId};

    fn project_with_config(
        config: ProjectConfig,
        delivery_target: Option<DeliveryTarget>,
    ) -> Project {
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
                delivery_target,
                constraints: vec![],
                confidence: 0.5,
            },
            config,
            arrangement: Arrangement {
                total_bars: 8,
                time_signature: TimeSignature::default(),
                segments: vec![Segment {
                    id: SegmentId::new(),
                    name: "loop".to_string(),
                    role: SegmentRole::Chorus,
                    start_bar: 0,
                    bars: 8,
                    layers: vec![Layer {
                        id: LayerId::new(),
                        name: "drums".to_string(),
                        instrument: "drum_sampler".to_string(),
                        pattern_instances: Vec::new(),
                        register: None,
                        automation: vec![],
                        fx: None,
                        mix_gain_db: None,
                        mix_pan: None,
                        group: None,
                    }],
                }],
                energy_curve: Vec::new(),
            },
            tracks: vec![],
            patterns: vec![],
            spectral_reference: None,
            mixer: scorestrata_core::domain::MixerState::default(),
            sound_bank: None,
            bus_graph: None,
            status: scorestrata_core::domain::ProjectStatus::ConfigDrafted,
        }
    }

    #[test]
    fn valid_config_produces_no_diagnostics() {
        let project = project_with_config(
            ProjectConfig {
                name: "t".to_string(),
                genre: "techno".to_string(),
                bpm: 124,
                key: "C minor".to_string(),
                time_signature: TimeSignature::default(),
                duration_bars: 8,
                delivery_target: DeliveryTarget::Streaming,
                mood: vec![],
                references: vec![],
            },
            Some(DeliveryTarget::Streaming),
        );
        assert!(check_config(&project).is_empty());
    }

    #[test]
    fn bad_bpm_and_key_are_diagnosed_with_fixes() {
        let project = project_with_config(
            ProjectConfig {
                name: "t".to_string(),
                genre: "techno".to_string(),
                bpm: 320,
                key: "H flat".to_string(),
                time_signature: TimeSignature::default(),
                duration_bars: 8,
                delivery_target: DeliveryTarget::Streaming,
                mood: vec![],
                references: vec![],
            },
            Some(DeliveryTarget::Streaming),
        );
        let diagnostics = check_config(&project);
        let codes: Vec<&str> = diagnostics.iter().map(|d| d.code.as_str()).collect();
        assert!(codes.contains(&"CONFIG_BPM_OUT_OF_RANGE"));
        assert!(codes.contains(&"CONFIG_KEY_UNPARSEABLE"));
        // Both fixes must be surfaced so the repair loop can apply them.
        for d in &diagnostics {
            assert!(!d.suggested_patches.is_empty());
        }
    }

    #[test]
    fn missing_genre_and_target_are_informational() {
        let project = project_with_config(
            ProjectConfig {
                name: "t".to_string(),
                genre: String::new(),
                bpm: 124,
                key: "C minor".to_string(),
                time_signature: TimeSignature::default(),
                duration_bars: 8,
                delivery_target: DeliveryTarget::Streaming,
                mood: vec![],
                references: vec![],
            },
            None,
        );
        let diagnostics = check_config(&project);
        assert!(diagnostics.iter().any(|d| d.code == "CONFIG_GENRE_MISSING"));
        assert!(
            diagnostics
                .iter()
                .any(|d| d.code == "CONFIG_DELIVERY_TARGET_MISSING")
        );
    }
}
