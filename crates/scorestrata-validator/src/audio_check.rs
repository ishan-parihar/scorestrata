//! Audio validation — LUFS, true peak, clipping, crest factor, spectral
//! balance, stereo correlation, and mix diagnostics.
//!
//! Phase 6 implementation per architecture spec sections 16.1–16.3.
//!
//! The DSP itself lives in [`scorestrata_audio::analysis`]; this module
//! converts an [`AudioAnalysis`] into validator [`Diagnostic`]s and
//! scorecard [`MetricSample`]s.

pub use scorestrata_audio::analysis::{AudioAnalysis, analyze_mono, analyze_stereo};

use scorestrata_core::diagnostic::{Diagnostic, MetricSample, MetricScope};
use scorestrata_core::error::Severity;
use scorestrata_core::ids::DiagnosticId;

/// Generate diagnostics from audio analysis.
#[must_use]
#[expect(clippy::too_many_lines, reason = "Audio diagnostic generation")]
pub fn generate_audio_diagnostics(analysis: &AudioAnalysis) -> Vec<Diagnostic> {
    let mut diagnostics = Vec::new();

    if analysis.integrated_lufs > -1.0 {
        diagnostics.push(Diagnostic {
            id: DiagnosticId::new(),
            code: "AUDIO_LUFS_TOO_LOUD".to_string(),
            metric: "audio.integrated_lufs".to_string(),
            severity: Severity::Error,
            message: format!(
                "Integrated LUFS ({:.1}) exceeds -1.0 LUFS target",
                analysis.integrated_lufs
            ),
            scope: MetricScope::default(),
            evidence_uris: vec!["project://audio/lufs".to_string()],
            root_causes: vec![],
            suggested_patches: vec![scorestrata_core::diagnostic::SuggestedPatch {
                op: "normalize_loudness_target".to_string(),
                params: serde_json::json!({"target_lufs": -14.0}),
                expected_delta: Some("+0.2".to_string()),
                confidence: 0.9,
            }],
            expected_delta: None,
            confidence: 0.95,
        });
    } else if analysis.integrated_lufs < -24.0 {
        diagnostics.push(Diagnostic {
            id: DiagnosticId::new(),
            code: "AUDIO_LUFS_TOO_QUIET".to_string(),
            metric: "audio.integrated_lufs".to_string(),
            severity: Severity::Warning,
            message: format!(
                "Integrated LUFS ({:.1}) is below -24.0 LUFS",
                analysis.integrated_lufs
            ),
            scope: MetricScope::default(),
            evidence_uris: vec!["project://audio/lufs".to_string()],
            root_causes: vec![],
            suggested_patches: vec![scorestrata_core::diagnostic::SuggestedPatch {
                op: "normalize_loudness_target".to_string(),
                params: serde_json::json!({"target_lufs": -14.0}),
                expected_delta: Some("+0.15".to_string()),
                confidence: 0.9,
            }],
            expected_delta: None,
            confidence: 0.9,
        });
    }

    if analysis.true_peak_dbtp > -1.0 {
        diagnostics.push(Diagnostic {
            id: DiagnosticId::new(),
            code: "AUDIO_TRUE_PEAK_EXCEEDED".to_string(),
            metric: "audio.true_peak_dbtp".to_string(),
            severity: Severity::Error,
            message: format!(
                "True peak ({:.1} dBTP) exceeds -1.0 dBTP limit",
                analysis.true_peak_dbtp
            ),
            scope: MetricScope::default(),
            evidence_uris: vec!["project://audio/true_peak".to_string()],
            root_causes: vec![],
            suggested_patches: vec![scorestrata_core::diagnostic::SuggestedPatch {
                op: "normalize_loudness_target".to_string(),
                params: serde_json::json!({"target_lufs": -14.0}),
                expected_delta: Some("+0.3".to_string()),
                confidence: 0.95,
            }],
            expected_delta: None,
            confidence: 0.99,
        });
    }

    if analysis.clipping_detected {
        diagnostics.push(Diagnostic {
            id: DiagnosticId::new(),
            code: "AUDIO_CLIPPING_DETECTED".to_string(),
            metric: "audio.clipping".to_string(),
            severity: Severity::Error,
            message: format!(
                "Clipping detected: {} samples at or above 0 dBFS",
                analysis.clipped_sample_count
            ),
            scope: MetricScope::default(),
            evidence_uris: vec!["project://audio/clipping".to_string()],
            root_causes: vec![],
            suggested_patches: vec![],
            expected_delta: None,
            confidence: 1.0,
        });
    }

    if analysis.crest_factor < 3.0 {
        diagnostics.push(Diagnostic {
            id: DiagnosticId::new(),
            code: "AUDIO_LOW_CREST_FACTOR".to_string(),
            metric: "audio.crest_factor".to_string(),
            severity: Severity::Warning,
            message: format!(
                "Crest factor ({:.1} dB) is low — may indicate over-compression",
                analysis.crest_factor
            ),
            scope: MetricScope::default(),
            evidence_uris: vec!["project://audio/crest_factor".to_string()],
            root_causes: vec![],
            suggested_patches: vec![],
            expected_delta: None,
            confidence: 0.85,
        });
    }

    if analysis.stereo_correlation > 0.95 {
        diagnostics.push(Diagnostic {
            id: DiagnosticId::new(),
            code: "AUDIO_NARROW_STEREO".to_string(),
            metric: "audio.stereo_correlation".to_string(),
            severity: Severity::Warning,
            message: format!(
                "Stereo correlation ({:.2}) is very high — mix may be too narrow",
                analysis.stereo_correlation
            ),
            scope: MetricScope::default(),
            evidence_uris: vec!["project://audio/stereo_correlation".to_string()],
            root_causes: vec![],
            suggested_patches: vec![],
            expected_delta: None,
            confidence: 0.8,
        });
    } else if analysis.stereo_correlation < 0.0 {
        diagnostics.push(Diagnostic {
            id: DiagnosticId::new(),
            code: "AUDIO_PHASE_ISSUES".to_string(),
            metric: "audio.stereo_correlation".to_string(),
            severity: Severity::Error,
            message: format!(
                "Stereo correlation ({:.2}) is negative — possible phase cancellation",
                analysis.stereo_correlation
            ),
            scope: MetricScope::default(),
            evidence_uris: vec!["project://audio/stereo_correlation".to_string()],
            root_causes: vec![],
            suggested_patches: vec![],
            expected_delta: None,
            confidence: 0.9,
        });
    }

    diagnostics
}

/// Generate metric samples from audio analysis for the scorecard.
#[must_use]
pub fn generate_audio_metrics(analysis: &AudioAnalysis) -> Vec<MetricSample> {
    let mut metrics = Vec::new();

    let lufs_score = map_lufs_to_score(analysis.integrated_lufs);
    metrics.push(MetricSample {
        id: scorestrata_core::ids::MetricId::new(),
        name: "audio.integrated_lufs".to_string(),
        value: lufs_score,
        severity: if lufs_score < 0.5 {
            Severity::Error
        } else if lufs_score < 0.8 {
            Severity::Warning
        } else {
            Severity::Info
        },
        scope: MetricScope::default(),
        evidence_uri: "project://audio/lufs".to_string(),
        explanation: format!(
            "Integrated LUFS: {:.1} (target: -14.0 LUFS)",
            analysis.integrated_lufs
        ),
        confidence: 0.95,
        suggested_patches: vec![],
    });

    let peak_score = map_peak_to_score(analysis.true_peak_dbtp);
    metrics.push(MetricSample {
        id: scorestrata_core::ids::MetricId::new(),
        name: "audio.true_peak_dbtp".to_string(),
        value: peak_score,
        severity: if analysis.true_peak_dbtp > -1.0 {
            Severity::Error
        } else if analysis.true_peak_dbtp > -3.0 {
            Severity::Warning
        } else {
            Severity::Info
        },
        scope: MetricScope::default(),
        evidence_uri: "project://audio/true_peak".to_string(),
        explanation: format!(
            "True peak: {:.1} dBTP (limit: -1.0 dBTP)",
            analysis.true_peak_dbtp
        ),
        confidence: 0.99,
        suggested_patches: vec![],
    });

    metrics.push(MetricSample {
        id: scorestrata_core::ids::MetricId::new(),
        name: "audio.clipping".to_string(),
        value: if analysis.clipping_detected { 0.0 } else { 1.0 },
        severity: if analysis.clipping_detected {
            Severity::Error
        } else {
            Severity::Info
        },
        scope: MetricScope::default(),
        evidence_uri: "project://audio/clipping".to_string(),
        explanation: format!("Clipped samples: {}", analysis.clipped_sample_count),
        confidence: 1.0,
        suggested_patches: vec![],
    });

    let crest_score = map_crest_to_score(analysis.crest_factor);
    metrics.push(MetricSample {
        id: scorestrata_core::ids::MetricId::new(),
        name: "audio.crest_factor".to_string(),
        value: crest_score,
        severity: if analysis.crest_factor < 3.0 {
            Severity::Warning
        } else {
            Severity::Info
        },
        scope: MetricScope::default(),
        evidence_uri: "project://audio/crest_factor".to_string(),
        explanation: format!("Crest factor: {:.1} dB", analysis.crest_factor),
        confidence: 0.9,
        suggested_patches: vec![],
    });

    metrics.push(MetricSample {
        id: scorestrata_core::ids::MetricId::new(),
        name: "audio.stereo_correlation".to_string(),
        value: f32::midpoint(analysis.stereo_correlation, 1.0),
        severity: if analysis.stereo_correlation > 0.95 {
            Severity::Warning
        } else if analysis.stereo_correlation < 0.0 {
            Severity::Error
        } else {
            Severity::Info
        },
        scope: MetricScope::default(),
        evidence_uri: "project://audio/stereo_correlation".to_string(),
        explanation: format!("Stereo correlation: {:.2}", analysis.stereo_correlation),
        confidence: 0.95,
        suggested_patches: vec![],
    });

    metrics
}

fn map_lufs_to_score(lufs: f32) -> f32 {
    let target = -14.0;
    let diff = (lufs - target).abs();
    (1.0 - diff / 10.0).clamp(0.0, 1.0)
}

fn map_peak_to_score(peak_dbtp: f32) -> f32 {
    if peak_dbtp < -3.0 {
        1.0
    } else if peak_dbtp < -1.0 {
        0.5 + (peak_dbtp + 3.0) / 4.0
    } else {
        0.0
    }
}

fn map_crest_to_score(crest: f32) -> f32 {
    if crest >= 6.0 {
        1.0
    } else if crest >= 3.0 {
        0.5 + (crest - 3.0) / 6.0
    } else {
        crest / 6.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn generate_diagnostics_for_clipped_audio() {
        let left = vec![1.0; 100];
        let right = vec![1.0; 100];
        let analysis = analyze_stereo(&left, &right);
        let diagnostics = generate_audio_diagnostics(&analysis);
        assert!(
            diagnostics
                .iter()
                .any(|d| d.code == "AUDIO_CLIPPING_DETECTED")
        );
    }

    #[test]
    fn generate_metrics_for_sine_wave() {
        let mut left = vec![0.0; 1000];
        let mut right = vec![0.0; 1000];
        for gen_idx in 0..1000 {
            #[expect(clippy::cast_precision_loss, reason = "Test index conversion")]
            let t = gen_idx as f32 * 0.01;
            left[gen_idx] = t.sin() * 0.5;
            right[gen_idx] = t.sin() * 0.5;
        }
        let analysis = analyze_stereo(&left, &right);
        let metrics = generate_audio_metrics(&analysis);
        assert!(metrics.len() >= 5);
    }

    #[test]
    fn loud_render_suggests_normalize_patch() {
        let left = vec![0.9; 1000];
        let right = vec![0.9; 1000];
        let analysis = analyze_stereo(&left, &right);
        let diagnostics = generate_audio_diagnostics(&analysis);
        assert!(diagnostics.iter().any(|d| {
            d.suggested_patches
                .iter()
                .any(|p| p.op == "normalize_loudness_target")
        }));
    }
}
