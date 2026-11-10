//! Compiled plugin feasibility harness. The `paperphase_v0` ABI is experimental.

mod runtime;

pub use paperphase_core::{Dataset, Field, FieldKind};
pub use runtime::{Limits, PluginRuntime, PreparedPlugin, RunConfig, RunMetrics, RunResult};
