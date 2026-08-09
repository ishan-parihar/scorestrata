//! Reference-driven debug loop (Phase E).
//!
//! A producer loads a commercial track (`load_reference_track`), the engine
//! analyzes it into a [`ReferenceProfile`] (6-band energy shares, LUFS, true
//! peak, spectral centroid), and every subsequent render can be compared
//! against it layer-by-layer (`compare_to_reference`). [`extract_groove`]
//! derives a swing map from the reference's transient timing (Ableton
//! "Extract Groove" semantics) that feeds the Phase C humanize layer.
//!
//! All analysis is the same deterministic STFT/LUFS code the audit uses, so
//! reference and render are compared on identical footing.

// Onset/flux math casts bounded sample indices and rates to f32; audio-range
// values are exactly representable.
#![expect(
    clippy::cast_precision_loss,
    reason = "Reference analysis casts bounded sample indices/rates which are exactly representable in f32"
)]

use serde::{Deserialize, Serialize};

use crate::analysis::{AUDIT_BAND_EDGES_HZ, analyze_stereo};
use crate::audit::audit_stereo;
use crate::error::AudioError;

/// Spectral target extracted from a user WAV (plan §3.2 "Reference profile").
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ReferenceProfile {
    /// Where the reference came from (path or "inline:base64").
    pub path: String,
    /// Sample rate of the decoded reference.
    pub sample_rate: u32,
    /// Six band-energy *shares* (0..1, aligned with `AUDIT_BAND_EDGES_HZ`).
    pub bands: [f32; 6],
    /// Integrated loudness in LUFS.
    pub lufs: f32,
    /// True peak in dBTP.
    pub true_peak_db: f32,
    /// Spectral centroid in Hz.
    pub centroid_hz: f32,
}

/// Per-layer (or master) deviation from the reference profile.
///
/// `band_deltas` are signed energy-share differences (reference − measured),
/// so a positive value means the render is *under* the reference in that
/// band and a negative value means it is over (e.g. a muddy low end shows up
/// as a negative delta in the low band).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ReferenceDeviation {
    /// The audited entity (layer name or `master`).
    pub entity: String,
    /// Signed per-band share deltas (reference − measured).
    pub band_deltas: [f32; 6],
    /// Loudness delta in LU (reference − measured).
    pub lufs_delta: f32,
    /// Centroid delta in Hz (reference − measured).
    pub centroid_delta_hz: f32,
    /// Normalized overall deviation 0..1 (mean absolute band delta +
    /// normalized loudness/centroid drift).
    pub overall: f32,
    /// Human-readable diagnosis of the worst band.
    pub diagnosis: String,
}

/// Analyze a decoded reference WAV into a [`ReferenceProfile`].
///
/// # Errors
///
/// Returns an error when the bytes are not a decodable WAV.
pub fn analyze_reference_wav(
    bytes: &[u8],
    path: impl Into<String>,
) -> Result<ReferenceProfile, AudioError> {
    let (left, right, sample_rate) = crate::wav::read_wav_stereo(bytes)?;
    let report = audit_stereo(&left, &right, sample_rate, None);
    let analysis = analyze_stereo(&left, &right);
    Ok(ReferenceProfile {
        path: path.into(),
        sample_rate,
        bands: report.band_shares(),
        lufs: analysis.integrated_lufs,
        true_peak_db: analysis.true_peak_dbtp,
        centroid_hz: report.spectral_centroid_hz,
    })
}

/// Compare a measured band/LUFS/centroid set against a reference profile.
///
/// `entity` labels the result (a layer name or `master`). The overall score
/// weights the mean absolute band-share delta (0..1 scale) plus normalized
/// loudness and centroid drift, so 0.0 = identical and ~1.0 = wildly off.
/// Pass `lufs = None` for entities without a loudness measurement (layers);
/// the loudness term is then omitted from the overall score.
#[must_use]
pub fn compare_measured(
    entity: &str,
    bands: [f32; 6],
    lufs: Option<f32>,
    centroid_hz: f32,
    reference: &ReferenceProfile,
) -> ReferenceDeviation {
    let mut band_deltas = [0.0f32; 6];
    let mut worst_idx = 0usize;
    let mut worst_mag = 0.0f32;
    for (i, slot) in band_deltas.iter_mut().enumerate() {
        let delta = reference.bands[i] - bands[i];
        *slot = delta;
        if delta.abs() > worst_mag {
            worst_mag = delta.abs();
            worst_idx = i;
        }
    }
    let mean_band = band_deltas.iter().map(|d| d.abs()).sum::<f32>() / 6.0;
    let lufs_delta = lufs.map_or(0.0, |l| reference.lufs - l);
    let centroid_delta_hz = reference.centroid_hz - centroid_hz;
    // Normalize loudness drift: 12 LU is a very large difference (scale 1).
    let lufs_norm = (lufs_delta / 12.0).clamp(-1.0, 1.0).abs();
    // Normalize centroid drift against a ~5 kHz reference span.
    let centroid_norm = (centroid_delta_hz / 5_000.0).clamp(-1.0, 1.0).abs();
    let overall = if lufs.is_some() {
        (mean_band * 2.0 + lufs_norm * 0.3 + centroid_norm * 0.3).clamp(0.0, 1.0)
    } else {
        (mean_band * 2.0 + centroid_norm * 0.3).clamp(0.0, 1.0)
    };
    let labels = crate::analysis::band_labels();
    let diagnosis = format!(
        "{} band {} {:.0}% {} reference (Δ share {:.3}); centroid Δ {:.0} Hz, LUFS Δ {:.1}",
        if band_deltas[worst_idx] >= 0.0 {
            "under"
        } else {
            "over"
        },
        labels[worst_idx],
        (band_deltas[worst_idx].abs() * 100.0),
        "vs",
        band_deltas[worst_idx],
        centroid_delta_hz,
        lufs_delta,
    );
    ReferenceDeviation {
        entity: entity.to_string(),
        band_deltas,
        lufs_delta,
        centroid_delta_hz,
        overall,
        diagnosis,
    }
}

/// Groove map extracted from a reference WAV's transient timing.
///
/// Emulates Ableton's "Extract Groove": onsets are detected by short-window
/// energy flux, the 16th-note grid is inferred from the dominant inter-onset
/// interval, and each pulse's deviation from the rigid grid is reported.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GrooveExtraction {
    /// Inferred 16th-note pulse length in ms.
    pub pulse_ms: f32,
    /// Per-pulse timing deviation (signed fraction of the pulse, late = +).
    pub pulse_offsets: Vec<f32>,
    /// Swing amount 0.0–1.0 (0 = straight, 0.66 ≈ classic swing-16).
    pub swing: f32,
    /// Closest built-in template: `straight`, `swing16`, `laid_back`, `driving`.
    pub template: String,
}

/// Extract a groove map from a decoded reference WAV.
///
/// `bpm` anchors the 16th grid; when absent (0) it is inferred from the
/// dominant onset interval. The returned [`GrooveExtraction::swing`] feeds
/// `set_humanize(groove = "swing16", groove_amount = swing)`.
///
/// # Errors
///
/// Returns an error when the bytes are not a decodable WAV or no onsets are
/// detected.
pub fn extract_groove(bytes: &[u8], bpm: u32) -> Result<GrooveExtraction, AudioError> {
    let (left, right, sample_rate) = crate::wav::read_wav_stereo(bytes)?;
    let mono: Vec<f32> = left
        .iter()
        .zip(&right)
        .map(|(l, r)| (l + r) * 0.5)
        .collect();
    // Energy flux onsets: fine hop (128 ≈ 2.7 ms @ 48 kHz) so swing shifts
    // well below 10 ms stay measurable, frame 512.
    let hop = 128usize;
    let mut flux: Vec<(f32, f32)> = Vec::new(); // (time_s, onset_strength)
    let mut prev_energy = 0.0f32;
    let mut frame = 0usize;
    while frame + 512 <= mono.len() {
        let energy: f32 = mono[frame..frame + 512].iter().map(|s| s * s).sum::<f32>();
        let flux_now = (energy - prev_energy).max(0.0);
        prev_energy = energy;
        if flux_now > 0.0 {
            let t = frame as f32 / sample_rate as f32;
            flux.push((t, flux_now));
        }
        frame += hop;
    }
    if flux.is_empty() {
        return Err(AudioError::Other(
            "no onsets detected in reference".to_string(),
        ));
    }
    // Keep only the strongest ~50% of frames as onsets.
    let mut strengths: Vec<f32> = flux.iter().map(|(_, s)| *s).collect();
    strengths.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    let threshold = strengths[(strengths.len() * 5) / 10];
    // Refine each onset to sample accuracy: the energy centroid of the
    // transient region (coarse flux frames land up to a window early).
    let onsets: Vec<f32> = flux
        .into_iter()
        .filter(|(_, s)| *s >= threshold)
        .map(|(t, _)| refine_onset(t, &mono, sample_rate))
        .collect();

    // Dominant inter-onset interval → pulse guess.
    let mut intervals: Vec<f32> = onsets
        .windows(2)
        .map(|w| w[1] - w[0])
        .filter(|d| *d > 0.03)
        .collect();
    intervals.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    let pulse_ms = if bpm > 0 {
        60_000.0 / bpm as f32 / 4.0
    } else {
        intervals.first().map_or(166.7, |d| d * 1000.0)
    };
    // Measure each onset's deviation from its 16th grid point (fraction of a
    // pulse). Straight beats sit near 0; swung off-beats sit ~1/3 of a pulse
    // late — this is immune to grid-phase alignment errors from quantization.
    let pulse_seconds = pulse_ms / 1000.0;
    let mut pulse_offsets: Vec<f32> = Vec::new();
    for t in &onsets {
        let frac = (t / pulse_seconds) % 1.0;
        let dev = if frac > 0.5 { frac - 1.0 } else { frac };
        pulse_offsets.push(dev);
    }
    pulse_offsets.truncate(64);
    // Swing amount = twice the *median* late deviation (off-beats shift up to
    // half a pulse late, so 0.66 ≈ classic swing-16); the median is robust to
    // a few stray onsets.
    let mut late: Vec<f32> = pulse_offsets
        .iter()
        .copied()
        .filter(|d| *d > 0.03)
        .collect();
    late.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    let swing = if late.is_empty() {
        0.0
    } else {
        (2.0 * late[late.len() / 2]).clamp(0.0, 1.0)
    };
    let template = if swing < 0.05 {
        "straight".to_string()
    } else if swing > 0.55 {
        "swing16".to_string()
    } else {
        "laid_back".to_string()
    };
    Ok(GrooveExtraction {
        pulse_ms,
        pulse_offsets,
        swing,
        template,
    })
}

/// Refine a coarse flux-frame onset time to the energy centroid of the
/// transient region (sample-accurate; coarse frames land up to a window early).
fn refine_onset(t: f32, mono: &[f32], sample_rate: u32) -> f32 {
    #[expect(
        clippy::cast_possible_truncation,
        clippy::cast_sign_loss,
        reason = "Bounded, non-negative sample index"
    )]
    let start = (t * sample_rate as f32) as usize;
    let lo = start.saturating_sub(128);
    let hi = (start + 384).min(mono.len());
    let mut num = 0.0f32;
    let mut den = 0.0f32;
    for (s, sample) in mono[lo..hi].iter().enumerate() {
        let e = sample * sample;
        num = e.mul_add((lo + s) as f32, num);
        den += e;
    }
    if den > 1e-9 {
        #[expect(clippy::cast_precision_loss, reason = "Centroid index → seconds")]
        let centroid = (num / den) / sample_rate as f32;
        centroid
    } else {
        t
    }
}

/// Center frequency of one of the six audit bands (for EQ patches).
#[must_use]
pub fn band_center_hz(band_idx: usize) -> f32 {
    let lo = AUDIT_BAND_EDGES_HZ[band_idx.min(5)];
    let hi = AUDIT_BAND_EDGES_HZ[(band_idx.min(5)) + 1];
    (lo * hi).sqrt()
}

/// The six band labels (re-exported for receipts).
#[must_use]
pub const fn band_labels() -> [&'static str; 6] {
    crate::analysis::band_labels()
}

#[cfg(test)]
mod tests {
    // Tests intentionally use `.expect()` on freshly generated fixtures.
    #![expect(
        clippy::expect_used,
        reason = "Test fixtures assert decode/onset success"
    )]

    use super::*;

    fn test_wav() -> Vec<u8> {
        let left: Vec<f32> = (0..48_000)
            .map(|i| 0.3 * (std::f32::consts::TAU * 440.0 * i as f32 / 48_000.0).sin())
            .collect();
        crate::wav::write_wav_stereo(&left, &left, 48_000, 16)
    }

    #[test]
    fn analyze_reference_wav_produces_profile() {
        let profile = analyze_reference_wav(&test_wav(), "ref.wav").expect("decodes");
        assert_eq!(profile.path, "ref.wav");
        assert_eq!(profile.sample_rate, 48_000);
        assert_eq!(profile.bands.len(), 6);
        // A 440 Hz tone lands in the lo-mid band (250–500 Hz).
        assert!(
            profile.bands[2] > 0.5,
            "lo-mid band should dominate, got {:?}",
            profile.bands
        );
        assert!(profile.true_peak_db < 0.0);
    }

    #[test]
    fn compare_measured_identical_inputs_score_zero() {
        let profile = analyze_reference_wav(&test_wav(), "ref.wav").expect("decodes");
        let dev = compare_measured(
            "master",
            profile.bands,
            Some(profile.lufs),
            profile.centroid_hz,
            &profile,
        );
        assert!(
            dev.overall < 0.01,
            "identical → ~0 deviation, got {}",
            dev.overall
        );
        assert_eq!(dev.entity, "master");
    }

    #[test]
    fn compare_measured_scooped_low_end_is_diagnosed() {
        let mut profile = analyze_reference_wav(&test_wav(), "ref.wav").expect("decodes");
        // Simulate a render that is missing low-end energy (measured below ref).
        let mut bands = profile.bands;
        bands[0] = 0.0; // low band completely missing
        profile.bands[0] = 0.3;
        let dev = compare_measured(
            "master",
            bands,
            Some(profile.lufs),
            profile.centroid_hz,
            &profile,
        );
        assert!(dev.band_deltas[0] > 0.0, "reference > measured in sub band");
        assert!(dev.overall > 0.05);
        assert!(dev.diagnosis.contains("sub"));
    }

    #[test]
    fn extract_groove_detects_straight_grid() {
        // A metronomic pulse every 500 ms (120 BPM quarters) is straight.
        let mut samples = vec![0.0f32; 48_000];
        for beat in 0..8 {
            let start = beat * 24_000;
            for i in 0..200 {
                let idx = start + i;
                if idx < samples.len() {
                    samples[idx] = 0.9;
                }
            }
        }
        let wav = crate::wav::write_wav(&samples, 48_000, 16);
        let groove = extract_groove(&wav, 120).expect("onsets found");
        assert!(
            (groove.pulse_ms - 125.0).abs() < 5.0,
            "16th at 120 BPM ≈ 125 ms"
        );
        assert!(
            groove.swing < 0.1,
            "straight grid → swing ≈ 0, got {}",
            groove.swing
        );
        assert_eq!(groove.template, "straight");
    }

    #[test]
    fn extract_groove_detects_swung_offbeats() {
        // Off-beat 16ths (odd slots) delayed ~42 ms = 1/3 pulse → swing 0.66.
        let mut samples = vec![0.0f32; 96_000];
        for slot in 0..16 {
            let base = slot * 6_000; // 16th = 125 ms @ 120 BPM (6000 samples)
            let delay = if slot % 2 == 1 { 2_000 } else { 0 }; // ~42 ms late
            let start = base + delay;
            for i in 0..200 {
                let idx = start + i;
                if idx < samples.len() {
                    samples[idx] = 0.9;
                }
            }
        }
        let wav = crate::wav::write_wav(&samples, 48_000, 16);
        let groove = extract_groove(&wav, 120).expect("onsets found");
        assert!(
            groove.swing > 0.6,
            "swung off-beats → swing > 0.6, got {}",
            groove.swing
        );
        assert_eq!(groove.template, "swing16");
    }
}
