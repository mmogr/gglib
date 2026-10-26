//! Setup handlers - first-run wizard system status and provisioning.

use axum::Json;
use axum::extract::State;
use serde::Serialize;

use crate::dto::diagnostics::{DiagnosticsDto, RecommendationDto, ResolvedPathsDto};
use crate::error::HttpError;
use crate::state::AppState;
use gglib_app_services::setup::SetupStatus;
use gglib_core::domain::RuntimeKind;

/// Get the full system setup status for the first-run wizard, with the chat
/// model llama-server serves, if any, under which no update is offered.
pub(crate) async fn status(State(state): State<AppState>) -> Result<Json<SetupStatus>, HttpError> {
    let mut status = state.setup.get_status().await?;
    status.llama_serving = state
        .setup
        .serving(state.runtime.as_ref(), RuntimeKind::Llama)
        .await;
    Ok(Json(status))
}

/// Set up the Python fast-download helper environment.
pub(crate) async fn setup_python(State(state): State<AppState>) -> Result<Json<()>, HttpError> {
    state.setup.setup_python_env().await?;
    Ok(Json(()))
}

/// Remove the fast-download helper environment — downloads revert to native
/// HTTP, which is a speed change, not a loss of capability.
pub(crate) async fn disable_fast_downloads(
    State(state): State<AppState>,
) -> Result<Json<DisableFastDownloadsResponse>, HttpError> {
    Ok(Json(DisableFastDownloadsResponse {
        removed: state.setup.remove_python_env()?,
    }))
}

/// Whether disabling actually removed anything, so the GUI can say
/// "disabled" rather than claiming a change that did not happen.
#[derive(Debug, Serialize)]
#[cfg_attr(feature = "ts-bindings", derive(ts_rs::TS), ts(export))]
#[serde(rename_all = "camelCase")]
pub(crate) struct DisableFastDownloadsResponse {
    pub removed: bool,
}

/// System diagnostics: dependency matrix, resolved paths, detected
/// acceleration and accelerator state — `gglib config check-deps`, `paths`
/// and `fast-downloads status` in one response.
pub(crate) async fn diagnostics(
    State(state): State<AppState>,
) -> Result<Json<DiagnosticsDto>, HttpError> {
    let d = state.setup.diagnostics()?;

    Ok(Json(DiagnosticsDto {
        dependencies: d.dependencies.iter().map(Into::into).collect(),
        paths: ResolvedPathsDto::from(d.paths),
        acceleration: d.acceleration,
        fast_downloads: d.fast_downloads,
    }))
}

/// A hardware-sized model suggestion — the shortlist `gglib up` picks from,
/// sized against this machine's memory.
///
/// Returns `null` when nothing in the shortlist fits. That is a real answer,
/// not an error: a machine too small for the smallest candidate needs to be
/// told so, not handed a recommendation it cannot run.
pub(crate) async fn recommend_model(
    State(state): State<AppState>,
) -> Result<Json<Option<RecommendationDto>>, HttpError> {
    Ok(Json(state.setup.recommend_model().map(Into::into)))
}
