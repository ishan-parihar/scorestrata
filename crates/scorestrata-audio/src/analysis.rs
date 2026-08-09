//! Audio analysis — Level 4 validation metrics (§11.4).
//!
//! Computes integrated loudness (LUFS), true peak (dBTP), clipping,
//! crest factor, spectral balance, and stereo correlation from raw audio
//! buffers. These metrics feed the validator's `audio_check` diagnostics
//! and the agent loop's mix repair decisions.
//!
//! All functions are pure and deterministic: given the same samples they
//! always produce the same values (per §17 determinism rules).

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
    /// Crest factor (ratio of peak to RMS, in dB).
    pub crest_factor: f32,
    /// Spectral balance (high/low frequency energy ratio).
    pub spectral_balance: f32,
    /// Stereo correlation (-1.0 to 1.0).
    pub stereo_correlation: f32,
    /// Number of clipped samples.
    pub clipped_sample_count: u32,
}

/// Analyze a mono buffer (used for stems) — stereo correlation is trivially 1.0.
#[must_use]
pub fn analyze_mono(samples: &[f32]) -> AudioAnalysis {
    analyze_stereo(samples, samples)
}

/// Analyze a stereo audio buffer (separate L/R sample slices).
///
/// Returns comprehensive Level-4 audio metrics. A silent or empty buffer
/// yields silence values (`-70 LUFS`, `-100 dBTP`, no clipping).
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

/// Integrated loudness using an RMS approximation of the K-weighted signal.
///
/// A full ITU-R BS.1770 implementation would apply K-weighting filters
/// before the mean-square; for the offline mixer this RMS-based estimate
/// is deterministic and adequate for repair-loop guidance.
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

/// True peak (sample peak in dBFS as a dBTP proxy) and clipping count.
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

/// Crest factor in dB: the ratio of peak to RMS.
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

/// Spectral balance: high-band energy over low-band energy.
///
/// A crude spectral proxy that splits the buffer into a low band (first
/// quarter) and a high band (the remaining three quarters). Deterministic
/// and cheap; suitable for detecting egregious brightness drift between
/// versions.
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

/// Pearson correlation between the left and right channels.
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

/// Frequency-band edges (Hz) shared by the audit module and genre
/// fingerprints: sub, bass, low-mid, mid, hi-mid, air.
pub const AUDIT_BAND_EDGES_HZ: [f32; 7] = [0.0, 100.0, 250.0, 500.0, 2000.0, 6000.0, 16000.0];

/// Human-readable labels for the six audit bands.
#[must_use]
pub fn band_labels() -> [&'static str; 6] {
    ["sub", "bass", "lo-mid", "mid", "hi-mid", "air"]
}

/// Expected spectral fingerprint of a genre (§5 of the audio plan).
///
/// Encodes the target band-energy distribution, spectral centroid, and
/// aliasing ceiling per genre so the audit gate can report deviation
/// (diagnostic `GENRE_FINGERPRINT_DRIFT`) and the agent loop can repair
/// *toward* genre.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct GenreFingerprint {
    /// Genre key this fingerprint describes.
    pub genre: &'static str,
    /// Expected share of energy in the six [`AUDIT_BAND_EDGES_HZ`] bands.
    pub band_energy_expected: [f32; 6],
    /// Expected spectral centroid in Hz.
    pub centroid_expected_hz: f32,
    /// Maximum acceptable energy share above 16 kHz (aliasing proxy).
    pub aliasing_max: f32,
}

impl GenreFingerprint {
    /// The fingerprint for a genre key, if one is defined.
    ///
    /// Matching is case-insensitive and substring-based (e.g. `techno` and
    /// `melodic_techno` both resolve to the techno fingerprint).
    #[must_use]
    pub fn for_genre(genre: &str) -> Option<&'static GenreFingerprint> {
        let lower = genre.to_lowercase();
        if lower.contains("house") {
            Some(&HOUSE_FINGERPRINT)
        } else if lower.contains("ambient") || lower.contains("drone") {
            Some(&AMBIENT_FINGERPRINT)
        } else if lower.contains("techno") || lower.contains("trance") {
            Some(&TECHNO_FINGERPRINT)
        } else if lower.contains("jazz") || lower.contains("swing") || lower.contains("bossa") {
            Some(&JAZZ_FINGERPRINT)
        } else if lower.contains("hip")
            || lower.contains("trap")
            || lower.contains("rap")
            || lower.contains("phonk")
        {
            Some(&HIPHOP_FINGERPRINT)
        } else if lower.contains("rock")
            || lower.contains("metal")
            || lower.contains("nu_metal")
            || lower.contains("punk")
        {
            Some(&ROCK_FINGERPRINT)
        } else if lower.contains("pop") {
            Some(&POP_FINGERPRINT)
        } else if lower.contains("funk") || lower.contains("soul") || lower.contains("disco") {
            Some(&FUNK_FINGERPRINT)
        } else if lower.contains("blues") {
            Some(&BLUES_FINGERPRINT)
        } else if lower.contains("synthwave")
            || lower.contains("retrowave")
            || lower.contains("vaporwave")
        {
            Some(&SYNTHWAVE_FINGERPRINT)
        } else if lower.contains("lo-fi") || lower.contains("lofi") {
            Some(&LOFI_FINGERPRINT)
        } else if lower.contains("cinematic") || lower.contains("orchestral") {
            Some(&CINEMATIC_FINGERPRINT)
        } else if lower.contains("drum and bass")
            || lower.contains("drum_and_bass")
            || lower.contains("dnb")
            || lower.contains("jungle")
        {
            Some(&DNB_FINGERPRINT)
        } else if lower.contains("classical") {
            Some(&CLASSICAL_FINGERPRINT)
        } else {
            None
        }
    }

    /// L1 deviation of an observed spectrum from this fingerprint.
    ///
    /// Returns 0.0 for an exact match and approaches 1.0 as the spectrum
    /// diverges. Band shape (mean absolute band-energy difference, halved to
    /// stay in 0..1) is weighted 0.6, centroid mismatch 0.3, and aliasing
    /// overshoot 0.1. A deviation above [`DRIFT_THRESHOLD`] is reported as
    /// `GENRE_FINGERPRINT_DRIFT` by the audit gate.
    #[must_use]
    pub fn deviation(&self, band_energy: &[f32; 6], centroid_hz: f32, aliasing_ratio: f32) -> f32 {
        let band_dev = band_energy
            .iter()
            .zip(self.band_energy_expected.iter())
            .map(|(actual, expected)| (actual - expected).abs())
            .sum::<f32>()
            / 2.0;
        let centroid_dev = if self.centroid_expected_hz > 0.0 {
            ((centroid_hz - self.centroid_expected_hz).abs() / self.centroid_expected_hz).min(1.0)
        } else {
            0.0
        };
        let alias_dev = if aliasing_ratio > self.aliasing_max {
            ((aliasing_ratio - self.aliasing_max) / 0.5).min(1.0)
        } else {
            0.0
        };
        band_dev.mul_add(0.6, centroid_dev.mul_add(0.3, alias_dev * 0.1))
    }
}

/// Deviation above this value triggers `GENRE_FINGERPRINT_DRIFT`.
pub const DRIFT_THRESHOLD: f32 = 0.30;

/// Techno: drums-dominant, bright — air and high-mid weighted.
const TECHNO_FINGERPRINT: GenreFingerprint = GenreFingerprint {
    genre: "melodic_techno",
    band_energy_expected: [0.12, 0.09, 0.08, 0.24, 0.16, 0.31],
    centroid_expected_hz: 6000.0,
    aliasing_max: 0.30,
};

/// House: balanced and mid-heavy.
const HOUSE_FINGERPRINT: GenreFingerprint = GenreFingerprint {
    genre: "house",
    band_energy_expected: [0.11, 0.13, 0.14, 0.22, 0.14, 0.26],
    centroid_expected_hz: 5000.0,
    aliasing_max: 0.25,
};

/// Ambient: pad-heavy low end, low centroid, minimal aliasing.
const AMBIENT_FINGERPRINT: GenreFingerprint = GenreFingerprint {
    genre: "ambient",
    band_energy_expected: [0.27, 0.24, 0.18, 0.22, 0.05, 0.04],
    centroid_expected_hz: 2000.0,
    aliasing_max: 0.12,
};

/// Jazz: mid-forward (horns/keys), warm low end, low aliasing.
const JAZZ_FINGERPRINT: GenreFingerprint = GenreFingerprint {
    genre: "jazz",
    band_energy_expected: [0.15, 0.18, 0.24, 0.26, 0.10, 0.07],
    centroid_expected_hz: 3500.0,
    aliasing_max: 0.12,
};

/// Hip-hop/trap: sub-heavy with a bright hat top — the 808 sandwich.
const HIPHOP_FINGERPRINT: GenreFingerprint = GenreFingerprint {
    genre: "hip-hop",
    band_energy_expected: [0.28, 0.16, 0.12, 0.16, 0.10, 0.18],
    centroid_expected_hz: 4200.0,
    aliasing_max: 0.20,
};

/// Rock/nu-metal: driven mid-band guitars, punchy low end.
const ROCK_FINGERPRINT: GenreFingerprint = GenreFingerprint {
    genre: "rock",
    band_energy_expected: [0.16, 0.16, 0.20, 0.24, 0.12, 0.12],
    centroid_expected_hz: 4000.0,
    aliasing_max: 0.25,
};

/// Pop: balanced full-range with a clear vocal mid.
const POP_FINGERPRINT: GenreFingerprint = GenreFingerprint {
    genre: "pop",
    band_energy_expected: [0.13, 0.15, 0.18, 0.26, 0.13, 0.15],
    centroid_expected_hz: 4500.0,
    aliasing_max: 0.20,
};

/// Funk: low-forward with a tight mid punch.
const FUNK_FINGERPRINT: GenreFingerprint = GenreFingerprint {
    genre: "funk",
    band_energy_expected: [0.20, 0.18, 0.16, 0.22, 0.12, 0.12],
    centroid_expected_hz: 3800.0,
    aliasing_max: 0.18,
};

/// Blues: warm mid-forward (guitar/harmonica), gentle top.
const BLUES_FINGERPRINT: GenreFingerprint = GenreFingerprint {
    genre: "blues",
    band_energy_expected: [0.18, 0.20, 0.22, 0.22, 0.10, 0.08],
    centroid_expected_hz: 3200.0,
    aliasing_max: 0.15,
};

/// Synthwave: driving 8th bass with a bright 16th arp top.
const SYNTHWAVE_FINGERPRINT: GenreFingerprint = GenreFingerprint {
    genre: "synthwave",
    band_energy_expected: [0.20, 0.14, 0.12, 0.20, 0.14, 0.20],
    centroid_expected_hz: 4800.0,
    aliasing_max: 0.22,
};

/// Lo-fi: warm low end, muffled top (vinyl), low centroid.
const LOFI_FINGERPRINT: GenreFingerprint = GenreFingerprint {
    genre: "lo-fi",
    band_energy_expected: [0.24, 0.20, 0.20, 0.22, 0.08, 0.06],
    centroid_expected_hz: 2800.0,
    aliasing_max: 0.10,
};

/// Cinematic: wide pads, low-forward, minimal aliasing.
const CINEMATIC_FINGERPRINT: GenreFingerprint = GenreFingerprint {
    genre: "cinematic",
    band_energy_expected: [0.22, 0.20, 0.18, 0.24, 0.08, 0.08],
    centroid_expected_hz: 3000.0,
    aliasing_max: 0.12,
};

/// Drum-and-bass: sub-forward with a bright breakbeat top.
const DNB_FINGERPRINT: GenreFingerprint = GenreFingerprint {
    genre: "drum_and_bass",
    band_energy_expected: [0.24, 0.14, 0.12, 0.18, 0.12, 0.20],
    centroid_expected_hz: 5000.0,
    aliasing_max: 0.25,
};

/// Classical: dynamic low-mid pads, gentle high end.
const CLASSICAL_FINGERPRINT: GenreFingerprint = GenreFingerprint {
    genre: "classical",
    band_energy_expected: [0.16, 0.20, 0.22, 0.24, 0.10, 0.08],
    centroid_expected_hz: 3000.0,
    aliasing_max: 0.10,
};

/// Apply a linear gain (from a dB value) to a buffer in place.
///
/// Used by `normalize_loudness_target` to move a render toward a target
/// LUFS. Gain is clamped to a sane range to avoid NaN/infinity.
pub fn apply_gain_db(samples: &mut [f32], gain_db: f32) {
    let gain = 10.0f32.powf(gain_db / 20.0).clamp(0.01, 100.0);
    for sample in samples.iter_mut() {
        *sample = (*sample * gain).clamp(-1.0, 1.0);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn s3_genre_fingerprints_cover_all_families() {
        // Every S3 genre family must resolve a fingerprint so the audit gate
        // can score its spectral deviation against a genre-appropriate
        // profile — not silently fall back to "no fingerprint".
        let genres = [
            "house",
            "ambient",
            "techno",
            "trance",
            "jazz",
            "swing",
            "hip-hop",
            "trap",
            "rock",
            "nu_metal",
            "pop",
            "funk",
            "blues",
            "synthwave",
            "lo-fi",
            "cinematic",
            "drum_and_bass",
            "classical",
        ];
        for genre in genres {
            assert!(
                GenreFingerprint::for_genre(genre).is_some(),
                "genre '{genre}' must have a fingerprint"
            );
        }
    }

    #[test]
    fn s3_fingerprints_are_distinct() {
        // Distinct genre profiles must not be identical — otherwise the
        // deviation gate cannot tell a jazz render from a techno one.
        let pairs = [
            ("jazz", "techno"),
            ("hip-hop", "ambient"),
            ("rock", "lo-fi"),
            ("pop", "blues"),
            ("synthwave", "classical"),
            ("drum_and_bass", "cinematic"),
        ];
        for (a, b) in pairs {
            let fa = GenreFingerprint::for_genre(a).expect("a");
            let fb = GenreFingerprint::for_genre(b).expect("b");
            assert_ne!(
                fa.band_energy_expected, fb.band_energy_expected,
                "'{a}' and '{b}' must have distinct band profiles"
            );
            assert!(
                (fa.centroid_expected_hz - fb.centroid_expected_hz).abs() > 100.0,
                "'{a}' and '{b}' must differ in expected centroid"
            );
        }
    }

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
        assert!(analysis.true_peak_dbtp > -0.001);
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
    fn analysis_is_deterministic() {
        let mut left = vec![0.0; 512];
        let mut right = vec![0.0; 512];
        for i in 0..512 {
            #[expect(clippy::cast_precision_loss, reason = "Test index conversion")]
            let t = i as f32 * 0.01;
            left[i] = t.sin();
            right[i] = (t * 1.5).cos() * 0.8;
        }
        let a = analyze_stereo(&left, &right);
        let b = analyze_stereo(&left, &right);
        assert_eq!(a.integrated_lufs.to_bits(), b.integrated_lufs.to_bits());
        assert_eq!(a.true_peak_dbtp.to_bits(), b.true_peak_dbtp.to_bits());
    }

    #[test]
    fn apply_gain_changes_loudness() {
        #[expect(clippy::cast_precision_loss, reason = "Test index conversion")]
        let mut samples: Vec<f32> = (0..1000).map(|i| (i as f32 * 0.01).sin() * 0.5).collect();
        let before = analyze_mono(&samples);
        apply_gain_db(&mut samples, 6.0);
        let after = analyze_mono(&samples);
        assert!(after.integrated_lufs > before.integrated_lufs);
        assert!(after.true_peak_dbtp > before.true_peak_dbtp);
    }

    #[test]
    fn apply_gain_clamps_to_full_scale() {
        let mut samples = vec![0.9f32; 100];
        apply_gain_db(&mut samples, 20.0);
        assert!(samples.iter().all(|s| s.abs() <= 1.0));
    }

    #[test]
    fn analyze_mono_matches_stereo_of_duplicate() {
        #[expect(clippy::cast_precision_loss, reason = "Test index conversion")]
        let samples: Vec<f32> = (0..1000).map(|i| (i as f32 * 0.01).sin() * 0.4).collect();
        let mono = analyze_mono(&samples);
        let stereo = analyze_stereo(&samples, &samples);
        assert_eq!(
            mono.integrated_lufs.to_bits(),
            stereo.integrated_lufs.to_bits()
        );
        assert!((mono.stereo_correlation - 1.0).abs() < 1e-6);
    }
}
