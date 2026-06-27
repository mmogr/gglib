//! `GET /api/benchmark/runs` — list benchmark runs (paginated).
//! `GET /api/benchmark/runs/{id}` — get a single run by ID.
//! `GET /api/models/{id}/benchmark` — get all benchmark history for one model.

use axum::Json;
use axum::extract::{Path, Query, State};

use gglib_core::domain::benchmark::{
    BenchmarkRun, ModelBenchmarkSummary, ModelCompareResult, ModelPerfResult,
};
use gglib_core::ports::BenchmarkRepositoryPort as _;

use crate::error::HttpError;
use crate::state::AppState;

// ─── DTOs ─────────────────────────────────────────────────────────────────────

/// Query parameters for `GET /api/benchmark/runs`.
#[derive(Debug, serde::Deserialize)]
pub struct ListRunsQuery {
    /// Maximum number of runs to return (default: 20, max: 100).
    #[serde(default = "default_limit")]
    pub limit: i64,
    /// Number of runs to skip for pagination (default: 0).
    #[serde(default)]
    pub offset: i64,
}

fn default_limit() -> i64 {
    20
}

/// Response body for `GET /api/benchmark/runs`.
#[derive(Debug, serde::Serialize)]
pub struct ListRunsResponse {
    pub runs: Vec<BenchmarkRun>,
}

/// Response body for `GET /api/benchmark/runs/{id}`.
#[derive(Debug, serde::Serialize)]
pub struct GetRunResponse {
    pub run: BenchmarkRun,
}

/// Query parameters for `GET /api/models/{id}/benchmark`.
#[derive(Debug, serde::Deserialize)]
pub struct ModelBenchmarkQuery {
    /// Maximum number of compare results to return (default: 20).
    #[serde(default = "default_limit")]
    pub limit: i64,
}

/// Response body for `GET /api/models/{id}/benchmark`.
#[derive(Debug, serde::Serialize)]
pub struct ModelBenchmarkResponse {
    pub summary: Option<ModelBenchmarkSummary>,
    pub compare_history: Vec<ModelCompareResult>,
    pub perf_history: Vec<ModelPerfResult>,
}

// ─── Handlers ─────────────────────────────────────────────────────────────────

/// `GET /api/benchmark/runs` — list recent benchmark runs.
pub async fn list_runs(
    State(state): State<AppState>,
    Query(params): Query<ListRunsQuery>,
) -> Result<Json<ListRunsResponse>, HttpError> {
    let limit = params.limit.clamp(1, 100);
    let runs = state.bench_repo.list_runs(limit, params.offset).await?;
    Ok(Json(ListRunsResponse { runs }))
}

/// `GET /api/benchmark/runs/{id}` — get a single benchmark run by ID.
pub async fn get_run(
    State(state): State<AppState>,
    Path(id): Path<i64>,
) -> Result<Json<GetRunResponse>, HttpError> {
    let run = state
        .bench_repo
        .get_run(id)
        .await?
        .ok_or_else(|| HttpError::NotFound(format!("benchmark run #{id} not found")))?;
    Ok(Json(GetRunResponse { run }))
}

/// `GET /api/models/{id}/benchmark` — get the full benchmark history for one model.
pub async fn model_benchmark(
    State(state): State<AppState>,
    Path(id): Path<i64>,
    Query(params): Query<ModelBenchmarkQuery>,
) -> Result<Json<ModelBenchmarkResponse>, HttpError> {
    let limit = params.limit.clamp(1, 100);

    let (summary, compare_history, perf_history) = tokio::try_join!(
        state.bench_repo.get_model_summary(id),
        state.bench_repo.get_model_compare_history(id, limit),
        state.bench_repo.get_model_perf_history(id, limit),
    )?;

    Ok(Json(ModelBenchmarkResponse {
        summary,
        compare_history,
        perf_history,
    }))
}
