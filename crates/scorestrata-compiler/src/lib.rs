//! # Scorestrata Compiler
//!
//! The symbolic compiler transforms project configuration and patterns
//! into a deterministic timeline IR suitable for validation and rendering.

#![deny(missing_docs)]

pub mod arrangement;
pub mod compile;
pub mod diagnostics;
pub mod incremental;
pub mod midi_export;
pub mod passes;
pub mod performance;
pub mod plan;
pub mod timeline;

use scorestrata_core::error::ScorestrataError;

/// Result type for compiler operations.
pub type CompilerResult<T> = Result<T, ScorestrataError>;

/// Project → timeline compilation entry points.
pub use compile::{compile_layer, compile_pattern, compile_project, compile_segment};
/// The compiler's output: a complete timeline IR.
pub use timeline::TimelineIR;
