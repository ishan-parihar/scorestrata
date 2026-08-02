//! # Scorestrata Agent
//!
//! Agent kernel, workflow state machine, budgets, candidate selection,
//! multi-agent task board, and blackboard coordination.
//!
//! Implements the Autonomy Plane per architecture spec §4:
//! - Agent kernel (L0–L4 loops)
//! - Workflow state machine
//! - Task DAG and lease-based claiming
//! - Blackboard model for coordination
//! - Candidate branches and tournaments
//! - Memory tiers (working, episodic, semantic, procedural)
//! - Stagnation detection and escalation

#![deny(missing_docs)]

pub mod kernel;
pub mod task_board;

// Re-export key types
pub use kernel::{
    AgentKernel, EscalationReason, HumanReviewPacket, IterationResult, KernelState, LoopLevel,
};
pub use task_board::{Blackboard, BoardTask, TaskBoard, TaskBoardStatus};
