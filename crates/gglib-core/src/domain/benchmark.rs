//! Benchmark domain types.
//!
//! Two benchmark modes:
//! - **Compare**: send the same prompt to N models sequentially; capture live
//!   streamed text and real-world timing data from llama-server's `timings`
//!   response field.
//! - **Perf**: run `llama-bench` for raw prompt-processing (pp) and
//!   token-generation (tg) throughput in tokens/sec.
//!
//! All timing fields are `Option<f64>` because llama-server may omit the
//! `timings` object (e.g. older builds, stream errors). Missing timing data
//! is gracefully represented as `None` — never causes a panic or parse error.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use super::inference::InferenceConfig;

// ────────────────────────────────────────────────────────────────────────────
// Run metadata
// ────────────────────────────────────────────────────────────────────────────

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

// ────────────────────────────────────────────────────────────────────────────
// Per-model results
// ────────────────────────────────────────────────────────────────────────────

/// Result of running a single model through a compare (inference) benchmark.
///
/// All timing fields are `Option<f64>` — llama-server may omit the `timings`
/// object. Missing values are stored as `NULL` in the database and surfaced as
/// `None` in the API; they never cause a panic.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelCompareResult {
    /// Database ID of this result row (set after persistence).
    pub id: Option<i64>,
    /// Foreign key → `models.id`.
    pub model_id: i64,
    /// Foreign key → `benchmark_runs.id` (nullable; SET NULL on run delete).
    pub run_id: Option<i64>,
    /// Prompt text sent to this model.
    pub prompt_text: String,
    /// Optional system prompt.
    pub system_prompt: Option<String>,
    /// Full response text accumulated from the stream.
    pub response_text: String,
    /// `true` if the response was cut short (`finish_reason == "length"`).
    pub was_truncated: bool,
    /// Number of prompt tokens reported by the model.
    pub prompt_tokens: Option<i64>,
    /// Number of completion tokens reported by the model.
    pub completion_tokens: Option<i64>,
    /// Time spent processing the prompt (milliseconds).
    pub prompt_ms: Option<f64>,
    /// Time spent generating the response (milliseconds).
    pub generation_ms: Option<f64>,
    /// Prompt-processing throughput (tokens/sec). `None` if timings absent.
    pub prompt_tps: Option<f64>,
    /// Token-generation throughput (tokens/sec). `None` if timings absent.
    pub generation_tps: Option<f64>,
    /// UTC timestamp of this result.
    pub created_at: DateTime<Utc>,
}

/// Result of running `llama-bench` on a single model.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelPerfResult {
    /// Database ID of this result row (set after persistence).
    pub id: Option<i64>,
    /// Foreign key → `models.id`.
    pub model_id: i64,
    /// Foreign key → `benchmark_runs.id` (nullable; SET NULL on run delete).
    pub run_id: Option<i64>,
    /// Prompt-processing throughput (tokens/sec).
    pub pp_tps: f64,
    /// Token-generation throughput (tokens/sec).
    pub tg_tps: f64,
    /// Number of prompt tokens used in the benchmark.
    pub pp_tokens: i64,
    /// Number of generation tokens used in the benchmark.
    pub tg_tokens: i64,
    /// Backend reported by llama-bench (e.g. "Metal", "CUDA", "CPU").
    pub backend: Option<String>,
    /// Number of GPU layers offloaded.
    pub ngl: Option<i64>,
    /// Context size used.
    pub context_size: Option<i64>,
    /// Number of repetitions averaged.
    pub repetitions: i64,
    /// UTC timestamp of this result.
    pub created_at: DateTime<Utc>,
}

// ────────────────────────────────────────────────────────────────────────────
// Per-model summary (denormalised aggregate kept in sync with results)
// ────────────────────────────────────────────────────────────────────────────

/// Denormalised benchmark summary for a single model.
///
/// Upserted in the same transaction as each new result so that the model list
/// query can LEFT JOIN this table and show speed badges without extra round-trips.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelBenchmarkSummary {
    /// Foreign key → `models.id`.
    pub model_id: i64,
    /// Best token-generation throughput across all perf runs.
    pub best_tg_tps: Option<f64>,
    /// Best prompt-processing throughput across all perf runs.
    pub best_pp_tps: Option<f64>,
    /// Token-generation throughput from the most recent perf run.
    pub latest_tg_tps: Option<f64>,
    /// Prompt-processing throughput from the most recent perf run.
    pub latest_pp_tps: Option<f64>,
    /// Backend reported by the most recent perf run.
    pub latest_backend: Option<String>,
    /// Total number of perf runs recorded for this model.
    pub perf_run_count: i64,
    /// Total number of compare runs recorded for this model.
    pub compare_run_count: i64,
    /// UTC timestamp of the most recent benchmark (either type).
    pub last_benchmarked_at: DateTime<Utc>,
    /// UTC timestamp of the last summary update.
    pub updated_at: DateTime<Utc>,
}

// ────────────────────────────────────────────────────────────────────────────
// Configuration
// ────────────────────────────────────────────────────────────────────────────

/// Configuration for a compare benchmark run.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CompareConfig {
    /// Models to benchmark (by database ID), run sequentially.
    pub model_ids: Vec<i64>,
    /// Prompt text to send to every model.
    pub prompt: String,
    /// Optional system prompt.
    pub system_prompt: Option<String>,
    /// Per-request inference overrides (temperature, context size, etc.).
    pub inference: Option<InferenceConfig>,
}

/// Configuration for a performance (`llama-bench`) run.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PerfConfig {
    /// Models to benchmark (by database ID), run sequentially.
    pub model_ids: Vec<i64>,
    /// Number of prompt tokens to use in the benchmark.
    #[serde(default = "PerfConfig::default_pp_tokens")]
    pub pp_tokens: u32,
    /// Number of generation tokens to use in the benchmark.
    #[serde(default = "PerfConfig::default_tg_tokens")]
    pub tg_tokens: u32,
    /// Number of repetitions to average.
    #[serde(default = "PerfConfig::default_repetitions")]
    pub repetitions: u32,
}

impl PerfConfig {
    const fn default_pp_tokens() -> u32 {
        512
    }
    const fn default_tg_tokens() -> u32 {
        128
    }
    const fn default_repetitions() -> u32 {
        3
    }
}

impl Default for PerfConfig {
    fn default() -> Self {
        Self {
            model_ids: vec![],
            pp_tokens: Self::default_pp_tokens(),
            tg_tokens: Self::default_tg_tokens(),
            repetitions: Self::default_repetitions(),
        }
    }
}

// ────────────────────────────────────────────────────────────────────────────
// SSE / channel event enum
// ────────────────────────────────────────────────────────────────────────────

/// Typed event emitted over the mpsc channel (and serialised as SSE to the
/// browser) during a benchmark run.
///
/// Both the CLI renderer and the Axum SSE bridge consume this enum, giving
/// full feature parity between CLI and web interfaces.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum BenchmarkEvent {
    /// A model is about to start (position is 1-based).
    ModelStarted {
        model_id: i64,
        model_name: String,
        position: usize,
        total: usize,
    },
    /// A chunk of generated text for one model (compare runs only).
    ModelTextDelta { model_id: i64, text: String },
    /// A model finished successfully.
    ModelComplete {
        model_id: i64,
        result: BenchmarkModelResult,
    },
    /// A model failed (e.g. binary not found, OOM).
    ModelFailed {
        model_id: i64,
        model_name: String,
        error: String,
    },
    /// All models finished; the run record is now `Complete`.
    RunComplete { run_id: i64 },
    /// The entire run failed (e.g. DB error, abort).
    RunFailed { error: String },
}

/// Wraps either a compare or perf result for `BenchmarkEvent::ModelComplete`.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum BenchmarkModelResult {
    /// Result from a compare run.
    Compare(ModelCompareResult),
    /// Result from a perf run.
    Perf(ModelPerfResult),
}
