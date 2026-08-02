//! Compiler pass framework.
//!
//! Each pass transforms the timeline IR and emits diagnostics.
//! Passes are ordered, hashable, and observable.

use scorestrata_core::diagnostic::{Diagnostic, MetricScope};
use scorestrata_core::error::Severity;
use serde::{Deserialize, Serialize};

use crate::timeline::TimelineIR;

/// The result of running a compiler pass.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PassResult {
    /// Pass name.
    pub name: String,
    /// Whether the pass succeeded.
    pub success: bool,
    /// Diagnostics emitted.
    pub diagnostics: Vec<Diagnostic>,
    /// Input hash.
    pub input_hash: String,
    /// Output hash.
    pub output_hash: String,
    /// Duration in milliseconds.
    pub duration_ms: u64,
}

/// A compiler pass that transforms a timeline.
pub trait Pass {
    /// Name of this pass.
    fn name(&self) -> &str;

    /// Run the pass on the timeline, returning the modified timeline and diagnostics.
    fn run(&self, timeline: &mut TimelineIR) -> Vec<Diagnostic>;
}

/// Validate that all notes in the timeline are within MIDI range (0–127).
pub struct RangeCheckPass;

impl Pass for RangeCheckPass {
    fn name(&self) -> &'static str {
        "range_check"
    }

    fn run(&self, timeline: &mut TimelineIR) -> Vec<Diagnostic> {
        let mut diagnostics = Vec::new();
        for event in &timeline.events {
            if let crate::timeline::EventKind::NoteOn { pitch, .. } = &event.kind
                && pitch.0 > 127
            {
                diagnostics.push(Diagnostic {
                    id: scorestrata_core::ids::DiagnosticId::new(),
                    code: "RANGE_VIOLATION".to_string(),
                    metric: "timeline.note_range".to_string(),
                    severity: Severity::Error,
                    scope: MetricScope::default(),
                    evidence_uris: Vec::new(),
                    message: format!("Note {} exceeds MIDI range", pitch.0),
                    root_causes: vec!["note pitch out of MIDI range".to_string()],
                    suggested_patches: Vec::new(),
                    expected_delta: None,
                    confidence: 1.0,
                });
            }
        }
        diagnostics
    }
}

/// Validate that events are in chronological order.
pub struct OrderCheckPass;

impl Pass for OrderCheckPass {
    fn name(&self) -> &'static str {
        "order_check"
    }

    fn run(&self, timeline: &mut TimelineIR) -> Vec<Diagnostic> {
        let mut diagnostics = Vec::new();
        for window in timeline.events.windows(2) {
            if window[1].tick < window[0].tick {
                diagnostics.push(Diagnostic {
                    id: scorestrata_core::ids::DiagnosticId::new(),
                    code: "ORDER_VIOLATION".to_string(),
                    metric: "timeline.event_order".to_string(),
                    severity: Severity::Error,
                    scope: MetricScope::default(),
                    evidence_uris: Vec::new(),
                    message: format!(
                        "Event at tick {} precedes event at tick {}",
                        window[1].tick.0, window[0].tick.0
                    ),
                    root_causes: vec!["events not sorted by tick".to_string()],
                    suggested_patches: Vec::new(),
                    expected_delta: None,
                    confidence: 1.0,
                });
            }
        }
        diagnostics
    }
}

/// Run a set of passes on a timeline.
pub fn run_passes(timeline: &mut TimelineIR, passes: &[Box<dyn Pass>]) -> Vec<PassResult> {
    let mut results = Vec::new();
    for pass in passes {
        let start = std::time::Instant::now();
        let input_hash = timeline.content_hash();
        let diagnostics = pass.run(timeline);
        let output_hash = timeline.content_hash();
        #[expect(clippy::cast_possible_truncation, reason = "Pass durations fit in u64")]
        let duration_ms = start.elapsed().as_millis() as u64;
        let success = diagnostics.iter().all(|d| d.severity != Severity::Critical);

        results.push(PassResult {
            name: pass.name().to_string(),
            success,
            diagnostics,
            input_hash,
            output_hash,
            duration_ms,
        });
    }
    results
}
