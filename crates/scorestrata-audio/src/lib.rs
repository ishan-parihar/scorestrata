//! # Scorestrata Audio
//!
//! Audio engine: instruments, FX, mixer, offline render, stems.
//! Phase 5 + Phase 6 MVP implementation.
//!
//! Modules:
//! - `sampler`: drum sampler instrument
//! - `synth`: subtractive synth (oscillator + filter + ADSR)
//! - `mixer`: multi-channel mixer with constant-power pan law
//! - `mastering`: deterministic peak limiter
//! - `wav`: WAV file writer + reader (mono + stereo)
//! - `render`: deterministic offline render of timelines to WAV
//! - `analysis`: Level 4 audio metrics (LUFS, true peak, clipping, …)
//! - `audit`: post-generation spectral audit (STFT, spectrogram/waveform PNG)
//! - `reference`: Phase E reference profiles, deviation metrics, groove extraction
//! - `error`: audio-crate error type

#![deny(missing_docs)]

pub mod analysis;
pub mod audit;
pub mod bus;
pub mod convolve;
pub mod error;
pub mod fx;
pub mod mastering;
pub mod mixer;
pub mod reference;
pub mod render;
pub mod samplebank;
pub mod sampler;
pub mod synth;
pub mod wav;
