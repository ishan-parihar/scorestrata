//! Audio engine error type.
//!
//! Distinct from the core `ScorestrataError` because WAV decoding and the
//! post-generation audit pipeline (I/O + PNG rendering) are audio-crate
//! concerns with their own failure modes.

/// Errors from audio I/O and the post-generation audit pipeline.
#[derive(Debug, thiserror::Error)]
pub enum AudioError {
    /// The WAV bytes could not be parsed as a supported PCM/float file.
    #[error("invalid WAV: {0}")]
    InvalidWav(String),
    /// Underlying filesystem error.
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),
    /// The audit visualization could not be rendered.
    #[error("visualization failed: {0}")]
    Visualization(String),
    /// A reference-analysis failure (e.g. no onsets in the WAV).
    #[error("{0}")]
    Other(String),
}
