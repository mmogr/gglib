//! Setup handlers - first-run wizard system status and provisioning.

use std::convert::Infallible;
use std::sync::atomic::AtomicBool;

use axum::Json;
use axum::extract::State;
use axum::response::sse::{Event, Sse};
use futures_util::stream::Stream;
use serde::Serialize;

use super::install_slot::{holding_install_slot, install_stream};
use crate::dto::diagnostics::{DiagnosticsDto, RecommendationDto, ResolvedPathsDto};
use crate::error::HttpError;
use crate::state::AppState;
use gglib_app_services::setup::SetupStatus;
use gglib_core::domain::RuntimeKind;
use gglib_runtime::engine::UninstallOutcome;
use gglib_runtime::llama::{LlamaStatus, llama_status};

/// One llama.cpp install or removal at a time, process-wide, as the image
/// runtime has: two installs would unpack into one download directory, and
/// a removal during an install would delete what it is writing.
static LLAMA_IN_FLIGHT: AtomicBool = AtomicBool::new(false);

/// The product the slot's refusals name.
const PRODUCT: &str = RuntimeKind::Llama.label();

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

/// Install llama.cpp's pinned pre-built binaries with SSE progress
/// streaming. Refused while a chat model runs on llama-server, and while
/// another install or a removal holds the slot.
pub(crate) async fn install_llama(
    State(state): State<AppState>,
) -> Sse<impl Stream<Item = Result<Event, Infallible>> + Send + 'static> {
    let (setup, runtime) = (state.setup.clone(), state.runtime.clone());
    install_stream(&LLAMA_IN_FLIGHT, PRODUCT, move |tx| async move {
        setup.install_llama(runtime.as_ref(), tx).await
    })
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

/// What llama.cpp install is present, if any — the GUI face of
/// `gglib config llama status` — with the chat model running on it, under
/// which no update is offered. Local and cheap: no network.
pub(crate) async fn llama_status_handler(
    State(state): State<AppState>,
) -> Result<Json<LlamaStatus>, HttpError> {
    // Probing spawns `llama-server --version` on a cold cache — blocking work
    // that does not belong on an async worker.
    let mut status = tokio::task::spawn_blocking(llama_status)
        .await
        .map_err(|e| HttpError::Internal(format!("Status task panicked: {e}")))?
        .map_err(|e| HttpError::Internal(e.to_string()))?;
    status.serving = state
        .setup
        .serving(state.runtime.as_ref(), RuntimeKind::Llama)
        .await;
    Ok(Json(status))
}

/// Remove the llama.cpp installation — the GUI face of
/// `gglib config llama uninstall --force`. 409 while a chat model runs on
/// it, or while an install or another removal holds the slot.
///
/// The confirmation is the GUI's to run; by the time this is called the
/// decision is made.
pub(crate) async fn uninstall_llama_handler(
    State(state): State<AppState>,
) -> Result<Json<UninstallOutcome>, HttpError> {
    let removal = state
        .setup
        .uninstall(state.runtime.as_ref(), RuntimeKind::Llama);
    Ok(Json(
        holding_install_slot(&LLAMA_IN_FLIGHT, PRODUCT, removal).await?,
    ))
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
