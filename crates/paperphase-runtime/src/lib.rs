//! Compiled plugin feasibility harness. The `paperphase_v0` ABI is experimental.

mod dataset;
mod runtime;

pub use dataset::{Dataset, Field, FieldKind};
pub use runtime::{Limits, PluginRuntime, PreparedPlugin, RunConfig, RunMetrics, RunResult};
