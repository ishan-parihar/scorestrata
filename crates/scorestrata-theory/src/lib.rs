//! # Scorestrata Theory
//!
//! Music theory: scales, intervals, chords, and musical time model.

#![deny(missing_docs)]

/// Chord types and chord construction.
pub mod chord;
/// Musical intervals.
pub mod interval;
/// Musical key representation.
pub mod key;
/// Musical scales and scale operations.
pub mod scale;

#[cfg(test)]
mod proptests;
