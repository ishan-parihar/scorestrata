//! Post-generation spectral audit — the "debug log" for generated music.
//!
//! §2 of the audio-quality plan: every render can be audited to a text log,
//! a spectrogram PNG, and a waveform PNG using a deterministic STFT
//! (`rustfft`) and bitmap rendering (`plotters`). The report carries the
//! metrics the agent loop and the user both need to see *why* a render
//! sounds the way it does: spectral centroid/rolloff, per-band energy,
//! aliasing ratio (>16 kHz share), DC offset, clipping, and silence.
//!
//! All analysis is pure and deterministic (no RNG): the same samples always
//! produce the same report bytes, per §17 determinism rules.

use rustfft::FftPlanner;
use rustfft::num_complex::Complex;
use serde::{Deserialize, Serialize};

use crate::analysis::{AUDIT_BAND_EDGES_HZ, GenreFingerprint};
use crate::error::AudioError;

/// STFT frame size in samples (48 ms at 48 kHz).
pub const STFT_FRAME: usize = 2048;
/// STFT hop in samples (10.7 ms at 48 kHz).
pub const STFT_HOP: usize = 512;
/// Spectrogram PNG width (columns). Frames are decimated down to this.
pub const SPECTROGRAM_WIDTH: usize = 512;
/// Spectrogram PNG height (rows).
pub const SPECTROGRAM_HEIGHT: usize = 240;
/// Waveform PNG width (columns).
pub const WAVEFORM_WIDTH: usize = 800;
/// Frequency ceiling drawn on the spectrogram (Hz).
pub const SPECTROGRAM_MAX_HZ: f32 = 12_000.0;
/// RMS below this dBFS is reported as silence.
pub const SILENCE_RMS_DB: f32 = -60.0;

/// Energy share of one spectral band.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BandEnergy {
    /// Human-readable band label (sub, bass, lo-mid, mid, hi-mid, air).
    pub label: String,
    /// Band lower edge in Hz.
    pub low_hz: f32,
    /// Band upper edge in Hz.
    pub high_hz: f32,
    /// Share of total spectral energy in this band (0..1).
    pub share: f32,
}

/// Post-generation spectral audit report.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AuditReport {
    /// Duration of the audited buffer in seconds.
    pub duration_seconds: f32,
    /// Sample rate of the audited buffer.
    pub sample_rate: u32,
    /// Peak level in dBFS.
    pub peak_dbfs: f32,
    /// RMS level in dBFS.
    pub rms_db: f32,
    /// DC offset (mean of the buffer).
    pub dc_offset: f32,
    /// Number of hard-clipped samples (|s| >= 1.0).
    pub clipped_sample_count: u32,
    /// Spectral centroid in Hz (energy-weighted mean frequency).
    pub spectral_centroid_hz: f32,
    /// Spectral rolloff in Hz (frequency below which 85% of energy lies).
    pub spectral_rolloff_hz: f32,
    /// Share of spectral energy above 16 kHz (aliasing/overs proxy).
    pub aliasing_ratio: f32,
    /// Whether the buffer is effectively silent.
    pub silence_detected: bool,
    /// Per-band energy shares (6 bands, summing to ~1.0 below 16 kHz).
    pub band_energy: Vec<BandEnergy>,
    /// Genre the fingerprint was checked against, if supplied.
    pub genre: Option<String>,
    /// `GenreFingerprint::deviation` (0..1) if a genre fingerprint applied.
    pub genre_fingerprint_deviation: Option<f32>,
}

/// Audited artifact (report plus rendered debug logs).
#[derive(Debug, Clone)]
pub struct AuditArtifact {
    /// The spectral report.
    pub report: AuditReport,
    /// Written text log path (`<artifact>.txt`).
    pub text_path: std::path::PathBuf,
    /// Written spectrogram PNG path.
    pub spectrogram_path: std::path::PathBuf,
    /// Written waveform PNG path.
    pub waveform_path: std::path::PathBuf,
}

/// Audit a mono buffer.
///
/// If `genre` is supplied and a [`GenreFingerprint`] exists for it, the
/// report also carries the fingerprint deviation.
#[must_use]
pub fn audit_mono(samples: &[f32], sample_rate: u32, genre: Option<&str>) -> AuditReport {
    let spectrum = stft(samples, STFT_FRAME, STFT_HOP, sample_rate);

    #[expect(
        clippy::cast_precision_loss,
        reason = "Buffer length fits in f32 for duration reporting"
    )]
    let duration_seconds = samples.len() as f32 / sample_rate as f32;

    let (peak_dbfs, rms_db, dc_offset, clipped_sample_count, silence_detected) =
        time_domain(samples);

    let centroid_hz = spectral_centroid(&spectrum.avg_magnitude, spectrum.freq_per_bin);
    let rolloff_hz = spectral_rolloff(&spectrum.avg_magnitude, spectrum.freq_per_bin, 0.85);
    let (aliasing_ratio, band_energy) = band_energy(&spectrum.avg_magnitude, spectrum.freq_per_bin);

    let (genre_name, genre_fingerprint_deviation) =
        match genre.and_then(GenreFingerprint::for_genre) {
            Some(fingerprint) => {
                let mut bands = [0.0f32; 6];
                for (slot, band) in bands.iter_mut().zip(&band_energy) {
                    *slot = band.share;
                }
                let deviation = fingerprint.deviation(&bands, centroid_hz, aliasing_ratio);
                (Some(fingerprint.genre.to_string()), Some(deviation))
            }
            None => (genre.map(str::to_string), None),
        };

    AuditReport {
        duration_seconds,
        sample_rate,
        peak_dbfs,
        rms_db,
        dc_offset,
        clipped_sample_count,
        spectral_centroid_hz: centroid_hz,
        spectral_rolloff_hz: rolloff_hz,
        aliasing_ratio,
        silence_detected,
        band_energy,
        genre: genre_name,
        genre_fingerprint_deviation,
    }
}

/// Audit a stereo buffer (folded to mono for the spectral analysis).
#[must_use]
pub fn audit_stereo(
    left: &[f32],
    right: &[f32],
    sample_rate: u32,
    genre: Option<&str>,
) -> AuditReport {
    let len = left.len().min(right.len());
    let mono: Vec<f32> = (0..len).map(|i| (left[i] + right[i]) * 0.5).collect();
    audit_mono(&mono, sample_rate, genre)
}

/// Deterministic STFT magnitude spectrum.
struct Stft {
    /// Average magnitude spectrum across all frames (bins 0..=N/2).
    avg_magnitude: Vec<f64>,
    /// Hz per FFT bin.
    freq_per_bin: f32,
}

/// Compute the STFT magnitude spectrum of a mono buffer.
fn stft(samples: &[f32], frame_size: usize, hop: usize, sample_rate: u32) -> Stft {
    let bins = frame_size / 2 + 1;
    let frame_count = samples.len().div_ceil(hop).max(1);

    let mut planner = FftPlanner::<f32>::new();
    let fft = planner.plan_fft_forward(frame_size);

    // Hann window.
    let window: Vec<f32> = (0..frame_size)
        .map(|i| {
            #[expect(clippy::cast_precision_loss, reason = "Window index conversion")]
            let t = i as f32 / (frame_size - 1) as f32;
            0.5 - 0.5 * (2.0 * std::f32::consts::PI * t).cos()
        })
        .collect();

    let mut avg = vec![0.0f64; bins];
    let mut buffer = vec![Complex::new(0.0f32, 0.0); frame_size];

    for frame in 0..frame_count {
        let start = frame * hop;
        for (i, slot) in buffer.iter_mut().enumerate() {
            let sample = samples.get(start + i).copied().unwrap_or(0.0);
            *slot = Complex::new(sample * window[i], 0.0);
        }
        fft.process(&mut buffer);
        for (bin, value) in buffer.iter().enumerate().take(bins) {
            avg[bin] += f64::from(value.norm());
        }
    }
    for value in &mut avg {
        *value /= frame_count as f64;
    }

    Stft {
        avg_magnitude: avg,
        #[expect(
            clippy::cast_precision_loss,
            reason = "Sample rate and frame size fit in f32"
        )]
        freq_per_bin: sample_rate as f32 / frame_size as f32,
    }
}

/// Peak, RMS, DC offset, clipped count, and silence from the time domain.
fn time_domain(samples: &[f32]) -> (f32, f32, f32, u32, bool) {
    let len = samples.len();
    if len == 0 {
        return (-100.0, -70.0, 0.0, 0, true);
    }
    let mut sum = 0.0f64;
    let mut sum_sq = 0.0f64;
    let mut peak = 0.0f32;
    let mut clipped = 0u32;
    for &s in samples {
        sum += f64::from(s);
        sum_sq += f64::from(s) * f64::from(s);
        let abs = s.abs();
        if abs > peak {
            peak = abs;
        }
        if abs >= 1.0 {
            clipped += 1;
        }
    }
    #[expect(clippy::cast_precision_loss, reason = "Buffer length fits in f32")]
    let rms = (sum_sq / len as f64).sqrt() as f32;
    #[expect(clippy::cast_precision_loss, reason = "Buffer length fits in f32")]
    let mean = (sum / len as f64) as f32;
    let rms_db = if rms > 1e-9 {
        20.0 * rms.log10()
    } else {
        -70.0
    };
    let peak_db = if peak > 1e-9 {
        20.0 * peak.log10()
    } else {
        -100.0
    };
    (peak_db, rms_db, mean, clipped, rms_db < SILENCE_RMS_DB)
}

/// Energy-weighted mean frequency.
fn spectral_centroid(magnitude: &[f64], freq_per_bin: f32) -> f32 {
    let mut weighted = 0.0f64;
    let mut total = 0.0f64;
    for (bin, &mag) in magnitude.iter().enumerate() {
        #[expect(
            clippy::cast_precision_loss,
            reason = "FFT bin index and bin spacing fit in f64"
        )]
        let freq = bin as f64 * f64::from(freq_per_bin);
        weighted += freq * mag;
        total += mag;
    }
    if total > 0.0 {
        #[expect(
            clippy::cast_precision_loss,
            reason = "Centroid computed in f64, returned as f32"
        )]
        return (weighted / total) as f32;
    }
    0.0
}

/// Frequency below which `fraction` of the total energy lies.
fn spectral_rolloff(magnitude: &[f64], freq_per_bin: f32, fraction: f32) -> f32 {
    let total: f64 = magnitude.iter().sum();
    if total <= 0.0 {
        return 0.0;
    }
    let target = f64::from(fraction) * total;
    let mut cumulative = 0.0f64;
    for (bin, &mag) in magnitude.iter().enumerate() {
        cumulative += mag;
        if cumulative >= target {
            #[expect(
                clippy::cast_precision_loss,
                reason = "Bin index and bin spacing fit in f64"
            )]
            let rolloff = (bin as f64 * f64::from(freq_per_bin)) as f32;
            return rolloff;
        }
    }
    #[expect(
        clippy::cast_precision_loss,
        reason = "Bin count times bin spacing fits in f32"
    )]
    let max_freq = (magnitude.len() as f32 * freq_per_bin) as f32;
    max_freq
}

/// Aliasing ratio (>16 kHz share) plus the six per-band energy shares.
fn band_energy(magnitude: &[f64], freq_per_bin: f32) -> (f32, Vec<BandEnergy>) {
    let total: f64 = magnitude.iter().sum();
    if total <= 0.0 {
        let labels = crate::analysis::band_labels();
        let bands = (0..6)
            .map(|i| BandEnergy {
                label: labels[i].to_string(),
                low_hz: AUDIT_BAND_EDGES_HZ[i],
                high_hz: AUDIT_BAND_EDGES_HZ[i + 1],
                share: 0.0,
            })
            .collect();
        return (0.0, bands);
    }

    let aliasing = |lo: f64, hi: f64| {
        magnitude
            .iter()
            .enumerate()
            .filter(|(bin, _)| {
                #[expect(clippy::cast_precision_loss, reason = "Bin index conversion")]
                let freq = *bin as f64 * f64::from(freq_per_bin);
                freq >= lo && freq < hi
            })
            .map(|(_, mag)| *mag)
            .sum::<f64>()
            / total
    };

    let mut bands = Vec::with_capacity(6);
    let labels = crate::analysis::band_labels();
    for (i, label) in labels.iter().enumerate() {
        #[expect(
            clippy::cast_precision_loss,
            reason = "Band energy share is a fraction, fits in f32"
        )]
        let share = aliasing(
            f64::from(AUDIT_BAND_EDGES_HZ[i]),
            f64::from(AUDIT_BAND_EDGES_HZ[i + 1]),
        ) as f32;
        bands.push(BandEnergy {
            label: (*label).to_string(),
            low_hz: AUDIT_BAND_EDGES_HZ[i],
            high_hz: AUDIT_BAND_EDGES_HZ[i + 1],
            share,
        });
    }
    #[expect(
        clippy::cast_precision_loss,
        reason = "Aliasing ratio is a fraction, fits in f32"
    )]
    let aliasing_ratio = aliasing(16_000.0, f64::INFINITY) as f32;
    (aliasing_ratio, bands)
}

/// Magma-like colormap used for the spectrogram heatmap.
fn magma_color(normalized: f32) -> (u8, u8, u8) {
    const STOPS: [(f32, (u8, u8, u8)); 5] = [
        (0.0, (0, 0, 4)),
        (0.25, (49, 7, 89)),
        (0.5, (122, 27, 104)),
        (0.75, (221, 55, 67)),
        (1.0, (252, 253, 191)),
    ];
    let v = normalized.clamp(0.0, 1.0);
    for pair in STOPS.windows(2) {
        let (t0, c0) = pair[0];
        let (t1, c1) = pair[1];
        if v <= t1 {
            let span = (t1 - t0).max(1e-6);
            let t = (v - t0) / span;
            let lerp = |a: u8, b: u8| {
                #[expect(
                    clippy::cast_precision_loss,
                    clippy::cast_possible_truncation,
                    reason = "Color channel interpolation"
                )]
                let channel = (f32::from(a) + (f32::from(b) - f32::from(a)) * t) as u8;
                channel
            };
            return (lerp(c0.0, c1.0), lerp(c0.1, c1.1), lerp(c0.2, c1.2));
        }
    }
    STOPS[4].1
}

/// Decimated STFT heatmap rows used to render the spectrogram.
///
/// `rows[y][x]` is a normalized magnitude (0..1); bins are clamped to
/// [`SPECTROGRAM_MAX_HZ`].
struct SpectrogramRows {
    rows: Vec<Vec<f32>>,
}

/// Build the decimated spectrogram heatmap (time × frequency rows).
fn spectrogram_rows(
    samples: &[f32],
    sample_rate: u32,
    frame_size: usize,
    hop: usize,
) -> SpectrogramRows {
    let max_bin = (SPECTROGRAM_MAX_HZ * frame_size as f32 / sample_rate as f32) as usize + 1;
    let max_bin = max_bin.min(frame_size / 2);
    let frame_count = samples.len().div_ceil(hop).max(1);
    let width = frame_count.min(SPECTROGRAM_WIDTH);

    let mut rows = vec![vec![0.0f32; width]; max_bin + 1];
    let mut counts = vec![vec![0u32; width]; max_bin + 1];

    let mut planner = FftPlanner::<f32>::new();
    let fft = planner.plan_fft_forward(frame_size);
    let window: Vec<f32> = (0..frame_size)
        .map(|i| {
            #[expect(clippy::cast_precision_loss, reason = "Window index conversion")]
            let t = i as f32 / (frame_size - 1) as f32;
            0.5 - 0.5 * (2.0 * std::f32::consts::PI * t).cos()
        })
        .collect();

    let mut buffer = vec![Complex::new(0.0f32, 0.0); frame_size];
    for frame in 0..frame_count {
        let start = frame * hop;
        for (i, slot) in buffer.iter_mut().enumerate() {
            let sample = samples.get(start + i).copied().unwrap_or(0.0);
            *slot = Complex::new(sample * window[i], 0.0);
        }
        fft.process(&mut buffer);
        #[expect(
            clippy::cast_possible_truncation,
            reason = "Decimation: frame index into width bucket"
        )]
        let column = (frame * width / frame_count) as usize;
        for bin in 0..=max_bin {
            let mag = buffer[bin].norm();
            rows[bin][column] += mag;
            counts[bin][column] += 1;
        }
    }

    // Normalize each cell by its count and by the global max (dB-scaled).
    let mut global_max = 0.0f32;
    for bin in 0..=max_bin {
        for column in 0..width {
            if counts[bin][column] > 0 {
                #[expect(clippy::cast_precision_loss, reason = "Averaging FFT magnitudes")]
                let divisor = counts[bin][column] as f32;
                rows[bin][column] /= divisor;
                global_max = global_max.max(rows[bin][column]);
            }
        }
    }
    if global_max > 0.0 {
        for bin in 0..=max_bin {
            for column in 0..width {
                let value = rows[bin][column];
                if value > 0.0 {
                    let db = 20.0 * (value / global_max).log10();
                    let db_min = -60.0;
                    rows[bin][column] = ((db - db_min) / -db_min).clamp(0.0, 1.0);
                } else {
                    rows[bin][column] = 0.0;
                }
            }
        }
    }

    SpectrogramRows { rows }
}

/// Write a spectrogram PNG of the buffer (log-magnitude heatmap, 0–12 kHz).
///
/// # Errors
///
/// Returns [`AudioError::Visualization`] when the plotters bitmap render
/// fails, or [`AudioError::Io`] on filesystem errors.
pub fn spectrogram_png(
    samples: &[f32],
    sample_rate: u32,
    out_path: &std::path::Path,
) -> Result<(), AudioError> {
    let image = spectrogram_rows(samples, sample_rate, STFT_FRAME, STFT_HOP);
    let width = image.rows.first().map_or(0, Vec::len);
    let height = image.rows.len();
    if width == 0 || height == 0 {
        return Err(AudioError::Visualization("empty spectrogram".to_string()));
    }

    use plotters::prelude::*;
    let root = BitMapBackend::new(
        out_path,
        (
            u32::try_from(width).expect("spectrogram width fits u32"),
            u32::try_from(height).expect("spectrogram height fits u32"),
        ),
    )
    .into_drawing_area();
    root.fill(&RGBColor(2, 2, 6))
        .map_err(|e| AudioError::Visualization(e.to_string()))?;

    for (row, bin_row) in image.rows.iter().enumerate() {
        for (column, &value) in bin_row.iter().enumerate() {
            let (r, g, b) = magma_color(value);
            root.draw(&Pixel::new((column as i32, row as i32), RGBColor(r, g, b)))
                .map_err(|e| AudioError::Visualization(e.to_string()))?;
        }
    }
    root.present()
        .map_err(|e| AudioError::Visualization(e.to_string()))?;
    Ok(())
}

/// Write a waveform PNG (min/max envelope) of the buffer.
///
/// # Errors
///
/// Returns [`AudioError::Visualization`] when the plotters bitmap render
/// fails, or [`AudioError::Io`] on filesystem errors.
pub fn waveform_png(samples: &[f32], out_path: &std::path::Path) -> Result<(), AudioError> {
    use plotters::prelude::*;
    let width = WAVEFORM_WIDTH;
    let height = 220i32;
    let root = BitMapBackend::new(out_path, (width as u32, height as u32)).into_drawing_area();
    root.fill(&RGBColor(6, 8, 12))
        .map_err(|e| AudioError::Visualization(e.to_string()))?;

    // Zero line.
    let mid = height / 2;
    root.draw(&PathElement::new(
        vec![(0, mid), (width as i32, mid)],
        &RGBColor(40, 44, 52),
    ))
    .map_err(|e| AudioError::Visualization(e.to_string()))?;

    if samples.is_empty() {
        return Ok(());
    }
    #[expect(
        clippy::cast_precision_loss,
        reason = "Buffer length fits in f32 for envelope column mapping"
    )]
    let samples_per_column = samples.len() as f32 / width as f32;
    for column in 0..width {
        #[expect(
            clippy::cast_possible_truncation,
            reason = "Column start index conversion"
        )]
        let start = (column as f32 * samples_per_column) as usize;
        #[expect(
            clippy::cast_possible_truncation,
            reason = "Column end index conversion"
        )]
        let end = (((column + 1) as f32 * samples_per_column) as usize).min(samples.len());
        let slice = &samples[start..end.max(start + 1).min(samples.len())];
        let min = slice.iter().copied().fold(f32::INFINITY, f32::min);
        let max = slice.iter().copied().fold(f32::NEG_INFINITY, f32::max);
        let y_min = mid - (min.abs().min(1.0) * (height / 2) as f32) as i32;
        let y_max = mid - (max.abs().min(1.0) * (height / 2) as f32) as i32;
        root.draw(&PathElement::new(
            vec![(column as i32, y_min), (column as i32, y_max)],
            &RGBColor(64, 200, 255),
        ))
        .map_err(|e| AudioError::Visualization(e.to_string()))?;
    }
    root.present()
        .map_err(|e| AudioError::Visualization(e.to_string()))?;
    Ok(())
}

/// Write a spectra-overlay PNG: several `(label, (freq, magnitude))` series
/// on one plot, for at-a-glance instrument/genre separation.
///
/// # Errors
///
/// Returns [`AudioError::Visualization`] when the plotters bitmap render
/// fails, or [`AudioError::Io`] on filesystem errors.
pub fn spectra_overlay_png(
    series: &[(&str, &[(f32, f32)])],
    out_path: &std::path::Path,
) -> Result<(), AudioError> {
    use plotters::prelude::*;
    let root = BitMapBackend::new(out_path, (640, 320)).into_drawing_area();
    root.fill(&RGBColor(6, 8, 12))
        .map_err(|e| AudioError::Visualization(e.to_string()))?;
    let mut chart = ChartBuilder::on(&root)
        .margin(10)
        .build_cartesian_2d(0.0f32..16_000.0f32, 0.0f32..1.0f32)
        .map_err(|e| AudioError::Visualization(e.to_string()))?;
    const COLORS: [RGBColor; 6] = [
        RGBColor(255, 92, 92),
        RGBColor(92, 200, 255),
        RGBColor(140, 255, 120),
        RGBColor(255, 220, 92),
        RGBColor(220, 140, 255),
        RGBColor(255, 160, 120),
    ];
    for (index, (_, points)) in series.iter().enumerate() {
        chart
            .draw_series(LineSeries::new(
                points.iter().map(|&(f, m)| (f, m)),
                &COLORS[index % COLORS.len()],
            ))
            .map_err(|e| AudioError::Visualization(e.to_string()))?;
    }
    root.present()
        .map_err(|e| AudioError::Visualization(e.to_string()))?;
    Ok(())
}

/// Render the human-readable `AUDIT:` debug-log block for a report.
#[must_use]
pub fn audit_text(report: &AuditReport) -> String {
    let mut out = String::new();
    out.push_str("AUDIT:\n");
    out.push_str(&format!(
        "  duration      : {:.2} s\n",
        report.duration_seconds
    ));
    out.push_str(&format!("  peak          : {:.2} dBFS\n", report.peak_dbfs));
    out.push_str(&format!("  rms           : {:.1} dB\n", report.rms_db));
    out.push_str(&format!(
        "  dc offset     : {:+.5}  {}\n",
        report.dc_offset,
        if report.dc_offset.abs() < 0.01 {
            "OK"
        } else {
            "WARN: nonzero DC"
        }
    ));
    out.push_str(&format!(
        "  clipped smp   : {}  {}\n",
        report.clipped_sample_count,
        if report.clipped_sample_count == 0 {
            "ok"
        } else {
            "WARN: HARD CLIPPING"
        }
    ));
    out.push_str(&format!(
        "  silence       : {}\n",
        if report.silence_detected { "yes" } else { "no" }
    ));
    out.push_str(&format!(
        "  centroid      : {:.0} Hz\n",
        report.spectral_centroid_hz
    ));
    out.push_str(&format!(
        "  rolloff 85%   : {:.0} Hz\n",
        report.spectral_rolloff_hz
    ));
    out.push_str(&format!(
        "  aliasing >16k : {:.1}%  {}\n",
        report.aliasing_ratio * 100.0,
        if report.aliasing_ratio > 0.05 {
            "WARN: bright/aliased"
        } else {
            "ok"
        }
    ));
    out.push_str("  band energy:\n");
    for band in &report.band_energy {
        out.push_str(&format!(
            "    {:<8} {:5.0}-{:<6.0} {:5.1}%\n",
            band.label,
            band.low_hz,
            band.high_hz,
            band.share * 100.0
        ));
    }
    if let Some(genre) = &report.genre {
        out.push_str(&format!("  genre fingerprint: {genre}\n"));
        match report.genre_fingerprint_deviation {
            Some(deviation) => out.push_str(&format!(
                "    deviation: {:.2}  {}\n",
                deviation,
                if deviation > crate::analysis::DRIFT_THRESHOLD {
                    "DRIFT"
                } else {
                    "match"
                }
            )),
            None => out.push_str("    (no fingerprint)\n"),
        }
    }
    out
}

/// Audit a buffer and write the text log + spectrogram + waveform PNGs.
///
/// Writes `<dir>/<artifact>.txt`, `<dir>/<artifact>.spectrogram.png`, and
/// `<dir>/<artifact>.waveform.png`. Returns the report and output paths.
///
/// # Errors
///
/// Returns [`AudioError::Io`] on filesystem errors and
/// [`AudioError::Visualization`] when a PNG render fails.
pub fn write_audit_files(
    samples: &[f32],
    sample_rate: u32,
    genre: Option<&str>,
    dir: &std::path::Path,
    artifact: &str,
) -> Result<AuditArtifact, AudioError> {
    std::fs::create_dir_all(dir)?;
    let report = audit_mono(samples, sample_rate, genre);

    let text_path = dir.join(format!("{artifact}.txt"));
    std::fs::write(&text_path, audit_text(&report))?;

    let spectrogram_path = dir.join(format!("{artifact}.spectrogram.png"));
    spectrogram_png(samples, sample_rate, &spectrogram_path)?;

    let waveform_path = dir.join(format!("{artifact}.waveform.png"));
    waveform_png(samples, &waveform_path)?;

    Ok(AuditArtifact {
        report,
        text_path,
        spectrogram_path,
        waveform_path,
    })
}

#[cfg(test)]
#[expect(
    clippy::expect_used,
    clippy::unwrap_used,
    reason = "Tests assert audit invariants explicitly"
)]
mod tests {
    use super::*;

    fn sine(sample_rate: u32, frequency: f32, seconds: f32) -> Vec<f32> {
        let n = (sample_rate as f32 * seconds) as usize;
        (0..n)
            .map(|i| {
                #[expect(clippy::cast_precision_loss, reason = "Test index")]
                let t = i as f32 / sample_rate as f32;
                (2.0 * std::f32::consts::PI * frequency * t).sin() * 0.5
            })
            .collect()
    }

    #[test]
    fn audit_centroid_tracks_sine_frequency() {
        let samples = sine(48_000, 440.0, 0.5);
        let report = audit_mono(&samples, 48_000, None);
        assert!(!report.silence_detected);
        assert_eq!(report.clipped_sample_count, 0);
        // A single 440 Hz tone has its centroid very near 440 Hz.
        assert!(
            (report.spectral_centroid_hz - 440.0).abs() < 120.0,
            "centroid {} Hz",
            report.spectral_centroid_hz
        );
        // Energy lands in the mid band (500 Hz..2 kHz catches harmonics; the
        // fundamental 440 Hz sits just under it, so the majority is in the
        // lo-mid 250-500 band).
        let mid_share = report
            .band_energy
            .iter()
            .find(|b| b.label == "lo-mid")
            .map_or(0.0, |b| b.share);
        assert!(mid_share > 0.3, "lo-mid share {mid_share}");
    }

    #[test]
    fn audit_detects_clipping_and_dc() {
        let mut samples = vec![0.9f32; 1000];
        samples.extend(vec![1.0f32; 100]);
        let report = audit_mono(&samples, 48_000, None);
        assert_eq!(report.clipped_sample_count, 100);
        assert!(report.dc_offset > 0.8, "DC offset {}", report.dc_offset);
    }

    #[test]
    fn audit_detects_silence() {
        let report = audit_mono(&[0.0; 1024], 48_000, None);
        assert!(report.silence_detected);
        assert_eq!(report.clipped_sample_count, 0);
    }

    #[test]
    fn audit_is_deterministic() {
        let samples = sine(48_000, 220.0, 0.5);
        let a = audit_mono(&samples, 48_000, Some("techno"));
        let b = audit_mono(&samples, 48_000, Some("techno"));
        assert_eq!(
            a.spectral_centroid_hz.to_bits(),
            b.spectral_centroid_hz.to_bits()
        );
        assert_eq!(
            a.genre_fingerprint_deviation.expect("deviation").to_bits(),
            b.genre_fingerprint_deviation.expect("deviation").to_bits()
        );
    }

    #[test]
    fn spectrogram_png_has_valid_magic_bytes() {
        let samples = sine(48_000, 880.0, 1.0);
        let path = std::env::temp_dir().join("ss_audit_spectrogram_test.png");
        spectrogram_png(&samples, 48_000, &path).expect("writes png");
        let bytes = std::fs::read(&path).expect("reads png");
        assert_eq!(&bytes[0..8], b"\x89PNG\r\n\x1a\n", "PNG magic bytes");
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn waveform_png_has_valid_magic_bytes() {
        let samples = sine(48_000, 220.0, 0.5);
        let path = std::env::temp_dir().join("ss_audit_waveform_test.png");
        waveform_png(&samples, &path).expect("writes png");
        let bytes = std::fs::read(&path).expect("reads png");
        assert_eq!(&bytes[0..8], b"\x89PNG\r\n\x1a\n", "PNG magic bytes");
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn spectra_overlay_png_has_valid_magic_bytes() {
        let path = std::env::temp_dir().join("ss_audit_overlay_test.png");
        let series = [
            ("sine", &[(0.0, 0.0), (440.0, 1.0), (880.0, 0.1)][..]),
            ("noise", &[(0.0, 0.5), (4000.0, 0.5), (16_000.0, 0.2)][..]),
        ];
        spectra_overlay_png(&series, &path).expect("writes png");
        let bytes = std::fs::read(&path).expect("reads png");
        assert_eq!(&bytes[0..8], b"\x89PNG\r\n\x1a\n", "PNG magic bytes");
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn genre_fingerprint_deviation_reported_for_wrong_timbre() {
        // A bright, hat-heavy loop should drift hard from the ambient
        // fingerprint but sit near the techno one.
        let mut bright: Vec<f32> = Vec::new();
        for i in 0..(48_000 * 2) {
            #[expect(clippy::cast_precision_loss, reason = "Test index")]
            let t = i as f32 / 48_000.0;
            let value = (2.0 * std::f32::consts::PI * 4000.0 * t).sin() * 0.3
                + (2.0 * std::f32::consts::PI * 9000.0 * t).sin() * 0.2;
            bright.push(value);
        }
        let as_ambient = audit_mono(&bright, 48_000, Some("ambient"));
        let as_techno = audit_mono(&bright, 48_000, Some("techno"));
        let ambient_dev = as_ambient
            .genre_fingerprint_deviation
            .expect("ambient deviation");
        let techno_dev = as_techno
            .genre_fingerprint_deviation
            .expect("techno deviation");
        assert!(
            ambient_dev > crate::analysis::DRIFT_THRESHOLD,
            "bright timbre must drift from ambient (deviation {ambient_dev})"
        );
        assert!(
            techno_dev < ambient_dev,
            "bright timbre matches techno better ({techno_dev} < {ambient_dev})"
        );
    }

    #[test]
    fn write_audit_files_writes_log_and_pngs() {
        let samples = sine(48_000, 330.0, 1.0);
        let dir = std::env::temp_dir().join("ss_audit_files_test");
        let artifact =
            write_audit_files(&samples, 48_000, Some("ambient"), &dir, "demo").expect("writes");
        let log = std::fs::read_to_string(&artifact.text_path).expect("reads log");
        assert!(log.contains("AUDIT:"), "text log has AUDIT header");
        assert!(log.contains("band energy"), "text log has band energy");
        let png = std::fs::read(&artifact.spectrogram_path).expect("reads spectrogram");
        assert_eq!(&png[0..8], b"\x89PNG\r\n\x1a\n", "spectrogram is a PNG");
        let wav_png = std::fs::read(&artifact.waveform_path).expect("reads waveform");
        assert_eq!(&wav_png[0..8], b"\x89PNG\r\n\x1a\n", "waveform is a PNG");
        let _ = std::fs::remove_dir_all(&dir);
    }
}
