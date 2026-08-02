//! Trace command — traces a command through the event ledger.
//!
//! The architecture spec requires: `trace a command`, `diff two snapshots`,
//! `diff two candidates`, `inspect compiler pass logs`.

use serde::{Deserialize, Serialize};

/// A trace entry — one step in a command's lifecycle.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TraceStep {
    /// Step sequence.
    pub step: u32,
    /// Phase name (e.g. `policy_check`, `compile`, `validate`, `apply`).
    pub phase: String,
    /// Status of this phase.
    pub status: String,
    /// Diagnostics produced in this phase.
    pub diagnostic_count: u32,
    /// Duration in milliseconds.
    pub duration_ms: u64,
    /// Evidence URIs from this phase.
    pub evidence_uris: Vec<String>,
}

/// Command trace — full lifecycle of a single command.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CommandTrace {
    /// Command ID.
    pub command_id: String,
    /// Command type.
    pub command_type: String,
    /// Actor that invoked the command.
    pub actor: String,
    /// Overall status.
    pub status: String,
    /// Steps in the trace.
    pub steps: Vec<TraceStep>,
    /// Total duration in milliseconds.
    pub total_duration_ms: u64,
    /// Next step counter.
    next_step: u32,
}

impl CommandTrace {
    /// Create a new trace for a command.
    #[must_use]
    pub fn new(command_id: &str, command_type: &str, actor: &str) -> Self {
        Self {
            command_id: command_id.to_string(),
            command_type: command_type.to_string(),
            actor: actor.to_string(),
            status: "in_progress".to_string(),
            steps: vec![],
            total_duration_ms: 0,
            next_step: 0,
        }
    }

    /// Add a step to the trace.
    pub fn add_step(
        &mut self,
        phase: &str,
        status: &str,
        duration_ms: u64,
        evidence_uris: Vec<String>,
    ) {
        let step = TraceStep {
            step: {
                self.next_step += 1;
                self.next_step
            },
            phase: phase.to_string(),
            status: status.to_string(),
            diagnostic_count: 0,
            duration_ms,
            evidence_uris,
        };
        self.total_duration_ms += duration_ms;
        self.steps.push(step);
    }

    /// Finalize the trace with a final status.
    pub fn finalize(&mut self, status: &str) {
        self.status = status.to_string();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn trace_records_steps() {
        let mut trace = CommandTrace::new("cmd_001", "apply_patch", "agent:composer");
        trace.add_step("policy_check", "passed", 5, vec![]);
        trace.add_step("compile", "passed", 12, vec![]);
        trace.add_step("validate", "passed", 8, vec![]);
        trace.finalize("succeeded");
        assert_eq!(trace.steps.len(), 3);
        assert_eq!(trace.total_duration_ms, 25);
        assert_eq!(trace.status, "succeeded");
    }

    #[test]
    fn trace_step_evidence_uris_preserved() {
        let mut trace = CommandTrace::new("cmd_002", "run_validation", "agent:critic");
        trace.add_step(
            "validate",
            "passed",
            10,
            vec!["project://test/evidence".to_string()],
        );
        assert_eq!(trace.steps[0].evidence_uris.len(), 1);
    }
}
