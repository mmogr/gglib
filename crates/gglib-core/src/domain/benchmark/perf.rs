//! Perf-mode benchmark types: `llama-bench` configuration and results.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

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
