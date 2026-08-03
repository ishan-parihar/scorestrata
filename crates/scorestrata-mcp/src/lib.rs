//! # Scorestrata MCP
//!
//! MCP server: tool adapters, resources, and capability discovery.
//! Implements the Contract & Governance Plane for agent operability.
//!
//! Phase 3 MVP: capability discovery, project summary, `run_validation`, `apply_patch`,
//! plus deterministic config generation (`config_gen`) and real patch/render
//! tool adapters.

#![deny(missing_docs)]

pub mod capabilities;
pub mod config_gen;
pub mod debug_ops;
pub mod dispatch;
pub mod patch_ops;
pub mod resources;
pub mod tools;

use scorestrata_core::error::ScorestrataError;

/// MCP server result type.
pub type McpResult<T> = Result<T, ScorestrataError>;
