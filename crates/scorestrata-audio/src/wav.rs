//! WAV file writer — exports audio buffers to standard WAV format.
//!
//! The architecture spec requires: "implement offline WAV render."
//!
//! Currently supports 16-bit PCM only.

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
fn write_i16_sample(wav: &mut Vec<u8>, sample: f32) {
    let clamped = sample.clamp(-1.0, 1.0);
    #[expect(
        clippy::cast_possible_truncation,
        reason = "Clamped f32 fits in i16 range"
    )]
    let int_sample = (clamped * 32767.0) as i16;
    #[expect(clippy::cast_sign_loss, reason = "i16-to-u16 is safe for WAV encoding")]
    write_u16_le(wav, int_sample as u16);
}

fn write_u32_le(buf: &mut Vec<u8>, value: u32) {
    buf.extend_from_slice(&value.to_le_bytes());
}

fn write_u16_le(buf: &mut Vec<u8>, value: u16) {
    buf.extend_from_slice(&value.to_le_bytes());
}

#[cfg(test)]
mod tests {
    use super::*;

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
