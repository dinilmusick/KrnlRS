//! Runtime-neutral ULTRST Rust kernel primitives.
//! Cross-service orchestration belongs to Skipo, not Krnl.

pub mod lifecycle;
pub use lifecycle::{Lifecycle, LifecycleError, LifecycleState};

pub mod telemetry;
pub use telemetry::{InvocationTrace, OperationMetrics, Telemetry, TelemetryMode, TelemetrySnapshot};
