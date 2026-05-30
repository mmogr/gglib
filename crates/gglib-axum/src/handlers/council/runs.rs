//! `GET /api/council/runs` — list orchestrator run records.
//! `GET /api/council/runs/{id}` — get a single run with its event log.

use axum::Json;
use axum::extract::{Path, Query, State};

use gglib_core::domain::council::run::{CouncilRun, CouncilRunEvent, CouncilRunStatus};
use gglib_core::ports::CouncilRepositoryPort;

use crate::error::HttpError;
use crate::state::AppState;

// ─── DTOs ─────────────────────────────────────────────────────────────────────

/// Query parameters for `GET /api/council/runs`.
#[derive(Debug, serde::Deserialize, Default)]
pub struct ListRunsQuery {
    /// Optional status filter.  Values: `running`, `awaiting_approval`,
    /// `interrupted`, `completed`, `failed`.
    #[serde(default)]
    pub status: Option<String>,
}

/// Response body for `GET /api/council/runs`.
#[derive(Debug, serde::Serialize)]
pub struct ListRunsResponse {
    pub runs: Vec<CouncilRun>,
}

/// Response body for `GET /api/council/runs/{id}`.
#[derive(Debug, serde::Serialize)]
pub struct GetRunResponse {
    pub run: CouncilRun,
    pub events: Vec<CouncilRunEvent>,
}

// ─── GET /api/council/runs ──────────────────────────────────────────────

/// List orchestrator runs, optionally filtered by status.
///
/// # Errors
///
/// Returns 400 on an invalid status value.
pub async fn list_runs(
    State(state): State<AppState>,
    Query(q): Query<ListRunsQuery>,
) -> Result<Json<ListRunsResponse>, HttpError> {
    let status_filter = q.status.as_deref().map(parse_status).transpose()?;

    let runs = state
        .council_repo
        .list_runs(status_filter)
        .await
        .map_err(|e| HttpError::Internal(e.to_string()))?;

    Ok(Json(ListRunsResponse { runs }))
}

// ─── GET /api/council/runs/{id} ─────────────────────────────────────────

/// Get a single orchestrator run and its event log.
///
/// # Errors
///
/// Returns 404 if the run is not found.
pub async fn get_run(
    State(state): State<AppState>,
    Path(run_id): Path<String>,
) -> Result<Json<GetRunResponse>, HttpError> {
    let run = state
        .council_repo
        .get_run(&run_id)
        .await
        .map_err(|e| HttpError::Internal(e.to_string()))?
        .ok_or_else(|| HttpError::NotFound(format!("run '{run_id}' not found")))?;

    let events = state
        .council_repo
        .list_events(&run_id)
        .await
        .map_err(|e| HttpError::Internal(e.to_string()))?;

    Ok(Json(GetRunResponse { run, events }))
}

// ─── Helpers ──────────────────────────────────────────────────────────────────

fn parse_status(s: &str) -> Result<CouncilRunStatus, HttpError> {
    match s {
        "running" => Ok(CouncilRunStatus::Running),
        "awaiting_approval" => Ok(CouncilRunStatus::AwaitingApproval),
        "interrupted" => Ok(CouncilRunStatus::Interrupted),
        "completed" => Ok(CouncilRunStatus::Completed),
        "failed" => Ok(CouncilRunStatus::Failed),
        other => Err(HttpError::BadRequest(format!(
            "unknown status '{other}'; valid values: running, awaiting_approval, interrupted, completed, failed"
        ))),
    }
}
