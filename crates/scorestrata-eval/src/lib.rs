//! # Scorestrata Eval
//!
//! Golden test harness, eval suite, regression reports, engineering plane.
//! Implements architecture spec §16 (testing and evaluation) and §14 (engineering plane).
//!
//! Modules:
//! - `engineering`: Task graph, executable specs, golden harness, eval reports (§14)
//! - `golden`: Golden test runner with assertions and expected outputs (§16.5)
//! - `metrics`: System, agent, and engineering eval metrics (§16.8)

#![deny(missing_docs)]

pub mod engineering;
pub mod golden;
pub mod metrics;
