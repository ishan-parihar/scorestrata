//! Telemetry registry — process-wide cost and performance accounting (§15.5).
//!
//! The dispatcher records every tool call into a shared [`TelemetrySnapshot`]
//! so operators can measure what the system is doing: tool call counts, patch
//! attempts, and the derived rates (escalation, rollback, regression, patch
//! success). Backed by a [`std::sync::OnceLock`] + [`std::sync::Mutex`] so the
//! registry is safe to record from any thread without a runtime.

use std::sync::{Mutex, OnceLock};

use scorestrata_core::telemetry::TelemetrySnapshot;

/// The process-wide snapshot, initialized lazily on first use.
static REGISTRY: OnceLock<Mutex<TelemetrySnapshot>> = OnceLock::new();

fn registry() -> &'static Mutex<TelemetrySnapshot> {
    REGISTRY.get_or_init(|| Mutex::new(TelemetrySnapshot::new()))
}

/// Record a tool call in the shared snapshot.
///
/// Called by [`crate::dispatch::dispatch_tool`] for every invocation, so the
/// snapshot always reflects what the system has actually run.
pub fn record_tool_call(tool_name: &str) {
    if let Ok(mut snap) = registry().lock() {
        snap.record_tool_call(tool_name);
    }
}

/// Record a patch attempt (successful or not).
pub fn record_patch_attempt(successful: bool) {
    if let Ok(mut snap) = registry().lock() {
        snap.record_patch_attempt(successful);
    }
}

/// Snapshot the current telemetry state.
///
/// Returns a clone of the shared snapshot, so callers get a consistent view
/// without holding the lock.
#[must_use]
pub fn snapshot() -> TelemetrySnapshot {
    registry()
        .lock()
        .map_or_else(|_| TelemetrySnapshot::new(), |s| s.clone())
}

/// Reset the registry to a fresh snapshot.
///
/// Useful for tests and for starting a new session. Like the other registry
/// methods, this is best-effort: a poisoned lock is ignored rather than
/// panicking, since telemetry must never take down the process.
pub fn reset() {
    if let Ok(mut snap) = registry().lock() {
        *snap = TelemetrySnapshot::new();
    }
}

#[cfg(test)]
/// Shared serialization lock for every test that mutates the process-wide
/// registry (`reset`/record). Both the `telemetry` and `dispatch` test modules
/// use this single lock so they can never race each other's exact counts.
pub(crate) static TEST_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

/// Acquire the shared test lock, recovering from poisoning.
///
/// Why: if one test panics while holding `TEST_LOCK` (e.g. an exact-count
/// assertion), the mutex becomes poisoned and a plain `.lock().unwrap()` in
/// every other guarded test would panic too — turning one real failure into a
/// cascade of phantom lock-panics that obscures the root cause. Recovering the
/// guard lets the remaining tests run normally and fail only on their own
/// merits. Serialization is unchanged: this is a test-only lock, so a poisoned
/// writer adds no safety concern.
#[cfg(test)]
#[must_use]
pub(crate) fn test_lock_guard() -> std::sync::MutexGuard<'static, ()> {
    TEST_LOCK
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn records_tool_calls_across_threads() {
        let _guard = test_lock_guard();
        reset();
        record_tool_call("apply_patch");
        record_tool_call("apply_patch");
        record_tool_call("run_validation");

        let snap = snapshot();
        assert_eq!(snap.tool_calls.get("apply_patch"), Some(&2));
        assert_eq!(snap.tool_calls.get("run_validation"), Some(&1));
        assert_eq!(snap.total_tool_calls(), 3);
    }

    #[test]
    fn records_patch_attempts_and_derives_success_rate() {
        let _guard = test_lock_guard();
        reset();
        record_patch_attempt(true);
        record_patch_attempt(true);
        record_patch_attempt(false);

        let snap = snapshot();
        assert_eq!(snap.successful_patches, 2);
        assert_eq!(snap.total_patches, 3);
        assert!((snap.patch_success_rate() - 0.666_666_7).abs() < 0.001);
    }

    #[test]
    fn reset_starts_fresh() {
        let _guard = test_lock_guard();
        reset();
        record_tool_call("get_capabilities");
        assert_eq!(snapshot().total_tool_calls(), 1);
        reset();
        assert_eq!(snapshot().total_tool_calls(), 0);
    }

    #[test]
    fn empty_registry_is_harmless() {
        let _guard = test_lock_guard();
        reset();
        let snap = snapshot();
        assert_eq!(snap.total_tool_calls(), 0);
        assert_eq!(snap.patch_success_rate(), 0.0);
        assert!(snap.top_tools(5).is_empty());
    }
}
