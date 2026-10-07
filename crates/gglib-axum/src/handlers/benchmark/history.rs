//! `GET /api/benchmark/runs` — list benchmark runs (paginated).
//! `GET /api/models/{id}/agentic-history` — past A/B reports for one model.

use axum::Json;
use axum::extract::{Path, Query, State};

use gglib_core::domain::benchmark::BenchmarkRun;
use gglib_core::domain::benchmark::agentic::AgenticEvalReport;
use gglib_core::ports::BenchmarkRepositoryPort as _;

use crate::error::HttpError;
use crate::state::AppState;

// ─── DTOs ─────────────────────────────────────────────────────────────────────

/// Query parameters for `GET /api/benchmark/runs`.
#[derive(Debug, serde::Deserialize)]
#[cfg_attr(feature = "ts-bindings", derive(ts_rs::TS), ts(export))]
pub(crate) struct ListRunsQuery {
    /// Maximum number of runs to return (default: 20, max: 100).
    #[cfg_attr(feature = "ts-bindings", ts(type = "number"))]
    #[serde(default = "default_limit")]
    pub limit: i64,
    /// Number of runs to skip for pagination (default: 0).
    #[cfg_attr(feature = "ts-bindings", ts(type = "number"))]
    #[serde(default)]
    pub offset: i64,
}

fn default_limit() -> i64 {
    20
}

/// Response body for `GET /api/benchmark/runs`.
#[derive(Debug, serde::Serialize)]
#[cfg_attr(feature = "ts-bindings", derive(ts_rs::TS), ts(export))]
pub(crate) struct ListRunsResponse {
    pub runs: Vec<BenchmarkRun>,
}

/// Query parameters for `GET /api/models/{id}/agentic-history`.
#[derive(Debug, serde::Deserialize)]
#[cfg_attr(feature = "ts-bindings", derive(ts_rs::TS), ts(export))]
pub(crate) struct ModelAgenticHistoryQuery {
    /// Maximum number of A/B reports to return (default: 20).
    #[cfg_attr(feature = "ts-bindings", ts(type = "number"))]
    #[serde(default = "default_limit")]
    pub limit: i64,
}

/// Response body for `GET /api/models/{id}/agentic-history`.
#[derive(Debug, serde::Serialize)]
#[cfg_attr(feature = "ts-bindings", derive(ts_rs::TS), ts(export))]
pub(crate) struct ModelAgenticHistoryResponse {
    pub reports: Vec<AgenticEvalReport>,
}

// ─── Handlers ─────────────────────────────────────────────────────────────────

/// `GET /api/benchmark/runs` — list recent benchmark runs.
pub(crate) async fn list_runs(
    State(state): State<AppState>,
    Query(params): Query<ListRunsQuery>,
) -> Result<Json<ListRunsResponse>, HttpError> {
    let limit = params.limit.clamp(1, 100);
    let runs = state.bench_repo.list_runs(limit, params.offset).await?;
    Ok(Json(ListRunsResponse { runs }))
}

/// `GET /api/models/{id}/agentic-history` — past raw-vs-gglib A/B reports for
/// one model, most recent first.
pub(crate) async fn model_agentic_history(
    State(state): State<AppState>,
    Path(id): Path<i64>,
    Query(params): Query<ModelAgenticHistoryQuery>,
) -> Result<Json<ModelAgenticHistoryResponse>, HttpError> {
    let limit = params.limit.clamp(1, 100);
    let reports = state
        .bench_repo
        .get_model_agentic_history(id, limit)
        .await?;
    Ok(Json(ModelAgenticHistoryResponse { reports }))
}
