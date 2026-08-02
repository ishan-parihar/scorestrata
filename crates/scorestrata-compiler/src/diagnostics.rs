//! Compiler diagnostic utilities.

use scorestrata_core::diagnostic::Diagnostic;
use scorestrata_core::error::Severity;

/// Collect all diagnostics from a set of pass results.
#[must_use]
pub fn collect_diagnostics(pass_results: &[crate::passes::PassResult]) -> Vec<Diagnostic> {
    pass_results
        .iter()
        .flat_map(|r| r.diagnostics.clone())
        .collect()
}

/// Check if any pass produced critical diagnostics.
#[must_use]
pub fn has_critical_diagnostics(diagnostics: &[Diagnostic]) -> bool {
    diagnostics.iter().any(|d| d.severity == Severity::Critical)
}

/// Count diagnostics by severity.
#[must_use]
pub fn count_by_severity(diagnostics: &[Diagnostic]) -> DiagnosticCounts {
    let mut counts = DiagnosticCounts::default();
    for d in diagnostics {
        match d.severity {
            Severity::Info => counts.info += 1,
            Severity::Warning => counts.warnings += 1,
            Severity::Error => counts.errors += 1,
            Severity::Critical => counts.critical += 1,
        }
    }
    counts
}

/// Counts of diagnostics by severity.
#[derive(Debug, Clone, Default)]
pub struct DiagnosticCounts {
    /// Number of informational diagnostics.
    pub info: u32,
    /// Number of warning diagnostics.
    pub warnings: u32,
    /// Number of error diagnostics.
    pub errors: u32,
    /// Number of critical diagnostics.
    pub critical: u32,
}
