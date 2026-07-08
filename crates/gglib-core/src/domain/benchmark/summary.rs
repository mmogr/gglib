//! Denormalised per-model benchmark summary.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

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
