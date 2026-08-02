//! # Scorestrata MCP
//!
//! MCP server: tool adapters, resources, and capability discovery.
//! Implements the Contract & Governance Plane for agent operability.
//!
//! Phase 3 MVP: capability discovery, project summary, `run_validation`, `apply_patch`.

#![deny(missing_docs)]

pub mod capabilities;
pub mod resources;
pub mod tools;

use scorestrata_core::error::ScorestrataError;

/// MCP server result type.
pub type McpResult<T> = Result<T, ScorestrataError>;
