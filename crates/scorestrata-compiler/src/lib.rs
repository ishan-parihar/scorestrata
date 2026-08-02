//! # Scorestrata Compiler
//!
//! The symbolic compiler transforms project configuration and patterns
//! into a deterministic timeline IR suitable for validation and rendering.

#![deny(missing_docs)]

pub mod arrangement;
pub mod diagnostics;
pub mod midi_export;
pub mod passes;
pub mod timeline;

use scorestrata_core::error::ScorestrataError;

/// Result type for compiler operations.
pub type CompilerResult<T> = Result<T, ScorestrataError>;

/// The compiler's output: a complete timeline IR.
pub use timeline::TimelineIR;
