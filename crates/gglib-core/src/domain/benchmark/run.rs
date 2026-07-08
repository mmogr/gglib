//! Benchmark run metadata: type, lifecycle status, and the run record itself.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

/// Whether a benchmark run measured inference quality/speed or raw throughput.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BenchmarkRunType {
    /// Prompt-comparison run: N models answer the same prompt.
    Compare,
    /// Performance run: `llama-bench` reports raw pp/tg tokens/sec.
    Perf,
}

/// Lifecycle state of a benchmark run.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BenchmarkRunStatus {
    /// Run is currently in progress.
    Running,
    /// Run finished successfully.
    Complete,
    /// Run encountered an error or was aborted.
    Failed,
}

/// Lightweight record grouping one or more model results under a single run.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BenchmarkRun {
    /// Database ID of the run.
    pub id: i64,
    /// Whether this is a compare or perf run.
    pub run_type: BenchmarkRunType,
    /// Current lifecycle state.
    pub status: BenchmarkRunStatus,
    /// Ordered list of model IDs that were (or will be) benchmarked.
    pub model_ids: Vec<i64>,
    /// Prompt text used for compare runs (absent for perf runs).
    pub prompt_text: Option<String>,
    /// System prompt used for compare runs.
    pub system_prompt: Option<String>,
    /// Serialised run configuration (`CompareConfig` or `PerfConfig` JSON).
    pub config_json: Option<String>,
    /// Error message if the run failed.
    pub error: Option<String>,
    /// UTC timestamp when the run was created.
    pub created_at: DateTime<Utc>,
    /// UTC timestamp when the run completed or failed.
    pub completed_at: Option<DateTime<Utc>>,
}
