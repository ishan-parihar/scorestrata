//! Schema registry — versioned, machine-readable JSON Schema documents.
//!
//! Implements Phase -1 (Contracts & Governance) of the architecture plan:
//! "schemas are machine-readable, contracts are versioned, tools are discoverable."
//!
//! Every data-facing contract type gets a registered JSON Schema so agents can:
//! - discover the shape of any resource or receipt,
//! - validate instances before submitting commands,
//! - generate typed clients,
//! - detect schema drift across versions.
//!
//! Schemas are hand-authored against the serde representation of each type
//! so they stay in sync with the Rust domain model without requiring the
//! `schemars` derive on every type.

use serde_json::{Value, json};
/// Schema stability level.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Stability {
    /// Experimental — may change without notice.
    Alpha,
    /// Usable — may change with deprecation notice.
    Beta,
    /// Stable — only changes under explicit version bumps.
    Stable,
}

impl Stability {
    /// Get the stability level as a string.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Alpha => "alpha",
            Self::Beta => "beta",
            Self::Stable => "stable",
        }
    }
}

/// A registered schema entry.
#[derive(Debug, Clone)]
pub struct SchemaEntry {
    /// Schema name (e.g. `ProjectConfig`, `CommandReceipt`).
    pub name: &'static str,
    /// Semantic version of this schema.
    pub version: &'static str,
    /// Stability level.
    pub stability: Stability,
    /// Description of what this schema represents.
    pub description: &'static str,
    /// The JSON Schema document.
    pub schema: Value,
}

impl SchemaEntry {
    /// Render the entry as a discoverable JSON document.
    #[must_use]
    pub fn as_json(&self) -> Value {
        json!({
            "name": self.name,
            "version": self.version,
            "stability": self.stability.as_str(),
            "description": self.description,
            "schema": self.schema,
        })
    }
}

// ── Core contract schemas ───────────────────────────────────────────

fn project_config_schema() -> Value {
    json!({
        "$schema": "https://json-schema.org/draft/2020-12/schema",
        "title": "ProjectConfig",
        "type": "object",
        "required": ["name", "genre", "bpm", "key", "duration_bars", "delivery_target"],
        "properties": {
            "name": { "type": "string" },
            "genre": { "type": "string" },
            "bpm": { "type": "integer", "minimum": 1, "maximum": 999 },
            "key": { "type": "string" },
            "time_signature": {
                "type": "object",
                "properties": {
                    "numerator": { "type": "integer" },
                    "denominator": { "type": "integer" }
                }
            },
            "duration_bars": { "type": "integer", "minimum": 1 },
            "delivery_target": { "enum": ["streaming", "cd", "club", "broadcast"] },
            "mood": { "type": "array", "items": { "type": "string" } },
            "references": { "type": "array", "items": { "type": "string" } }
        }
    })
}

fn note_schema() -> Value {
    json!({
        "$schema": "https://json-schema.org/draft/2020-12/schema",
        "title": "Note",
        "type": "object",
        "required": ["id", "pitch", "start", "duration", "velocity"],
        "properties": {
            "id": { "type": "string" },
            "pitch": { "type": "integer", "minimum": 0, "maximum": 127 },
            "start": { "type": "integer", "minimum": 0 },
            "duration": { "type": "integer", "minimum": 1 },
            "velocity": { "type": "integer", "minimum": 0, "maximum": 127 },
            "tied": { "type": "boolean" }
        }
    })
}

fn pattern_schema() -> Value {
    json!({
        "$schema": "https://json-schema.org/draft/2020-12/schema",
        "title": "Pattern",
        "type": "object",
        "required": ["id", "name", "bars"],
        "properties": {
            "id": { "type": "string" },
            "name": { "type": "string" },
            "bars": { "type": "integer", "minimum": 1 },
            "time_signature": {
                "type": "object",
                "properties": {
                    "numerator": { "type": "integer" },
                    "denominator": { "type": "integer" }
                }
            },
            "notes": { "type": "array", "items": { "type": "object" } },
            "steps": { "type": "array", "items": { "type": "object" } }
        }
    })
}

fn segment_schema() -> Value {
    json!({
        "$schema": "https://json-schema.org/draft/2020-12/schema",
        "title": "Segment",
        "type": "object",
        "required": ["id", "name", "role", "start_bar", "bars"],
        "properties": {
            "id": { "type": "string" },
            "name": { "type": "string" },
            "role": {
                "enum": ["intro", "verse", "build", "chorus", "bridge", "breakdown", "outro", "custom"]
            },
            "start_bar": { "type": "integer", "minimum": 0 },
            "bars": { "type": "integer", "minimum": 1 },
            "layers": { "type": "array", "items": { "type": "object" } }
        }
    })
}

fn command_schema() -> Value {
    json!({
        "$schema": "https://json-schema.org/draft/2020-12/schema",
        "title": "Command",
        "type": "object",
        "required": ["id", "idempotency_key", "command_type", "actor", "project_id", "scope", "params", "created_at"],
        "properties": {
            "id": { "type": "string" },
            "idempotency_key": { "type": "string" },
            "command_type": { "type": "string" },
            "actor": { "type": "string" },
            "project_id": { "type": "string" },
            "branch_id": { "type": ["string", "null"] },
            "dry_run": { "type": "boolean" },
            "scope": { "type": "object" },
            "params": { "type": "object" },
            "budget_id": { "type": ["string", "null"] },
            "created_at": { "type": "string", "format": "date-time" }
        }
    })
}

fn command_receipt_schema() -> Value {
    json!({
        "$schema": "https://json-schema.org/draft/2020-12/schema",
        "title": "CommandReceipt",
        "type": "object",
        "required": ["command_id", "status", "dry_run", "cost"],
        "properties": {
            "command_id": { "type": "string" },
            "status": { "enum": ["succeeded", "failed", "rejected", "dry_run"] },
            "dry_run": { "type": "boolean" },
            "snapshot_id": { "type": ["string", "null"] },
            "branch_id": { "type": ["string", "null"] },
            "score_delta": { "type": "object", "additionalProperties": { "type": "string" } },
            "policy_decisions": {
                "type": "array",
                "items": {
                    "type": "object",
                    "properties": {
                        "rule": { "type": "string" },
                        "passed": { "type": "boolean" }
                    }
                }
            },
            "events": { "type": "array", "items": { "type": "string" } },
            "evidence_uris": { "type": "array", "items": { "type": "string" } },
            "focus_uris": { "type": "array", "items": { "type": "string" } },
            "next_actions": { "type": "array", "items": { "type": "string" } },
            "cost": {
                "type": "object",
                "properties": {
                    "cpu_ms": { "type": "integer" },
                    "tool_calls": { "type": "integer" },
                    "render_seconds": { "type": "number" }
                }
            },
            "warnings": { "type": "array", "items": { "type": "string" } },
            "replay_cursor": { "type": ["string", "null"] }
        }
    })
}

fn diagnostic_schema() -> Value {
    json!({
        "$schema": "https://json-schema.org/draft/2020-12/schema",
        "title": "Diagnostic",
        "type": "object",
        "required": ["id", "code", "metric", "severity", "message", "confidence"],
        "properties": {
            "id": { "type": "string" },
            "code": { "type": "string" },
            "metric": { "type": "string" },
            "severity": { "enum": ["info", "warning", "error", "critical"] },
            "scope": { "type": "object" },
            "evidence_uris": { "type": "array", "items": { "type": "string" } },
            "message": { "type": "string" },
            "root_causes": { "type": "array", "items": { "type": "string" } },
            "suggested_patches": {
                "type": "array",
                "items": {
                    "type": "object",
                    "properties": {
                        "op": { "type": "string" },
                        "params": { "type": "object" },
                        "confidence": { "type": "number" }
                    }
                }
            },
            "expected_delta": { "type": ["string", "null"] },
            "confidence": { "type": "number", "minimum": 0.0, "maximum": 1.0 }
        }
    })
}

fn scorecard_schema() -> Value {
    json!({
        "$schema": "https://json-schema.org/draft/2020-12/schema",
        "title": "Scorecard",
        "type": "object",
        "required": ["id", "metrics", "overall_score"],
        "properties": {
            "id": { "type": "string" },
            "metrics": {
                "type": "array",
                "items": {
                    "type": "object",
                    "properties": {
                        "name": { "type": "string" },
                        "value": { "type": "number" },
                        "severity": { "enum": ["info", "warning", "error", "critical"] },
                        "evidence_uri": { "type": "string" },
                        "confidence": { "type": "number" }
                    }
                }
            },
            "overall_score": { "type": "number", "minimum": 0.0, "maximum": 1.0 },
            "hard_failures": { "type": "integer" },
            "warnings": { "type": "integer" }
        }
    })
}

fn validation_report_schema() -> Value {
    json!({
        "$schema": "https://json-schema.org/draft/2020-12/schema",
        "title": "ValidationReport",
        "type": "object",
        "required": ["project_id", "level", "diagnostics", "scorecard", "all_hard_gates_passed"],
        "properties": {
            "project_id": { "type": "string" },
            "level": { "type": "integer", "minimum": 1, "maximum": 6 },
            "diagnostics": { "type": "array", "items": { "type": "object" } },
            "scorecard": { "type": "object" },
            "all_hard_gates_passed": { "type": "boolean" },
            "summary": { "type": "object" }
        }
    })
}

fn budget_schema() -> Value {
    json!({
        "$schema": "https://json-schema.org/draft/2020-12/schema",
        "title": "Budget",
        "type": "object",
        "required": ["id", "project_id", "max_iterations", "max_tool_calls"],
        "properties": {
            "id": { "type": "string" },
            "project_id": { "type": "string" },
            "max_iterations": { "type": "integer" },
            "current_iterations": { "type": "integer" },
            "max_tool_calls": { "type": "integer" },
            "current_tool_calls": { "type": "integer" },
            "max_render_seconds": { "type": "number" },
            "current_render_seconds": { "type": "number" },
            "max_cpu_seconds": { "type": "number" },
            "current_cpu_seconds": { "type": "number" },
            "max_candidate_branches": { "type": "integer" },
            "current_candidate_branches": { "type": "integer" },
            "max_no_improvement_rounds": { "type": "integer" },
            "current_no_improvement_rounds": { "type": "integer" },
            "max_human_escalations": { "type": "integer" },
            "current_human_escalations": { "type": "integer" },
            "expires_at": { "type": ["string", "null"] }
        }
    })
}

fn policy_profile_schema() -> Value {
    json!({
        "$schema": "https://json-schema.org/draft/2020-12/schema",
        "title": "PolicyProfile",
        "type": "object",
        "properties": {
            "allow_destructive_commands": { "type": "boolean" },
            "require_snapshot_before_patch": { "type": "boolean" },
            "max_out_of_scale_ratio": { "type": "number", "minimum": 0.0, "maximum": 1.0 },
            "min_chorus_energy_delta": { "type": "number", "minimum": 0.0, "maximum": 1.0 },
            "max_true_peak_dbtp": { "type": "number" },
            "require_midi_export_on_finalize": { "type": "boolean" },
            "max_candidate_renders_per_segment": { "type": "integer" }
        }
    })
}

fn snapshot_schema() -> Value {
    json!({
        "$schema": "https://json-schema.org/draft/2020-12/schema",
        "title": "Snapshot",
        "type": "object",
        "required": ["id", "project_id", "branch_id", "label", "created_at", "state_hash"],
        "properties": {
            "id": { "type": "string" },
            "project_id": { "type": "string" },
            "branch_id": { "type": "string" },
            "label": { "type": "string" },
            "created_at": { "type": "string", "format": "date-time" },
            "command_id": { "type": ["string", "null"] },
            "state_hash": { "type": "string" },
            "scorecard_hash": { "type": ["string", "null"] }
        }
    })
}

fn brief_schema() -> Value {
    json!({
        "$schema": "https://json-schema.org/draft/2020-12/schema",
        "title": "Brief",
        "type": "object",
        "required": ["raw_text"],
        "properties": {
            "raw_text": { "type": "string" },
            "genre": { "type": ["string", "null"] },
            "mood": { "type": "array", "items": { "type": "string" } },
            "references": { "type": "array", "items": { "type": "string" } },
            "delivery_target": {
                "enum": ["streaming", "cd", "club", "broadcast"]
            },
            "constraints": { "type": "array", "items": { "type": "string" } },
            "confidence": { "type": "number", "minimum": 0.0, "maximum": 1.0 }
        }
    })
}

fn tool_receipt_schema() -> Value {
    json!({
        "$schema": "https://json-schema.org/draft/2020-12/schema",
        "title": "ToolReceipt",
        "type": "object",
        "required": ["tool", "status", "summary", "cost"],
        "properties": {
            "tool": { "type": "string" },
            "status": { "enum": ["succeeded", "failed", "blocked", "dry_run_completed"] },
            "summary": { "type": "string" },
            "score_delta": { "type": "object", "additionalProperties": { "type": "string" } },
            "evidence_uris": { "type": "array", "items": { "type": "string" } },
            "focus_uris": { "type": "array", "items": { "type": "string" } },
            "next_actions": { "type": "array", "items": { "type": "string" } },
            "cost": { "type": "object" },
            "warnings": { "type": "array", "items": { "type": "string" } },
            "dry_run": { "type": "boolean" },
            "diagnostics": { "type": "array", "items": { "type": "object" } }
        }
    })
}

// ── Registry ────────────────────────────────────────────────────────

/// Build the complete schema registry.
///
/// The registry is deterministic: entries are ordered by name and each
/// carries a semantic version for drift detection. The registry is built
/// once and cached in a [`OnceLock`] for zero-allocation repeated access.
fn build_registry() -> Vec<SchemaEntry> {
    vec![
        SchemaEntry {
            name: "Brief",
            version: "0.1.0",
            stability: Stability::Beta,
            description: "Production brief describing what to create",
            schema: brief_schema(),
        },
        SchemaEntry {
            name: "Budget",
            version: "0.1.0",
            stability: Stability::Beta,
            description: "Budget envelope limiting an agent session",
            schema: budget_schema(),
        },
        SchemaEntry {
            name: "Command",
            version: "0.1.0",
            stability: Stability::Beta,
            description: "Command envelope — the intent to perform a mutation",
            schema: command_schema(),
        },
        SchemaEntry {
            name: "CommandReceipt",
            version: "0.1.0",
            stability: Stability::Beta,
            description: "Structured result of a mutation",
            schema: command_receipt_schema(),
        },
        SchemaEntry {
            name: "Diagnostic",
            version: "0.1.0",
            stability: Stability::Beta,
            description: "Actionable diagnostic with evidence and suggested patches",
            schema: diagnostic_schema(),
        },
        SchemaEntry {
            name: "Note",
            version: "0.1.0",
            stability: Stability::Stable,
            description: "A musical note with pitch, position, duration, velocity",
            schema: note_schema(),
        },
        SchemaEntry {
            name: "Pattern",
            version: "0.1.0",
            stability: Stability::Stable,
            description: "A musical pattern containing notes or steps",
            schema: pattern_schema(),
        },
        SchemaEntry {
            name: "PolicyProfile",
            version: "0.1.0",
            stability: Stability::Beta,
            description: "Policy-as-code rules for governance and safety",
            schema: policy_profile_schema(),
        },
        SchemaEntry {
            name: "ProjectConfig",
            version: "0.1.0",
            stability: Stability::Stable,
            description: "High-level project configuration",
            schema: project_config_schema(),
        },
        SchemaEntry {
            name: "Scorecard",
            version: "0.1.0",
            stability: Stability::Beta,
            description: "Multi-metric scorecard with overall score",
            schema: scorecard_schema(),
        },
        SchemaEntry {
            name: "Segment",
            version: "0.1.0",
            stability: Stability::Stable,
            description: "A section of the arrangement",
            schema: segment_schema(),
        },
        SchemaEntry {
            name: "Snapshot",
            version: "0.1.0",
            stability: Stability::Beta,
            description: "Complete project state at a point in time",
            schema: snapshot_schema(),
        },
        SchemaEntry {
            name: "ToolReceipt",
            version: "0.1.0",
            stability: Stability::Beta,
            description: "Standard MCP tool result envelope",
            schema: tool_receipt_schema(),
        },
        SchemaEntry {
            name: "ValidationReport",
            version: "0.1.0",
            stability: Stability::Beta,
            description: "Validation report with diagnostics and scorecard",
            schema: validation_report_schema(),
        },
    ]
}

/// Get the cached schema registry.
#[must_use]
pub fn schema_registry() -> &'static [SchemaEntry] {
    static REGISTRY: std::sync::OnceLock<Vec<SchemaEntry>> = std::sync::OnceLock::new();
    REGISTRY.get_or_init(build_registry)
}

/// Get a schema entry by name.
#[must_use]
pub fn get_schema(name: &str) -> Option<&'static SchemaEntry> {
    schema_registry().iter().find(|e| e.name == name)
}

/// List all registered schema names.
#[must_use]
pub fn list_schema_names() -> Vec<&'static str> {
    schema_registry().iter().map(|e| e.name).collect()
}

/// Schema registry version — bumped when any schema changes.
pub const SCHEMA_REGISTRY_VERSION: &str = "0.1.0";

/// Render the full registry as a discoverable JSON document.
#[must_use]
pub fn registry_as_json() -> Value {
    json!({
        "schema_version": SCHEMA_REGISTRY_VERSION,
        "schemas": schema_registry().iter().map(SchemaEntry::as_json).collect::<Vec<_>>()
    })
}

// ── Structural instance validation ──────────────────────────────────

/// Validate that a JSON instance has all required top-level properties
/// for the given schema name.
///
/// This is a lightweight structural check (required-property presence and
/// simple type checks) that keeps the core dependency-free while still
/// catching malformed agent payloads before they reach the tools.
#[must_use]
pub fn validate_required_fields(schema_name: &str, instance: &Value) -> Vec<String> {
    let Some(entry) = get_schema(schema_name) else {
        return vec![format!("unknown schema: {schema_name}")];
    };
    let Some(required) = entry.schema.get("required").and_then(Value::as_array) else {
        return Vec::new();
    };

    let mut errors = Vec::new();
    for field in required {
        if let Some(field_name) = field.as_str()
            && instance.get(field_name).is_none()
        {
            errors.push(format!("missing required field '{field_name}'"));
        }
    }
    errors
}

/// Validate that an instance is a JSON object with the required fields.
///
/// # Errors
///
/// Returns `Err(Vec<String>)` with the list of missing required fields, or
/// `Err(vec!["instance must be a JSON object"])` if the instance is not an
/// object.
pub fn validate_instance(schema_name: &str, instance: &Value) -> Result<(), Vec<String>> {
    if !instance.is_object() {
        return Err(vec!["instance must be a JSON object".to_string()]);
    }
    let errors = validate_required_fields(schema_name, instance);
    if errors.is_empty() {
        Ok(())
    } else {
        Err(errors)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn registry_contains_core_contracts() {
        let names = list_schema_names();
        for expected in [
            "ProjectConfig",
            "Command",
            "CommandReceipt",
            "Diagnostic",
            "Scorecard",
            "ValidationReport",
            "Budget",
            "PolicyProfile",
            "Snapshot",
            "ToolReceipt",
            "Brief",
            "Note",
            "Pattern",
            "Segment",
        ] {
            assert!(names.contains(&expected), "missing schema {expected}");
        }
    }

    #[test]
    fn every_schema_has_title_and_type() {
        for entry in schema_registry() {
            assert_eq!(
                entry.schema["title"], entry.name,
                "schema title mismatch for {}",
                entry.name
            );
            assert_eq!(entry.schema["type"], "object");
        }
    }

    #[test]
    fn every_schema_has_version_and_stability() {
        for entry in schema_registry() {
            assert!(!entry.version.is_empty());
            assert!(matches!(
                entry.stability,
                Stability::Alpha | Stability::Beta | Stability::Stable
            ));
        }
    }

    #[test]
    fn registry_renders_as_json() {
        #[expect(clippy::unwrap_used, reason = "Test assertions")]
        let json = serde_json::to_string_pretty(&registry_as_json()).unwrap();
        assert!(json.contains("ProjectConfig"));
        assert!(json.contains("0.1.0"));
        assert!(json.contains("stable"));
    }

    #[test]
    fn valid_project_config_passes_validation() {
        let config = json!({
            "name": "Dark Techno",
            "genre": "melodic_techno",
            "bpm": 124,
            "key": "C minor",
            "duration_bars": 16,
            "delivery_target": "streaming"
        });
        assert!(validate_instance("ProjectConfig", &config).is_ok());
    }

    #[test]
    fn missing_required_field_fails_validation() {
        let config = json!({
            "name": "Dark Techno",
            "bpm": 124
        });
        let err = validate_instance("ProjectConfig", &config);
        assert!(err.is_err());
        #[expect(clippy::unwrap_used, reason = "Test assertions")]
        let errors = err.unwrap_err();
        assert!(errors.iter().any(|e| e.contains("genre")));
    }

    #[test]
    fn non_object_instance_fails_validation() {
        let err = validate_instance("ProjectConfig", &json!([1, 2, 3]));
        assert!(err.is_err());
    }

    #[test]
    fn unknown_schema_reports_error() {
        let err = validate_instance("NoSuchSchema", &json!({}));
        assert!(err.is_err());
        #[expect(clippy::unwrap_used, reason = "Test assertions")]
        let errors = err.unwrap_err();
        assert!(errors[0].contains("unknown schema"));
    }

    #[test]
    fn receipt_schema_requires_status() {
        let receipt = json!({
            "command_id": "cmd_1",
            "dry_run": false,
            "cost": {}
        });
        let errors = validate_required_fields("CommandReceipt", &receipt);
        assert!(errors.iter().any(|e| e.contains("status")));
    }

    #[test]
    fn schema_entries_are_deterministic() {
        let first = schema_registry();
        let second = schema_registry();
        assert_eq!(first.len(), second.len());
        for (a, b) in first.iter().zip(second) {
            assert_eq!(a.name, b.name);
            assert_eq!(a.schema, b.schema);
        }
    }
}
