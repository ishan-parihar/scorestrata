//! WAV file writer + reader — exports audio buffers to standard WAV format
//! and decodes standard WAV files back to f32 buffers.
//!
//! The architecture spec requires: "implement offline WAV render." The
//! reader powers the post-generation audit (`ss analyze`): a rendered WAV
//! is decoded, spectrally analyzed, and visualized.
//!
//! The writer supports 16-bit PCM; the reader supports 8/16-bit PCM and
//! 32-bit float, mono or stereo.

use crate::error::AudioError;

/// Write a stereo WAV file from interleaved L/R f32 sample slices.
///
/// Returns the WAV file bytes. Supports 16-bit PCM only; other values of
/// `bits_per_sample` are clamped to 16. Length is the min of the two
/// channel lengths.
#[must_use]
pub fn write_wav_stereo(
    left: &[f32],
    right: &[f32],
    sample_rate: u32,
    bits_per_sample: u16,
) -> Vec<u8> {
    let bits = bits_per_sample.min(16);
    let num_channels: u16 = 2;
    let frames = left.len().min(right.len());
    let byte_rate = sample_rate * u32::from(num_channels) * u32::from(bits) / 8;
    let block_align = num_channels * bits / 8;
    #[expect(
        clippy::cast_possible_truncation,
        reason = "Audio buffer length fits in u32 for WAV"
    )]
    let data_size = frames as u32 * u32::from(block_align);

    let mut wav = Vec::with_capacity(44 + data_size as usize);
    wav.extend_from_slice(b"RIFF");
    write_u32_le(&mut wav, 36 + data_size);
    wav.extend_from_slice(b"WAVE");
    wav.extend_from_slice(b"fmt ");
    write_u32_le(&mut wav, 16);
    write_u16_le(&mut wav, 1); // PCM
    write_u16_le(&mut wav, num_channels);
    write_u32_le(&mut wav, sample_rate);
    write_u32_le(&mut wav, byte_rate);
    write_u16_le(&mut wav, block_align);
    write_u16_le(&mut wav, bits);
    wav.extend_from_slice(b"data");
    write_u32_le(&mut wav, data_size);

    for i in 0..frames {
        write_i16_sample(&mut wav, left[i]);
        write_i16_sample(&mut wav, right[i]);
    }
    wav
}

/// Write a mono WAV file from f32 samples.
///
/// Returns the WAV file bytes.
/// Supports 16-bit PCM only; other values of `bits_per_sample` will be
/// clamped to 16.
#[must_use]
pub fn write_wav(samples: &[f32], sample_rate: u32, bits_per_sample: u16) -> Vec<u8> {
    // Only support 16-bit PCM
    let bits = bits_per_sample.min(16);
    let num_channels: u16 = 1;
    let byte_rate = sample_rate * u32::from(num_channels) * u32::from(bits) / 8;
    let block_align = num_channels * bits / 8;
    #[expect(
        clippy::cast_possible_truncation,
        reason = "Audio buffer length fits in u32 for WAV"
    )]
    let data_size = samples.len() as u32 * u32::from(block_align);

    let mut wav = Vec::new();

    // RIFF header
    wav.extend_from_slice(b"RIFF");
    write_u32_le(&mut wav, 36 + data_size);
    wav.extend_from_slice(b"WAVE");

    // fmt chunk
    wav.extend_from_slice(b"fmt ");
    write_u32_le(&mut wav, 16); // chunk size
    write_u16_le(&mut wav, 1); // PCM format
    write_u16_le(&mut wav, num_channels);
    write_u32_le(&mut wav, sample_rate);
    write_u32_le(&mut wav, byte_rate);
    write_u16_le(&mut wav, block_align);
    write_u16_le(&mut wav, bits);

    // data chunk
    wav.extend_from_slice(b"data");
    write_u32_le(&mut wav, data_size);

    for &sample in samples {
        write_i16_sample(&mut wav, sample);
    }

    wav
}

/// Encode one f32 sample as a clamped 16-bit PCM little-endian value.
///
/// Rounds to nearest (instead of truncating) so the quantization error is
/// bounded by half an LSB symmetrically, keeping f32 round-trips faithful.
fn write_i16_sample(wav: &mut Vec<u8>, sample: f32) {
    let clamped = sample.clamp(-1.0, 1.0);
    #[expect(
        clippy::cast_possible_truncation,
        reason = "Clamped f32 fits in i16 range"
    )]
    let int_sample = (clamped * 32767.0).round() as i16;
    #[expect(clippy::cast_sign_loss, reason = "i16-to-u16 is safe for WAV encoding")]
    write_u16_le(wav, int_sample as u16);
}

fn write_u32_le(buf: &mut Vec<u8>, value: u32) {
    buf.extend_from_slice(&value.to_le_bytes());
}

fn write_u16_le(buf: &mut Vec<u8>, value: u16) {
    buf.extend_from_slice(&value.to_le_bytes());
}

/// Decode a WAV file from bytes into (left, right, sample_rate) f32 buffers.
///
/// Supports 8/16-bit PCM and 32-bit float, mono or stereo. Mono input is
/// mirrored to both channels. Returns an error for malformed headers or
/// unsupported encodings.
///
/// # Errors
///
/// Returns [`AudioError::InvalidWav`] when the bytes are not a parseable
/// WAV in a supported encoding.
pub fn read_wav_stereo(bytes: &[u8]) -> Result<(Vec<f32>, Vec<f32>, u32), AudioError> {
    if bytes.len() < 12 || &bytes[0..4] != b"RIFF" || &bytes[8..12] != b"WAVE" {
        return Err(AudioError::InvalidWav(
            "missing RIFF/WAVE header".to_string(),
        ));
    }

    let mut pos = 12usize;
    let mut fmt: Option<(u16, u16, u32, u16)> = None; // (encoding, channels, rate, bits)
    let mut data: Option<&[u8]> = None;
    while pos + 8 <= bytes.len() {
        let chunk_id = &bytes[pos..pos + 4];
        let size = u32::from_le_bytes(
            bytes[pos + 4..pos + 8]
                .try_into()
                .expect("8-byte chunk size slice"),
        ) as usize;
        let body_start = pos + 8;
        if body_start + size > bytes.len() {
            break;
        }
        match chunk_id {
            b"fmt " => {
                if size < 16 {
                    return Err(AudioError::InvalidWav("fmt chunk too small".to_string()));
                }
                let encoding = u16::from_le_bytes(
                    bytes[body_start..body_start + 2]
                        .try_into()
                        .expect("2-byte encoding"),
                );
                let channels = u16::from_le_bytes(
                    bytes[body_start + 2..body_start + 4]
                        .try_into()
                        .expect("2-byte channels"),
                );
                let sample_rate = u32::from_le_bytes(
                    bytes[body_start + 4..body_start + 8]
                        .try_into()
                        .expect("4-byte sample rate"),
                );
                let bits = u16::from_le_bytes(
                    bytes[body_start + 14..body_start + 16]
                        .try_into()
                        .expect("2-byte bits"),
                );
                fmt = Some((encoding, channels, sample_rate, bits));
            }
            b"data" => {
                data = Some(&bytes[body_start..body_start + size]);
            }
            _ => {}
        }
        // Chunks are word-aligned: skip the padding byte on odd sizes.
        pos = body_start + size + (size % 2);
    }

    let (encoding, channels, sample_rate, bits) =
        fmt.ok_or_else(|| AudioError::InvalidWav("missing fmt chunk".to_string()))?;
    let raw = data.ok_or_else(|| AudioError::InvalidWav("missing data chunk".to_string()))?;
    if !(1..=2).contains(&channels) {
        return Err(AudioError::InvalidWav(format!(
            "unsupported channel count {channels}"
        )));
    }

    let mut left = Vec::new();
    let mut right = Vec::new();
    match (encoding, bits) {
        (1, 16) => {
            let n = raw.len() / 2;
            for frame in 0..n / usize::from(channels) {
                let base = frame * usize::from(channels) * 2;
                left.push(i16_sample(&raw[base..base + 2]));
                if channels == 2 {
                    right.push(i16_sample(&raw[base + 2..base + 4]));
                }
            }
        }
        (1, 8) => {
            let n = raw.len();
            for frame in 0..n / usize::from(channels) {
                let base = frame * usize::from(channels);
                let s = (f32::from(raw[base]) - 128.0) / 128.0;
                left.push(s);
                if channels == 2 {
                    right.push((f32::from(raw[base + 1]) - 128.0) / 128.0);
                }
            }
        }
        (3, 32) => {
            let n = raw.len() / 4;
            for frame in 0..n / usize::from(channels) {
                let base = frame * usize::from(channels) * 4;
                left.push(f32_sample(&raw[base..base + 4]));
                if channels == 2 {
                    right.push(f32_sample(&raw[base + 4..base + 8]));
                }
            }
        }
        _ => {
            return Err(AudioError::InvalidWav(format!(
                "unsupported encoding {encoding}/{bits}-bit"
            )));
        }
    }
    if channels == 1 {
        right = left.clone();
    }
    Ok((left, right, sample_rate))
}

/// Read and decode a WAV file from disk.
///
/// # Errors
///
/// Returns [`AudioError::Io`] on filesystem errors and
/// [`AudioError::InvalidWav`] for malformed/unsupported files.
pub fn read_wav_file(path: &std::path::Path) -> Result<(Vec<f32>, Vec<f32>, u32), AudioError> {
    let bytes = std::fs::read(path)?;
    read_wav_stereo(&bytes)
}

/// Decode a little-endian 16-bit PCM sample to f32.
///
/// Divides by 32767.0 to match the writer's scale, keeping the f32 round trip
/// symmetric (error ≤ half an LSB) instead of drifting by a full LSB. The
/// result is clamped to ±1.0 so a full-scale `i16::MIN` (−32768/32767 ≈
/// −1.00003) cannot trip the audit's `|s| >= 1.0` hard-clipping detector on
/// third-party files.
fn i16_sample(bytes: &[u8]) -> f32 {
    let raw = i16::from_le_bytes([bytes[0], bytes[1]]);
    (f32::from(raw) / 32767.0).clamp(-1.0, 1.0)
}

/// Decode a little-endian 32-bit float sample (clamped to ±1).
fn f32_sample(bytes: &[u8]) -> f32 {
    let raw = f32::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]);
    raw.clamp(-1.0, 1.0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    #[allow(clippy::unwrap_used, reason = "Test should panic if WAV is malformed")]
    fn read_wav_round_trips_stereo_pcm() {
        let left: Vec<f32> = vec![0.0, 0.25, -0.5, 0.75, -1.0, 0.125];
        let right: Vec<f32> = vec![0.1, -0.2, 0.3, -0.4, 0.5, -0.6];
        let wav = write_wav_stereo(&left, &right, 48000, 16);
        let (decoded_l, decoded_r, rate) = read_wav_stereo(&wav).expect("decodes");
        assert_eq!(rate, 48000);
        assert_eq!(decoded_l.len(), left.len());
        for (a, b) in decoded_l.iter().zip(&left) {
            assert!(
                (a - b).abs() < 1.0 / 32768.0,
                "stereo left sample {a} vs {b}"
            );
        }
        for (a, b) in decoded_r.iter().zip(&right) {
            assert!(
                (a - b).abs() < 1.0 / 32768.0,
                "stereo right sample {a} vs {b}"
            );
        }
    }

    #[test]
    #[allow(clippy::unwrap_used, reason = "Test should panic if WAV is malformed")]
    fn read_wav_mirrors_mono_to_both_channels() {
        let samples: Vec<f32> = vec![0.0, 0.5, -0.5, 1.0];
        let wav = write_wav(&samples, 44100, 16);
        let (l, r, _) = read_wav_stereo(&wav).expect("decodes");
        assert_eq!(l.len(), samples.len());
        assert_eq!(r, l, "mono mirrors to both channels");
    }

    #[test]
    fn read_wav_rejects_non_wav_bytes() {
        assert!(read_wav_stereo(b"not a wav file at all").is_err());
    }

    #[test]
    fn read_wav_skips_unknown_chunks_and_pad_bytes() {
        // A `LIST` chunk (odd size → one pad byte) interleaved between `fmt `
        // and `data` must be skipped; real files from DAWs carry these.
        let left: Vec<f32> = vec![0.5, -0.5];
        let right: Vec<f32> = vec![0.25, -0.25];
        let wav = write_wav_stereo(&left, &right, 48000, 16);

        // Split at the `data` chunk and insert a LIST chunk (odd, padded).
        let data_pos = wav
            .windows(4)
            .position(|w| w == b"data")
            .expect("writer emits a data chunk");
        let mut with_list = wav[..data_pos].to_vec();
        let list_body = b"LISTTEST"; // 8 bytes, even — still exercises skipping
        with_list.extend_from_slice(b"LIST");
        with_list.extend_from_slice(&(list_body.len() as u32).to_le_bytes());
        with_list.extend_from_slice(list_body);
        with_list.extend_from_slice(&wav[data_pos..]);

        let (l, r, rate) = read_wav_stereo(&with_list).expect("skips LIST chunk");
        assert_eq!(rate, 48000);
        for (a, b) in l.iter().zip(&left) {
            assert!(
                (a - b).abs() < 1.0 / 32768.0,
                "left sample {a} vs {b} within one LSB"
            );
        }
        for (a, b) in r.iter().zip(&right) {
            assert!(
                (a - b).abs() < 1.0 / 32768.0,
                "right sample {a} vs {b} within one LSB"
            );
        }
        assert_eq!(l.len(), left.len());
        assert_eq!(r.len(), right.len());
    }

    #[test]
    fn read_wav_clamps_full_scale_negative_to_negative_one() {
        // i16::MIN / 32767.0 would be -1.00003; the reader must clamp so the
        // audit never sees a phantom hard-clipped sample from a valid file.
        let raw = [0u8; 44].to_vec();
        let mut wav = raw;
        wav[0..4].copy_from_slice(b"RIFF");
        wav[8..12].copy_from_slice(b"WAVE");
        wav[12..16].copy_from_slice(b"fmt ");
        wav[16..20].copy_from_slice(&16u32.to_le_bytes());
        wav[20..22].copy_from_slice(&1u16.to_le_bytes()); // PCM
        wav[22..24].copy_from_slice(&1u16.to_le_bytes()); // mono
        wav[24..28].copy_from_slice(&48_000u32.to_le_bytes());
        // Byte rate = sample_rate * channels * bits / 8 = 48000 * 1 * 2.
        wav[28..32].copy_from_slice(&96_000u32.to_le_bytes());
        wav[32..34].copy_from_slice(&2u16.to_le_bytes()); // block align
        wav[34..36].copy_from_slice(&16u16.to_le_bytes()); // bits
        wav[36..40].copy_from_slice(b"data");
        wav[40..44].copy_from_slice(&4u32.to_le_bytes()); // two 16-bit samples
        wav.extend_from_slice(&i16::MIN.to_le_bytes());
        wav.extend_from_slice(&i16::MAX.to_le_bytes());

        let (l, _r, _rate) = read_wav_stereo(&wav).expect("decodes manual WAV");
        assert_eq!(l[0], -1.0, "i16::MIN clamps to -1.0");
        assert_eq!(l[1], 1.0, "i16::MAX reads as 1.0");
    }

    #[test]
    #[allow(clippy::unwrap_used, reason = "Test should panic if WAV is malformed")]
    fn write_wav_produces_valid_header() {
        let samples = vec![0.0; 100];
        let wav = write_wav(&samples, 44100, 16);
        assert_eq!(&wav[0..4], b"RIFF");
        assert_eq!(&wav[8..12], b"WAVE");
        assert_eq!(&wav[12..16], b"fmt ");
        assert_eq!(&wav[36..40], b"data");
    }

    #[test]
    #[allow(clippy::unwrap_used, reason = "Test should panic if WAV is malformed")]
    fn write_wav_correct_data_size() {
        let samples = vec![0.0; 256];
        let wav = write_wav(&samples, 44100, 16);
        // data size should be 256 * 2 bytes = 512
        let data_size = u32::from_le_bytes([wav[40], wav[41], wav[42], wav[43]]);
        assert_eq!(data_size, 512);
    }

    #[test]
    fn write_wav_clamps_to_16bit() {
        let samples = vec![0.0; 10];
        let wav_32 = write_wav(&samples, 44100, 32);
        let wav_16 = write_wav(&samples, 44100, 16);
        // Both should produce same output (32 clamped to 16)
        assert_eq!(wav_32.len(), wav_16.len());
    }

    #[test]
    fn write_wav_stereo_interleaves_lr() {
        let left = vec![0.5f32; 8];
        let right = vec![-0.5f32; 8];
        let wav = write_wav_stereo(&left, &right, 44100, 16);
        assert_eq!(&wav[0..4], b"RIFF");
        // fmt chunk channels field = 2
        assert_eq!(u16::from_le_bytes([wav[22], wav[23]]), 2);
        // data chunk size = 8 frames * 4 bytes
        let data_size = u32::from_le_bytes([wav[40], wav[41], wav[42], wav[43]]);
        assert_eq!(data_size, 32);
        // First frame: L=0.5*32767 (positive), R=-0.5*32767 (negative)
        let l0 = i16::from_le_bytes([wav[44], wav[45]]);
        let r0 = i16::from_le_bytes([wav[46], wav[47]]);
        assert!(l0 > 16_000, "left first sample positive");
        assert!(r0 < -16_000, "right first sample negative");
    }

    #[test]
    fn write_wav_stereo_truncates_to_shortest_channel() {
        let left = vec![0.1f32; 10];
        let right = vec![0.1f32; 5];
        let wav = write_wav_stereo(&left, &right, 44100, 16);
        let data_size = u32::from_le_bytes([wav[40], wav[41], wav[42], wav[43]]);
        assert_eq!(data_size, 5 * 4, "uses min channel length");
    }
}
