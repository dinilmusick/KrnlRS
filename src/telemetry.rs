//! Runtime-neutral, opt-in service invocation telemetry.
//! This module does not sample OS resources or perform cross-service orchestration.

use std::collections::{BTreeMap, VecDeque};
use std::time::Duration;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TelemetryMode {
    Off,
    Standard,
    Detailed,
    Profile,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InvocationTrace {
    pub operation: String,
    pub correlation_id: Option<String>,
    pub elapsed: Duration,
    pub succeeded: bool,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct OperationMetrics {
    pub invocations: u64,
    pub successes: u64,
    pub failures: u64,
    pub total_duration: Duration,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TelemetrySnapshot {
    pub mode: TelemetryMode,
    pub operations: BTreeMap<String, OperationMetrics>,
    pub recent: Vec<InvocationTrace>,
}

#[derive(Debug)]
pub struct Telemetry {
    mode: TelemetryMode,
    capacity: usize,
    operations: BTreeMap<String, OperationMetrics>,
    recent: VecDeque<InvocationTrace>,
}

impl Default for Telemetry {
    fn default() -> Self {
        Self::new(128)
    }
}

impl Telemetry {
    pub fn new(capacity: usize) -> Self {
        Self {
            mode: TelemetryMode::Off,
            capacity,
            operations: BTreeMap::new(),
            recent: VecDeque::new(),
        }
    }

    /// Reconfigure trace retention without changing aggregate metrics.
    /// Reducing capacity evicts the oldest traces first.
    pub fn set_capacity(&mut self, capacity: usize) {
        self.capacity = capacity;
        while self.recent.len() > capacity {
            self.recent.pop_front();
        }
    }

    pub fn enable(&mut self) {
        self.mode = TelemetryMode::Standard;
    }

    pub fn set_mode(&mut self, mode: TelemetryMode) {
        self.mode = mode;
    }

    pub fn status(&self) -> TelemetryMode {
        self.mode
    }

    pub fn reset(&mut self) {
        self.operations.clear();
        self.recent.clear();
    }

    pub fn functionalities(&self) -> Vec<&str> {
        self.operations.keys().map(String::as_str).collect()
    }

    pub fn functionality(&self, name: &str) -> Option<&OperationMetrics> {
        self.operations.get(name)
    }

    pub fn snapshot(&self) -> TelemetrySnapshot {
        TelemetrySnapshot {
            mode: self.mode,
            operations: self.operations.clone(),
            recent: self.recent.iter().cloned().collect(),
        }
    }

    /// Execute an operation and record its result without requiring callers to
    /// manually construct a trace. Errors are returned unchanged.
    ///
    /// This instruments a single functionality, not cross-service orchestration.
    /// Panics are deliberately not caught or converted to successful results.
    pub fn measure<T, E, F>(
        &mut self,
        operation: impl Into<String>,
        correlation_id: Option<String>,
        action: F,
    ) -> Result<T, E>
    where
        F: FnOnce() -> Result<T, E>,
    {
        if self.mode == TelemetryMode::Off {
            return action();
        }
        let started = std::time::Instant::now();
        let result = action();
        self.record(InvocationTrace {
            operation: operation.into(),
            correlation_id,
            elapsed: started.elapsed(),
            succeeded: result.is_ok(),
        });
        result
    }

    /// Record a completed operation. The caller owns timing and correlation.
    /// Disabled telemetry never stores the operation name or trace.
    pub fn record(&mut self, trace: InvocationTrace) {
        if self.mode == TelemetryMode::Off {
            return;
        }
        let entry = self.operations.entry(trace.operation.clone()).or_default();
        entry.invocations = entry.invocations.saturating_add(1);
        if trace.succeeded {
            entry.successes = entry.successes.saturating_add(1);
        } else {
            entry.failures = entry.failures.saturating_add(1);
        }
        entry.total_duration = entry.total_duration.saturating_add(trace.elapsed);
        if self.capacity > 0 {
            while self.recent.len() >= self.capacity {
                self.recent.pop_front();
            }
            self.recent.push_back(trace);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn trace(name: &str, succeeded: bool) -> InvocationTrace {
        InvocationTrace {
            operation: name.into(),
            correlation_id: Some("req-1".into()),
            elapsed: Duration::from_millis(10),
            succeeded,
        }
    }

    #[test]
    fn disabled_by_default_and_off_is_non_recording() {
        let mut telemetry = Telemetry::new(2);
        telemetry.record(trace("secret", false));
        assert!(telemetry.snapshot().operations.is_empty());
        telemetry.enable();
        telemetry.record(trace("allowed", true));
        telemetry.set_mode(TelemetryMode::Off);
        telemetry.record(trace("secret", false));
        assert_eq!(telemetry.functionalities(), vec!["allowed"]);
    }

    #[test]
    fn metrics_and_bounded_traces_are_consistent() {
        let mut telemetry = Telemetry::new(2);
        telemetry.enable();
        telemetry.record(trace("alpha", true));
        telemetry.record(trace("alpha", false));
        telemetry.record(trace("beta", true));
        let alpha = telemetry.functionality("alpha").unwrap();
        assert_eq!((alpha.invocations, alpha.successes, alpha.failures), (2, 1, 1));
        assert_eq!(alpha.total_duration, Duration::from_millis(20));
        let snapshot = telemetry.snapshot();
        assert_eq!(snapshot.recent.len(), 2);
        assert_eq!(snapshot.recent[0].operation, "alpha");
        assert_eq!(snapshot.recent[1].operation, "beta");
        telemetry.reset();
        assert!(telemetry.snapshot().recent.is_empty());
        assert!(telemetry.functionalities().is_empty());
    }

    #[test]
    fn zero_capacity_keeps_metrics_without_traces() {
        let mut telemetry = Telemetry::new(0);
        telemetry.set_mode(TelemetryMode::Detailed);
        telemetry.record(trace("run", true));
        assert!(telemetry.snapshot().recent.is_empty());
        assert_eq!(telemetry.functionality("run").unwrap().invocations, 1);
    }
    #[test]
    fn measured_invocations_preserve_results_and_correlation() {
        let mut telemetry = Telemetry::new(2);
        telemetry.enable();
        let ok: Result<i32, &str> =
            telemetry.measure("calculate", Some("trace-42".into()), || Ok(42));
        let error: Result<(), &str> =
            telemetry.measure("calculate", Some("trace-43".into()), || Err("failed"));
        assert_eq!(ok, Ok(42));
        assert_eq!(error, Err("failed"));
        let metrics = telemetry.functionality("calculate").unwrap();
        assert_eq!((metrics.invocations, metrics.successes, metrics.failures), (2, 1, 1));
        let snapshot = telemetry.snapshot();
        assert_eq!(snapshot.recent[0].correlation_id.as_deref(), Some("trace-42"));
        assert_eq!(snapshot.recent[1].correlation_id.as_deref(), Some("trace-43"));
        assert!(!snapshot.recent[1].succeeded);
    }

    #[test]
    fn capacity_reconfiguration_preserves_metrics_and_newest_traces() {
        let mut telemetry = Telemetry::new(3);
        telemetry.enable();
        telemetry.record(trace("first", true));
        telemetry.record(trace("second", false));
        telemetry.record(trace("third", true));
        telemetry.set_capacity(1);
        let snapshot = telemetry.snapshot();
        assert_eq!(snapshot.recent.len(), 1);
        assert_eq!(snapshot.recent[0].operation, "third");
        assert_eq!(snapshot.operations.len(), 3);
        telemetry.set_capacity(0);
        assert!(telemetry.snapshot().recent.is_empty());
        telemetry.record(trace("fourth", true));
        assert!(telemetry.snapshot().recent.is_empty());
        assert_eq!(telemetry.functionality("fourth").unwrap().invocations, 1);
        telemetry.set_capacity(2);
        telemetry.record(trace("fifth", true));
        assert_eq!(telemetry.snapshot().recent[0].operation, "fifth");
    }

    #[test]
    fn retention_remains_bounded_across_repeated_capacity_changes() {
        let mut telemetry = Telemetry::new(4);
        telemetry.enable();
        for i in 0..20 {
            telemetry.set_capacity(i % 5);
            telemetry.record(trace(&format!("op-{i}"), i % 2 == 0));
            let snapshot = telemetry.snapshot();
            assert!(snapshot.recent.len() <= i % 5);
            assert_eq!(snapshot.operations.len(), i + 1);
        }
        telemetry.reset();
        assert!(telemetry.snapshot().operations.is_empty());
        assert_eq!(telemetry.status(), TelemetryMode::Standard);
        telemetry.record(trace("after-reset", true));
        assert_eq!(telemetry.functionality("after-reset").unwrap().invocations, 1);
    }

    #[test]
    fn off_mode_executes_once_without_recording() {
        let mut telemetry = Telemetry::default();
        let mut calls = 0;
        let result: Result<usize, ()> = telemetry.measure("private", None, || {
            calls += 1;
            Ok(calls)
        });
        assert_eq!(result, Ok(1));
        assert_eq!(calls, 1);
        assert!(telemetry.snapshot().operations.is_empty());
    }


}
