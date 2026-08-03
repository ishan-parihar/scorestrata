//! Storage crate root.
//!
//! Project bundle, event ledger, snapshots, and content-addressed cache.
//!
//! Note: Domain types like `LedgerEntry` and `EventLedger` live in
//! `scorestrata-core::event`. This crate provides storage primitives
//! for persisting them to disk.

#![deny(missing_docs)]

pub mod adr;
pub mod bundle;
pub mod cache;
pub mod calibration;
pub mod candidate;
pub mod docs;
pub mod engineering;
pub mod evidence;
pub mod experiment;
pub mod knowledge;
pub mod ledger;
pub mod lesson_store;
pub mod memory;
pub mod provenance;
pub mod render_job;
pub mod replay;
pub mod snapshot_graph;
pub mod trace;

use scorestrata_core::error::ScorestrataError;

/// Result type for storage operations.
pub type StorageResult<T> = Result<T, ScorestrataError>;
