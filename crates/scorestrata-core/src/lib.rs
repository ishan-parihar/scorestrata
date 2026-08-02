//! # Scorestrata Core
//!
//! Foundational types, IDs, error taxonomy, and domain model for Scorestrata.
//!
//! This crate defines the shared vocabulary used across all Scorestrata planes:
//! - **IDs** — newtyped identifiers for every entity
//! - **Error taxonomy** — structured, actionable error types
//! - **Domain model** — musical entities, operational entities, evidence entities
//! - **Commands & receipts** — the mutation envelope
//! - **Budgets & policies** — governance primitives

#![deny(missing_docs)]

pub mod budget;
pub mod command;
pub mod debug;
pub mod diagnostic;
pub mod domain;
pub mod error;
pub mod event;
pub mod ids;
pub mod policy;
pub mod policy_engine;
pub mod snapshot;
pub mod state_machine;
pub mod telemetry;

pub use error::ScorestrataError;
/// Re-export commonly used types at crate root.
pub use ids::*;
#[cfg(test)]
mod proptests;
