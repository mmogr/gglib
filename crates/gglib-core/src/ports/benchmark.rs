//! Benchmark repository port definition.
//!
//! Defines the interface for persisting benchmark runs and results.
//! Implementations live in `gglib-db`; this trait contains only domain types.

use async_trait::async_trait;

use super::RepositoryError;
use crate::domain::{
    BenchmarkRun, BenchmarkRunType, ModelBenchmarkSummary, ModelCompareResult, ModelPerfResult,
};

/// Repository interface for benchmark persistence.
///
/// Implementations are responsible for:
/// - Creating and updating run records
/// - Storing per-model compare and perf results
/// - Upserting `model_benchmark_summaries` in the same transaction as each
///   result save, so the model list query always has fresh summary data
///   without extra round-trips
#[async_trait]
pub trait BenchmarkRepositoryPort: Send + Sync {
    /// Create a new benchmark run record in `Running` status.
    ///
    /// Returns the auto-assigned run ID.
    async fn create_run(
        &self,
        run_type: BenchmarkRunType,
        model_ids: &[i64],
        prompt_text: Option<&str>,
        system_prompt: Option<&str>,
        config_json: Option<&str>,
    ) -> Result<i64, RepositoryError>;

    /// Mark a run as `Complete` and record the completion timestamp.
    async fn complete_run(&self, run_id: i64) -> Result<(), RepositoryError>;

    /// Mark a run as `Failed` and record the error message.
    async fn fail_run(&self, run_id: i64, error: &str) -> Result<(), RepositoryError>;

    /// Persist a compare result and upsert the model's benchmark summary.
    ///
    /// Both the result INSERT and the summary upsert happen in the same
    /// database transaction to keep the denormalised summary consistent.
    ///
    /// Returns the auto-assigned result ID.
    async fn save_compare_result(
        &self,
        result: &ModelCompareResult,
        run_id: i64,
    ) -> Result<i64, RepositoryError>;

    /// Persist a perf result and upsert the model's benchmark summary.
    ///
    /// Both the result INSERT and the summary upsert happen in the same
    /// database transaction to keep the denormalised summary consistent.
    ///
    /// Returns the auto-assigned result ID.
    async fn save_perf_result(
        &self,
        result: &ModelPerfResult,
        run_id: i64,
    ) -> Result<i64, RepositoryError>;

    /// List benchmark runs, most recent first.
    async fn list_runs(
        &self,
        limit: i64,
        offset: i64,
    ) -> Result<Vec<BenchmarkRun>, RepositoryError>;

    /// Get a single benchmark run by ID.
    async fn get_run(&self, run_id: i64) -> Result<Option<BenchmarkRun>, RepositoryError>;

    /// Get compare results for one model, most recent first.
    async fn get_model_compare_history(
        &self,
        model_id: i64,
        limit: i64,
    ) -> Result<Vec<ModelCompareResult>, RepositoryError>;

    /// Get perf results for one model, most recent first.
    async fn get_model_perf_history(
        &self,
        model_id: i64,
        limit: i64,
    ) -> Result<Vec<ModelPerfResult>, RepositoryError>;

    /// Get the denormalised benchmark summary for one model.
    ///
    /// Returns `None` if no benchmark has been run for this model.
    async fn get_model_summary(
        &self,
        model_id: i64,
    ) -> Result<Option<ModelBenchmarkSummary>, RepositoryError>;
}
