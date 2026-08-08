//! # Scorestrata Validator
//!
//! Multi-objective validator with diagnostics, evidence URIs,
//! root-cause attribution, and calibration-aware scoring.

#![deny(missing_docs)]

pub mod audio_check;
pub mod brief_check;
pub mod chord_check;
pub mod config_check;
pub mod density;
pub mod layer_check;
pub mod register_check;
pub mod regression_check;
pub mod report;
pub mod scale_check;
pub mod scope;
pub mod segment_check;
pub mod timeline_check;
pub mod transition_check;

use scorestrata_core::error::ScorestrataError;

/// Result type for validator operations.
pub type ValidatorResult<T> = Result<T, ScorestrataError>;

/// Validation level (1=config, 2=symbolic, 3=timeline, 4=audio, 5=brief, 6=regression).
pub type ValidationLevel = u8;
