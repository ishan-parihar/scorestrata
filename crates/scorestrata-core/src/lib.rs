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
pub mod schema_registry;
pub mod snapshot;
pub mod state_machine;
pub mod telemetry;

pub use error::ScorestrataError;
/// Re-export commonly used types at crate root.
pub use ids::*;
pub use schema_registry::{
    SCHEMA_REGISTRY_VERSION, SchemaEntry, Stability, get_schema, list_schema_names,
    registry_as_json, schema_registry, validate_instance, validate_required_fields,
};

/// Contract tests — verify real serialized types conform to registered schemas.
#[cfg(test)]
mod contract_tests;
#[cfg(test)]
mod proptests;
