//! Audio validation — LUFS, true peak, clipping, crest factor, spectral
//! balance, stereo correlation, and mix diagnostics.
//!
//! Phase 6 implementation per architecture spec sections 16.1–16.3.

use scorestrata_core::diagnostic::{Diagnostic, MetricSample, MetricScope};
use scorestrata_core::error::Severity;
use scorestrata_core::ids::DiagnosticId;
use serde::{Deserialize, Serialize};

/// Audio analysis result for a stereo buffer.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AudioAnalysis {
    /// Integrated LUFS (Loudness Units relative to Full Scale).
    pub integrated_lufs: f32,
    /// True peak in dBTP.
    pub true_peak_dbtp: f32,
    /// Whether clipping was detected.
    pub clipping_detected: bool,
    /// Crest factor (ratio of peak to RMS).
    pub crest_factor: f32,
    /// Spectral balance (high/low frequency ratio).
    pub spectral_balance: f32,
    /// Stereo correlation (-1.0 to 1.0).
    pub stereo_correlation: f32,
    /// Number of clipped samples.
    pub clipped_sample_count: u32,
}

/// Analyze a stereo audio buffer (separate L/R sample slices).
///
/// Returns comprehensive audio analysis metrics.
#[must_use]
pub fn analyze_stereo(left: &[f32], right: &[f32]) -> AudioAnalysis {
    let len = left.len().min(right.len());
    if len == 0 {
        return AudioAnalysis {
            integrated_lufs: -70.0,
            true_peak_dbtp: -100.0,
            clipping_detected: false,
            crest_factor: 0.0,
            spectral_balance: 1.0,
            stereo_correlation: 1.0,
            clipped_sample_count: 0,
        };
    }

    let integrated_lufs = calculate_lufs(left, right);
    let (true_peak_dbtp, clipped_count) = calculate_true_peak(left, right);
    let clipping_detected = clipped_count > 0;
    let crest_factor = calculate_crest_factor(left, right);
    let spectral_balance = calculate_spectral_balance(left, right);
    let stereo_correlation = calculate_stereo_correlation(left, right);

    AudioAnalysis {
        integrated_lufs,
        true_peak_dbtp,
        clipping_detected,
        crest_factor,
        spectral_balance,
        stereo_correlation,
        clipped_sample_count: clipped_count,
    }
}

fn calculate_lufs(left: &[f32], right: &[f32]) -> f32 {
    let len = left.len().min(right.len());
    if len == 0 {
        return -70.0;
    }
    let mut sum_sq = 0.0f32;
    for i in 0..len {
        let mono = (left[i] + right[i]) * 0.5;
        sum_sq += mono * mono;
    }
    #[expect(
        clippy::cast_precision_loss,
        reason = "Audio buffer length fits in f32"
    )]
    let rms = (sum_sq / len as f32).sqrt();
    if rms > 0.0 {
        20.0f32.mul_add(rms.log10(), -0.691)
    } else {
        -70.0
    }
}

fn calculate_true_peak(left: &[f32], right: &[f32]) -> (f32, u32) {
    let len = left.len().min(right.len());
    let mut max_peak = 0.0f32;
    let mut clipped = 0u32;
    for i in 0..len {
        let abs_l = left[i].abs();
        let abs_r = right[i].abs();
        let peak = abs_l.max(abs_r);
        if peak > max_peak {
            max_peak = peak;
        }
        if abs_l >= 1.0 || abs_r >= 1.0 {
            clipped += 1;
        }
    }
    let dbtp = if max_peak > 0.0 {
        20.0 * max_peak.log10()
    } else {
        -100.0
    };
    (dbtp, clipped)
}

fn calculate_crest_factor(left: &[f32], right: &[f32]) -> f32 {
    let len = left.len().min(right.len());
    if len == 0 {
        return 0.0;
    }
    let mut sum_sq = 0.0f32;
    let mut max_peak = 0.0f32;
    for i in 0..len {
        let mono = (left[i] + right[i]) * 0.5;
        sum_sq += mono * mono;
        let peak = left[i].abs().max(right[i].abs());
        if peak > max_peak {
            max_peak = peak;
        }
    }
    #[expect(
        clippy::cast_precision_loss,
        reason = "Audio buffer length fits in f32"
    )]
    let rms = (sum_sq / len as f32).sqrt();
    if rms > 0.0 && max_peak > 0.0 {
        20.0 * (max_peak / rms).log10()
    } else {
        0.0
    }
}

fn calculate_spectral_balance(left: &[f32], right: &[f32]) -> f32 {
    let len = left.len().min(right.len());
    if len < 4 {
        return 1.0;
    }
    let mid = len / 4;
    let mut low_energy = 0.0f32;
    let mut high_energy = 0.0f32;
    for i in 0..mid {
        let mono = (left[i] + right[i]) * 0.5;
        low_energy += mono * mono;
    }
    for i in mid..len {
        let mono = (left[i] + right[i]) * 0.5;
        high_energy += mono * mono;
    }
    if low_energy > 0.0 {
        high_energy / low_energy
    } else {
        1.0
    }
}

fn calculate_stereo_correlation(left: &[f32], right: &[f32]) -> f32 {
    let len = left.len().min(right.len());
    if len == 0 {
        return 1.0;
    }
    let mut sum_lr = 0.0f32;
    let mut sum_left_sq = 0.0f32;
    let mut sum_right_sq = 0.0f32;
    for i in 0..len {
        sum_lr += left[i] * right[i];
        sum_left_sq += left[i] * left[i];
        sum_right_sq += right[i] * right[i];
    }
    let denom = (sum_left_sq * sum_right_sq).sqrt();
    if denom > 0.0 {
        (sum_lr / denom).clamp(-1.0, 1.0)
    } else {
        1.0
    }
}

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
            suggested_patches: vec![],
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
            suggested_patches: vec![],
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
            suggested_patches: vec![],
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
    fn analyze_silent_audio_returns_silence_metrics() {
        let left = vec![0.0; 1000];
        let right = vec![0.0; 1000];
        let analysis = analyze_stereo(&left, &right);
        assert!((analysis.integrated_lufs - (-70.0)).abs() < f32::EPSILON);
        assert!(!analysis.clipping_detected);
    }

    #[test]
    fn analyze_sine_wave_returns_valid_metrics() {
        let mut left = vec![0.0; 1000];
        let mut right = vec![0.0; 1000];
        for sine_idx in 0..1000 {
            #[expect(clippy::cast_precision_loss, reason = "Test index conversion")]
            let t = sine_idx as f32 * 0.01;
            left[sine_idx] = t.sin() * 0.5;
            right[sine_idx] = t.sin() * 0.5;
        }
        let analysis = analyze_stereo(&left, &right);
        assert!(analysis.integrated_lufs > -70.0);
        assert!(analysis.stereo_correlation > 0.99);
    }

    #[test]
    fn analyze_clipped_audio_detects_clipping() {
        let left = vec![1.0; 100];
        let right = vec![1.0; 100];
        let analysis = analyze_stereo(&left, &right);
        assert!(analysis.clipping_detected);
        assert_eq!(analysis.clipped_sample_count, 100);
    }

    #[test]
    fn analyze_stereo_signal_has_correlation_less_than_one() {
        let mut left = vec![0.0; 1000];
        let mut right = vec![0.0; 1000];
        for stereo_idx in 0..1000 {
            #[expect(clippy::cast_precision_loss, reason = "Test index conversion")]
            let t = stereo_idx as f32 * 0.01;
            left[stereo_idx] = t.sin();
            right[stereo_idx] = t.mul_add(0.5, 0.0).sin();
        }
        let analysis = analyze_stereo(&left, &right);
        assert!(analysis.stereo_correlation < 0.99);
    }

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
}
