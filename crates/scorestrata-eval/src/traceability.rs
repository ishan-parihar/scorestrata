//! Traceability matrix — links spec → task → code → tests → MCP tool → eval.
//!
//! Architecture spec §14.5: every feature should link
//! `spec -> task -> code -> tests -> golden -> MCP tool -> docs -> eval result`.
//! This is what makes autonomous engineering possible: an agent can verify
//! a feature is fully delivered by walking the matrix.

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

/// A single traceability entry linking a feature across all planes.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TraceabilityEntry {
    /// Feature / spec ID (e.g. `SPEC-VALIDATOR-SCALE-001`).
    pub spec_id: String,
    /// Feature title.
    pub title: String,
    /// Architecture phase this belongs to (e.g. `-1`, `0`, `1`).
    pub phase: String,
    /// Engineering task IDs that implement this feature.
    pub tasks: Vec<String>,
    /// Source modules that implement this feature (crate::module).
    pub code: Vec<String>,
    /// Test identifiers covering this feature.
    pub tests: Vec<String>,
    /// MCP tool/resource exposing this feature (if agent-facing).
    pub mcp_tools: Vec<String>,
    /// CLI command exposing this feature (if user-facing).
    pub cli_commands: Vec<String>,
    /// Docs references.
    pub docs: Vec<String>,
    /// Latest eval result (pass/fail).
    pub eval_result: Option<String>,
}

/// The full traceability matrix.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct TraceabilityMatrix {
    /// Entries keyed by spec ID.
    pub entries: BTreeMap<String, TraceabilityEntry>,
}

impl TraceabilityMatrix {
    /// Create a new empty matrix.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Add or replace an entry.
    pub fn add_entry(&mut self, entry: TraceabilityEntry) {
        self.entries.insert(entry.spec_id.clone(), entry);
    }

    /// Get an entry by spec ID.
    #[must_use]
    pub fn get(&self, spec_id: &str) -> Option<&TraceabilityEntry> {
        self.entries.get(spec_id)
    }

    /// Count total entries.
    #[must_use]
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// Whether the matrix is empty.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// Get entries that are fully delivered: have code, tests, and (if
    /// agent-facing) an MCP tool or CLI command, plus a passing eval result.
    #[must_use]
    pub fn fully_delivered(&self) -> Vec<&TraceabilityEntry> {
        self.entries
            .values()
            .filter(|e| {
                !e.code.is_empty()
                    && !e.tests.is_empty()
                    && e.eval_result.as_deref() == Some("pass")
                    && (!e.mcp_tools.is_empty() || !e.cli_commands.is_empty())
            })
            .collect()
    }

    /// Get entries missing required artifacts (incomplete features).
    #[must_use]
    pub fn gaps(&self) -> Vec<(&TraceabilityEntry, Vec<String>)> {
        let mut gaps = Vec::new();
        for entry in self.entries.values() {
            let mut missing = Vec::new();
            if entry.code.is_empty() {
                missing.push("code".to_string());
            }
            if entry.tests.is_empty() {
                missing.push("tests".to_string());
            }
            if entry.eval_result.as_deref() != Some("pass") {
                missing.push("eval_result".to_string());
            }
            if entry.mcp_tools.is_empty() && entry.cli_commands.is_empty() {
                missing.push("mcp_tool_or_cli".to_string());
            }
            if !missing.is_empty() {
                gaps.push((entry, missing));
            }
        }
        gaps
    }

    /// Coverage ratio of fully delivered features (0.0–1.0).
    #[must_use]
    pub fn coverage(&self) -> f32 {
        if self.entries.is_empty() {
            return 0.0;
        }
        let delivered = self.fully_delivered().len();
        #[expect(clippy::cast_precision_loss, reason = "Coverage ratio for reporting")]
        let ratio = delivered as f32 / self.entries.len() as f32;
        ratio
    }

    /// Serialize the matrix to a JSON string.
    #[must_use]
    pub fn to_json(&self) -> String {
        serde_json::to_string_pretty(self).unwrap_or_default()
    }

    /// Render a human-readable coverage summary.
    #[must_use]
    pub fn summary(&self) -> String {
        let delivered = self.fully_delivered().len();
        let gaps = self.gaps().len();
        format!(
            "Traceability: {} features, {} fully delivered, {} with gaps (coverage {:.0}%)",
            self.entries.len(),
            delivered,
            gaps,
            self.coverage() * 100.0
        )
    }
}

/// Build the canonical Scorestrata traceability matrix for the implemented
/// roadmap. Wired to the actual modules, tests, MCP tools, and CLI commands
/// delivered in this codebase.
#[expect(
    clippy::too_many_lines,
    reason = "Matrix construction is a data table by design"
)]
#[must_use]
pub fn canonical_matrix() -> TraceabilityMatrix {
    let mut matrix = TraceabilityMatrix::new();

    matrix.add_entry(TraceabilityEntry {
        spec_id: "SPEC-CORE-IDS".to_string(),
        title: "Typed entity IDs".to_string(),
        phase: "-1".to_string(),
        tasks: vec!["CORE-001".to_string()],
        code: vec!["scorestrata_core::ids".to_string()],
        tests: vec!["contract_tests".to_string()],
        mcp_tools: vec![],
        cli_commands: vec![],
        docs: vec!["architecture.md §8".to_string()],
        eval_result: Some("pass".to_string()),
    });

    matrix.add_entry(TraceabilityEntry {
        spec_id: "SPEC-CORE-SCHEMA-REGISTRY".to_string(),
        title: "Versioned JSON Schema registry".to_string(),
        phase: "-1".to_string(),
        tasks: vec!["CORE-002".to_string()],
        code: vec!["scorestrata_core::schema_registry".to_string()],
        tests: vec!["contract_tests::schema_*".to_string()],
        mcp_tools: vec!["get_contracts".to_string()],
        cli_commands: vec!["ss doctor".to_string()],
        docs: vec!["architecture.md §3.2".to_string()],
        eval_result: Some("pass".to_string()),
    });

    matrix.add_entry(TraceabilityEntry {
        spec_id: "SPEC-CORE-ERROR-TAXONOMY".to_string(),
        title: "Unified error taxonomy".to_string(),
        phase: "-1".to_string(),
        tasks: vec!["CORE-003".to_string()],
        code: vec!["scorestrata_core::error".to_string()],
        tests: vec!["contract_tests".to_string()],
        mcp_tools: vec![],
        cli_commands: vec![],
        docs: vec!["architecture.md §28 Step 2".to_string()],
        eval_result: Some("pass".to_string()),
    });

    matrix.add_entry(TraceabilityEntry {
        spec_id: "SPEC-THEORY-SCALES".to_string(),
        title: "Scales, keys, and quantization".to_string(),
        phase: "1".to_string(),
        tasks: vec!["THEORY-001".to_string()],
        code: vec![
            "scorestrata_theory::scale".to_string(),
            "scorestrata_theory::key".to_string(),
        ],
        tests: vec!["scale::tests".to_string(), "key::tests".to_string()],
        mcp_tools: vec!["quantize_layer_to_scale".to_string()],
        cli_commands: vec![],
        docs: vec!["architecture.md §7.2".to_string()],
        eval_result: Some("pass".to_string()),
    });

    matrix.add_entry(TraceabilityEntry {
        spec_id: "SPEC-COMPILER-TIMELINE".to_string(),
        title: "Arrangement planning + timeline IR + MIDI export".to_string(),
        phase: "1".to_string(),
        tasks: vec!["COMPILER-001".to_string(), "COMPILER-002".to_string()],
        code: vec![
            "scorestrata_compiler::plan".to_string(),
            "scorestrata_compiler::compile".to_string(),
            "scorestrata_compiler::timeline".to_string(),
            "scorestrata_compiler::midi_export".to_string(),
        ],
        tests: vec![
            "plan::tests".to_string(),
            "compile::tests".to_string(),
            "mvas_simulation".to_string(),
        ],
        mcp_tools: vec!["plan_arrangement".to_string()],
        cli_commands: vec!["ss new".to_string(), "ss compile".to_string()],
        docs: vec!["architecture.md §7".to_string()],
        eval_result: Some("pass".to_string()),
    });

    matrix.add_entry(TraceabilityEntry {
        spec_id: "SPEC-VALIDATOR-SYMBOLIC".to_string(),
        title: "Symbolic validation (scale, segment, transition, density)".to_string(),
        phase: "2".to_string(),
        tasks: vec!["VALIDATOR-001".to_string()],
        code: vec![
            "scorestrata_validator::scale_check".to_string(),
            "scorestrata_validator::segment_check".to_string(),
            "scorestrata_validator::transition_check".to_string(),
            "scorestrata_validator::density".to_string(),
        ],
        tests: vec![
            "scale_check::tests".to_string(),
            "mvas_simulation".to_string(),
        ],
        mcp_tools: vec!["run_validation".to_string(), "suggest_fixes".to_string()],
        cli_commands: vec!["ss validate".to_string()],
        docs: vec!["architecture.md §11".to_string()],
        eval_result: Some("pass".to_string()),
    });

    matrix.add_entry(TraceabilityEntry {
        spec_id: "SPEC-AUDIO-RENDER".to_string(),
        title: "Offline WAV render from timeline IR".to_string(),
        phase: "5".to_string(),
        tasks: vec!["RENDER-001".to_string()],
        code: vec![
            "scorestrata_audio::render".to_string(),
            "scorestrata_audio::synth".to_string(),
        ],
        tests: vec!["render::tests".to_string(), "tools::tests".to_string()],
        mcp_tools: vec!["render_project".to_string(), "render_segment".to_string()],
        cli_commands: vec![],
        docs: vec!["architecture.md §17.2".to_string()],
        eval_result: Some("pass".to_string()),
    });

    matrix.add_entry(TraceabilityEntry {
        spec_id: "SPEC-MCP-CAPABILITIES".to_string(),
        title: "Capability discovery and tool catalog".to_string(),
        phase: "3".to_string(),
        tasks: vec!["MCP-001".to_string()],
        code: vec![
            "scorestrata_mcp::capabilities".to_string(),
            "scorestrata_mcp::tools".to_string(),
        ],
        tests: vec![
            "capabilities::tests".to_string(),
            "tools::tests".to_string(),
        ],
        mcp_tools: vec!["get_capabilities".to_string()],
        cli_commands: vec!["ss capabilities".to_string()],
        docs: vec!["architecture.md §10".to_string()],
        eval_result: Some("pass".to_string()),
    });

    matrix.add_entry(TraceabilityEntry {
        spec_id: "SPEC-AGENT-KERNEL".to_string(),
        title: "L0–L4 agent kernel with tournament selection".to_string(),
        phase: "3".to_string(),
        tasks: vec!["AGENT-001".to_string()],
        code: vec!["scorestrata_agent::kernel".to_string()],
        tests: vec!["kernel::tests".to_string(), "mvas_simulation".to_string()],
        mcp_tools: vec!["claim_task".to_string(), "submit_task_result".to_string()],
        cli_commands: vec![],
        docs: vec!["architecture.md §13".to_string()],
        eval_result: Some("pass".to_string()),
    });

    matrix.add_entry(TraceabilityEntry {
        spec_id: "SPEC-STORAGE-EVIDENCE".to_string(),
        title: "Ledger, snapshots, evidence warehouse, replay".to_string(),
        phase: "0".to_string(),
        tasks: vec!["STORAGE-001".to_string()],
        code: vec![
            "scorestrata_storage::ledger".to_string(),
            "scorestrata_storage::snapshot_graph".to_string(),
            "scorestrata_storage::evidence".to_string(),
            "scorestrata_storage::replay".to_string(),
        ],
        tests: vec![
            "ledger::tests".to_string(),
            "snapshot_graph::tests".to_string(),
            "mvas_simulation".to_string(),
        ],
        mcp_tools: vec![
            "get_snapshot_diff".to_string(),
            "get_command_trace".to_string(),
        ],
        cli_commands: vec![
            "ss trace".to_string(),
            "ss diff".to_string(),
            "ss replay".to_string(),
        ],
        docs: vec!["architecture.md §9".to_string()],
        eval_result: Some("pass".to_string()),
    });

    matrix.add_entry(TraceabilityEntry {
        spec_id: "SPEC-EVAL-GOLDEN".to_string(),
        title: "Golden harness and eval metrics".to_string(),
        phase: "8".to_string(),
        tasks: vec!["EVAL-001".to_string()],
        code: vec![
            "scorestrata_eval::golden".to_string(),
            "scorestrata_eval::metrics".to_string(),
        ],
        tests: vec![
            "golden::tests".to_string(),
            "golden_harness_e2e".to_string(),
        ],
        mcp_tools: vec!["get_eval_report".to_string()],
        cli_commands: vec![],
        docs: vec!["architecture.md §16".to_string()],
        eval_result: Some("pass".to_string()),
    });

    matrix.add_entry(TraceabilityEntry {
        spec_id: "SPEC-DEBUG-PLANE".to_string(),
        title: "Debug plane — traces, diffs, isolation, metric explanation".to_string(),
        phase: "4".to_string(),
        tasks: vec!["DEBUG-001".to_string()],
        code: vec![
            "scorestrata_core::debug".to_string(),
            "scorestrata_mcp::debug_ops".to_string(),
            "scorestrata_mcp::tools".to_string(),
        ],
        tests: vec![
            "debug_ops::tests".to_string(),
            "tools::tests".to_string(),
            "mvas_simulation".to_string(),
        ],
        mcp_tools: vec![
            "get_command_trace".to_string(),
            "get_snapshot_diff".to_string(),
            "get_candidate_diff".to_string(),
            "isolate_layer".to_string(),
            "solo_segment".to_string(),
            "explain_metric".to_string(),
        ],
        cli_commands: vec!["ss trace".to_string(), "ss diff".to_string()],
        docs: vec!["architecture.md §12".to_string()],
        eval_result: Some("pass".to_string()),
    });

    matrix.add_entry(TraceabilityEntry {
        spec_id: "SPEC-LEARNING-LOOP".to_string(),
        title: "Learning loop — lessons, repair templates, memory".to_string(),
        phase: "7".to_string(),
        tasks: vec!["LEARN-001".to_string()],
        code: vec![
            "scorestrata_storage::lesson_store".to_string(),
            "scorestrata_storage::memory".to_string(),
            "scorestrata_mcp::debug_ops".to_string(),
            "scorestrata_mcp::tools".to_string(),
        ],
        tests: vec![
            "lesson_store::tests".to_string(),
            "memory::tests".to_string(),
            "mvas_simulation".to_string(),
        ],
        mcp_tools: vec![
            "get_lessons".to_string(),
            "record_lesson".to_string(),
            "get_repair_templates".to_string(),
        ],
        cli_commands: vec![],
        docs: vec!["architecture.md §13".to_string()],
        eval_result: Some("pass".to_string()),
    });

    matrix.add_entry(TraceabilityEntry {
        spec_id: "SPEC-CLI-BUNDLE".to_string(),
        title: "CLI wired to real storage and pipeline".to_string(),
        phase: "0".to_string(),
        tasks: vec!["CLI-001".to_string()],
        code: vec!["scorestrata_cli::main".to_string()],
        tests: vec!["cli tests".to_string(), "mvas_simulation".to_string()],
        mcp_tools: vec![],
        cli_commands: vec![
            "ss new".to_string(),
            "ss config".to_string(),
            "ss compile".to_string(),
            "ss validate".to_string(),
            "ss doctor".to_string(),
        ],
        docs: vec!["architecture.md §19".to_string()],
        eval_result: Some("pass".to_string()),
    });

    matrix
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn canonical_matrix_covers_implemented_phases() {
        let matrix = canonical_matrix();
        assert!(matrix.len() >= 10, "canonical matrix covers the roadmap");
        let delivered = matrix.fully_delivered().len();
        assert!(
            delivered >= 8,
            "most features are fully delivered (got {delivered})"
        );
        assert!(matrix.coverage() > 0.7, "coverage: {}", matrix.coverage());
    }

    #[test]
    fn matrix_reports_gaps_for_incomplete_entries() {
        let mut matrix = TraceabilityMatrix::new();
        matrix.add_entry(TraceabilityEntry {
            spec_id: "SPEC-INCOMPLETE".to_string(),
            title: "Missing pieces".to_string(),
            phase: "9".to_string(),
            tasks: vec![],
            code: vec![],
            tests: vec![],
            mcp_tools: vec![],
            cli_commands: vec![],
            docs: vec![],
            eval_result: None,
        });
        let gaps = matrix.gaps();
        assert_eq!(gaps.len(), 1);
        let missing = &gaps[0].1;
        assert!(missing.contains(&"code".to_string()));
        assert!(missing.contains(&"tests".to_string()));
        assert!(missing.contains(&"eval_result".to_string()));
    }

    #[test]
    fn matrix_summary_is_human_readable() {
        let matrix = canonical_matrix();
        let summary = matrix.summary();
        assert!(summary.contains("Traceability"));
        assert!(summary.contains("coverage"));
    }

    #[test]
    fn matrix_serializes_to_json() {
        let matrix = canonical_matrix();
        let json = matrix.to_json();
        assert!(json.contains("SPEC-COMPILER-TIMELINE"));
        assert!(json.contains("mcp_tools"));
    }
}
