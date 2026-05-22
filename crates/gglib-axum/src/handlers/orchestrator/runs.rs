//! `GET /api/orchestrator/runs` — list orchestrator run records.
//! `GET /api/orchestrator/runs/{id}` — get a single run with its event log.

use axum::Json;
use axum::extract::{Path, Query, State};

use gglib_core::domain::orchestrator::run::{
    OrchestratorRun, OrchestratorRunEvent, OrchestratorRunStatus,
};
use gglib_core::ports::OrchestratorRepositoryPort;

use crate::error::HttpError;
use crate::state::AppState;

// ─── DTOs ─────────────────────────────────────────────────────────────────────

/// Query parameters for `GET /api/orchestrator/runs`.
#[derive(Debug, serde::Deserialize, Default)]
pub struct ListRunsQuery {
    /// Optional status filter.  Values: `running`, `awaiting_approval`,
    /// `interrupted`, `completed`, `failed`.
    #[serde(default)]
    pub status: Option<String>,
}

/// Response body for `GET /api/orchestrator/runs`.
#[derive(Debug, serde::Serialize)]
pub struct ListRunsResponse {
    pub runs: Vec<OrchestratorRun>,
}

/// Response body for `GET /api/orchestrator/runs/{id}`.
#[derive(Debug, serde::Serialize)]
pub struct GetRunResponse {
    pub run: OrchestratorRun,
    pub events: Vec<OrchestratorRunEvent>,
}

// ─── GET /api/orchestrator/runs ──────────────────────────────────────────────

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
        .orchestrator_repo
        .list_runs(status_filter)
        .await
        .map_err(|e| HttpError::Internal(e.to_string()))?;

    Ok(Json(ListRunsResponse { runs }))
}

// ─── GET /api/orchestrator/runs/{id} ─────────────────────────────────────────

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
        .orchestrator_repo
        .get_run(&run_id)
        .await
        .map_err(|e| HttpError::Internal(e.to_string()))?
        .ok_or_else(|| HttpError::NotFound(format!("run '{run_id}' not found")))?;

    let events = state
        .orchestrator_repo
        .list_events(&run_id)
        .await
        .map_err(|e| HttpError::Internal(e.to_string()))?;

    Ok(Json(GetRunResponse { run, events }))
}

// ─── Helpers ──────────────────────────────────────────────────────────────────

fn parse_status(s: &str) -> Result<OrchestratorRunStatus, HttpError> {
    match s {
        "running" => Ok(OrchestratorRunStatus::Running),
        "awaiting_approval" => Ok(OrchestratorRunStatus::AwaitingApproval),
        "interrupted" => Ok(OrchestratorRunStatus::Interrupted),
        "completed" => Ok(OrchestratorRunStatus::Completed),
        "failed" => Ok(OrchestratorRunStatus::Failed),
        other => Err(HttpError::BadRequest(format!(
            "unknown status '{other}'; valid values: running, awaiting_approval, interrupted, completed, failed"
        ))),
    }
}
