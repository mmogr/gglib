//! Compare-mode benchmark types: configuration and per-model results.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::domain::inference::InferenceConfig;

/// Configuration for a compare benchmark run.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CompareConfig {
    /// Models to benchmark (by database ID), run sequentially.
    pub model_ids: Vec<i64>,
    /// Prompt text to send to every model.
    pub prompt: String,
    /// Optional system prompt.
    pub system_prompt: Option<String>,
    /// Per-request inference overrides (`temperature`, `max_tokens`, etc.).
    pub inference: Option<InferenceConfig>,
    /// Override the llama-server context window size for this run.
    ///
    /// When `None` the benchmark service falls back to the app-wide
    /// `default_context_size` setting (same fallback the proxy uses).
    #[serde(default)]
    pub ctx_size: Option<u64>,
}

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
