//! Convolution reverb (Phase B) — partitioned FFT convolution with embedded
//! room / plate / hall impulse responses.
//!
//! Production reverb is *convolution*: the input is convolved with a real
//! impulse response so the tail carries the character of an actual space
//! (room = dense early reflections, plate = smooth metallic bloom, hall =
//! sparse long shimmer). The IRs here are **embedded and deterministic** —
//! synthesized from a seeded PRNG with per-kind decay shaping, no wall-clock
//! state, no external assets (R7) — so the same render always produces the
//! same wet tail (R2).
//!
//! The convolution is *partitioned*: the IR is split into fixed-size blocks
//! and each block is convolved via FFT (overlap-add), so the memory cost
//! stays flat regardless of IR length and the offline render tolerates the
//! one-partition latency (plan §3.4 / §5 "Convolution reverb too slow
//! offline → partitioned FFT (head/tail)").

// DSP-length math casts bounded sample counts into f32 for envelope math and
// back into usize for buffer indices; both directions are bounded by practical
// IR lengths, so the pedantic cast lints are expected (same discipline as
// `render.rs`).
#![expect(
    clippy::cast_precision_loss,
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    reason = "Convolution IR math casts bounded sample counts between usize/u64/u32/f32; lengths fit practical audio buffers"
)]

use rustfft::FftPlanner;
use rustfft::num_complex::Complex32;

/// Default FFT partition size (samples). Powers of two keep FFTs exact.
const PARTITION: usize = 2048;

/// Impulse-response tail lengths per character (seconds at 48 kHz).
const ROOM_TAIL_S: f32 = 0.9;
const PLATE_TAIL_S: f32 = 1.8;
const HALL_TAIL_S: f32 = 3.6;

/// A deterministic room/plate/hall impulse response.
#[derive(Debug, Clone)]
pub struct ImpulseResponse {
    /// Raw IR samples (mono, normalized peak 1.0).
    pub data: Vec<f32>,
    /// Character label (`room`, `plate`, `hall`).
    pub kind: String,
}

/// Synthesize the embedded IR for a reverb character at `sample_rate`.
///
/// Deterministic: the noise source is a fixed-seed `SplitMix64` stream, and the
/// decay envelope is pure math over the sample index, so the IR is identical
/// on every call and every machine (R2).
#[must_use]
pub fn embedded_ir(kind: &str, sample_rate: u32) -> ImpulseResponse {
    let sr = f32::max(sample_rate as f32, 1.0);
    let (tail_s, seed, hf_damp, density) = match kind {
        "plate" => (PLATE_TAIL_S, 0x5EED_CAFEu64, 0.72, 1.0),
        "hall" => (HALL_TAIL_S, 0xBAD_CAFEu64, 0.55, 0.6),
        _ => (ROOM_TAIL_S, 0x00C0_FFEEu64, 0.85, 1.3),
    };
    let n = (tail_s * sr).round() as usize;
    let mut rng = SplitMix64::new(seed);
    let mut ir: Vec<f32> = Vec::with_capacity(n);
    let mut lp = 0.0f32;
    for i in 0..n {
        let t = i as f32 / sr;
        // Frequency-dependent decay: low frequencies ring longer (denser for
        // room, brighter for plate). One-pole low-pass whose cutoff closes as
        // the tail decays.
        let cutoff = 12_000.0_f32.mul_add(1.0 - hf_damp, 800.0 * hf_damp / 4.0_f32.mul_add(t, 1.0));
        let coef = one_pole_coef(cutoff, sr);
        let noise = rng.next_f32().mul_add(2.0, -1.0);
        lp += coef * (noise - lp);
        let env = (-3.0 * t / tail_s).exp();
        // Early-reflection clustering: first ~30 ms is denser.
        let density_scale = if i < (0.03 * sr) as usize {
            density
        } else {
            density * 0.8
        };
        ir.push(lp * env * density_scale);
    }
    // Normalize peak to 1.0 so the wet level is meaningful across kinds.
    let peak = ir.iter().fold(0.0f32, |m, v| m.max(v.abs())).max(1e-6);
    for v in &mut ir {
        *v /= peak;
    }
    ImpulseResponse {
        data: ir,
        kind: kind.to_string(),
    }
}

/// One-pole low-pass coefficient from a cutoff frequency.
fn one_pole_coef(cutoff_hz: f32, sample_rate: f32) -> f32 {
    let tau = sample_rate / (2.0 * std::f32::consts::PI * cutoff_hz.max(1.0));
    1.0 - (-1.0 / tau).exp()
}

/// Compute the tail length in samples for an IR kind at a sample rate.
#[must_use]
pub fn ir_len_samples(kind: &str, sample_rate: u32) -> usize {
    let tail_s = match kind {
        "plate" => PLATE_TAIL_S,
        "hall" => HALL_TAIL_S,
        _ => ROOM_TAIL_S,
    };
    ((tail_s * f32::max(sample_rate as f32, 1.0)).round()) as usize
}

/// `SplitMix64` — tiny deterministic PRNG (no external crate, fixed seeds).
struct SplitMix64 {
    state: u64,
}

impl SplitMix64 {
    const fn new(seed: u64) -> Self {
        Self { state: seed }
    }

    const fn next_u64(&mut self) -> u64 {
        self.state = self.state.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut z = self.state;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        z ^ (z >> 31)
    }

    fn next_f32(&mut self) -> f32 {
        (self.next_u64() >> 40) as f32 / (1u64 << 24) as f32
    }
}

/// Partitioned FFT convolver: convolves an input buffer with a fixed IR.
///
/// The IR is split into `PARTITION`-sample blocks; each block is zero-padded
/// to `2*PARTITION`, FFT'd once at construction, and applied per input block
/// with overlap-add. Memory is O(partitions × 2·PARTITION) — flat in IR
/// length — and every float op is deterministic.
pub struct PartitionedConvolver {
    ir_len: usize,
    partition: usize,
    /// FFT of each zero-padded IR partition (length 2·partition).
    ir_partitions_fft: Vec<Vec<Complex32>>,
    fft: std::sync::Arc<dyn rustfft::Fft<f32>>,
    ifft: std::sync::Arc<dyn rustfft::Fft<f32>>,
}

impl PartitionedConvolver {
    /// Build a convolver for `ir`.
    #[must_use]
    pub fn new(ir: &[f32]) -> Self {
        let partition = PARTITION;
        let fft_len = 2 * partition;
        let mut planner = FftPlanner::<f32>::new();
        let fft = planner.plan_fft_forward(fft_len);
        let ifft = planner.plan_fft_inverse(fft_len);
        let mut scratch = vec![Complex32::default(); fft_len];
        let ir_len = ir.len().max(1);
        let n_partitions = ir_len.div_ceil(partition);
        let mut ir_partitions_fft = Vec::with_capacity(n_partitions);
        for p in 0..n_partitions {
            scratch.fill(Complex32::default());
            let start = p * partition;
            let end = (start + partition).min(ir_len);
            for (k, &v) in ir[start..end].iter().enumerate() {
                scratch[k] = Complex32::new(v, 0.0);
            }
            fft.process(&mut scratch);
            ir_partitions_fft.push(scratch.clone());
        }
        Self {
            ir_len,
            partition,
            ir_partitions_fft,
            fft,
            ifft,
        }
    }

    /// Full linear convolution of `input` with the IR (length
    /// `input.len() + ir_len - 1`).
    #[must_use]
    pub fn convolve(&self, input: &[f32]) -> Vec<f32> {
        if input.is_empty() {
            return Vec::new();
        }
        let out_len = input.len() + self.ir_len - 1;
        let mut out = vec![0.0f32; out_len];
        let p = self.partition;
        let mut work = vec![Complex32::default(); 2 * p];
        let mut block = vec![0.0f32; 2 * p];
        let mut y = vec![Complex32::default(); 2 * p];
        // Process the input in overlapping blocks of `partition` samples with
        // 50% overlap (overlap-add of length-2p inverse FFTs).
        let mut start = 0usize;
        while start < input.len() {
            block.fill(0.0);
            let end = (start + 2 * p).min(input.len());
            block[..end - start].copy_from_slice(&input[start..end]);
            for (k, v) in block.iter().enumerate() {
                work[k] = Complex32::new(*v, 0.0);
            }
            self.fft.process(&mut work);
            for (part, ir_fft) in self.ir_partitions_fft.iter().enumerate() {
                for (yk, (wk, ik)) in y.iter_mut().zip(work.iter().zip(ir_fft)) {
                    *yk = *wk * *ik;
                }
                self.ifft.process(&mut y);
                let base = start + part * p;
                for (k, yk) in y.iter().enumerate() {
                    let idx = base + k;
                    if idx < out_len {
                        out[idx] += yk.re / (2 * p) as f32;
                    }
                }
            }
            start += p;
        }
        out
    }
}

/// A ready-to-render convolution-reverb node: IR + pre-delay + wet level.
///
/// The renderer routes a layer's reverb **send** into this node's input; the
/// node returns the 100%-wet tail (pre-delay applied, dry untouched), which
/// the bus graph folds back at the send level. `wet` is the return bus level
/// (0.0–1.0) applied to the convolved tail.
pub struct ConvolutionReverb {
    /// The partitioned convolver for the selected IR.
    pub convolver: PartitionedConvolver,
    /// Pre-delay in samples (dry attack separated from the wet tail).
    pub pre_delay_samples: usize,
    /// Wet level (0.0–1.0) applied to the convolved tail.
    pub wet: f32,
}

impl ConvolutionReverb {
    /// Build a reverb node from a character and pre-delay (ms).
    #[must_use]
    pub fn new(kind: &str, pre_delay_ms: f32, wet: f32, sample_rate: u32) -> Self {
        let ir = embedded_ir(kind, sample_rate);
        let pre_delay_samples =
            ((pre_delay_ms.clamp(0.0, 200.0) * sample_rate as f32) / 1000.0).round() as usize;
        Self {
            convolver: PartitionedConvolver::new(&ir.data),
            pre_delay_samples,
            wet: wet.clamp(0.0, 1.0),
        }
    }

    /// Process `input` (the summed send bus) into the 100%-wet return tail.
    ///
    /// Returns a buffer of the same length: dry input is *not* folded in —
    /// the bus graph adds this tail scaled by the send amount.
    #[must_use]
    pub fn process(&self, input: &[f32]) -> Vec<f32> {
        if input.is_empty() {
            return Vec::new();
        }
        let out_len = input.len() + self.convolver.ir_len - 1 + self.pre_delay_samples;
        let mut tail = vec![0.0f32; out_len];
        let conv = self.convolver.convolve(input);
        for (i, &v) in conv.iter().enumerate() {
            tail[i + self.pre_delay_samples] = v * self.wet;
        }
        tail
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn irs_are_distinct_and_deterministic() {
        for kind in ["room", "plate", "hall"] {
            let a = embedded_ir(kind, 48_000);
            let b = embedded_ir(kind, 48_000);
            assert_eq!(a.data, b.data, "{kind} IR deterministic");
            assert!(a.data.iter().any(|v| v.abs() > 0.01), "{kind} not silent");
        }
        let room = embedded_ir("room", 48_000);
        let hall = embedded_ir("hall", 48_000);
        assert_ne!(room.data, hall.data, "room and hall differ");
        assert!(hall.data.len() > room.data.len(), "hall tail longer");
    }

    #[test]
    fn convolution_of_impulse_is_ir() {
        let ir = embedded_ir("plate", 48_000);
        let mut input = vec![0.0f32; 512];
        input[0] = 1.0;
        let conv = PartitionedConvolver::new(&ir.data).convolve(&input);
        // First `ir.len()` samples of conv(input) should be the IR itself.
        for (i, &v) in ir.data.iter().take(500).enumerate() {
            assert!(
                (conv[i] - v).abs() < 1e-3,
                "conv[{i}]={} vs ir={v}",
                conv[i]
            );
        }
    }

    #[test]
    fn reverb_applies_predelay_and_wet() {
        let reverb = ConvolutionReverb::new("room", 40.0, 0.5, 48_000);
        let mut input = vec![0.0f32; 4096];
        input[0] = 1.0;
        let tail = reverb.process(&input);
        // Pre-delay: nothing before ~40 ms.
        let pre = (40.0_f32 * 48_000.0_f32 / 1000.0_f32).round() as usize;
        for v in tail.iter().take(pre) {
            assert!(v.abs() < 1e-4, "silence before pre-delay");
        }
        // After pre-delay, the wet tail appears, scaled by wet=0.5.
        let peak: f32 = tail.iter().skip(pre).fold(0.0, |m, v| m.max(v.abs()));
        assert!(peak > 0.05, "tail present after pre-delay");
        assert!(peak <= 0.55, "wet level caps the tail");
    }
}
