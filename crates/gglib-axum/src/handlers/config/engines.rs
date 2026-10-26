//! Either engine over HTTP, at `/api/config/system/engines/{kind}/…` with
//! `kind` `llama` or `stable_diffusion`: the web face of
//! `gglib config llama|sd install|status|uninstall`.
//!
//! Each engine's install and removal hold that engine's slot
//! (`install_slot`): two installs would unpack into one download directory,
//! and a removal during an install would delete what it is writing. Both
//! installs stream the same events under the same SSE names, so one reader
//! draws either.

use std::convert::Infallible;
use std::sync::atomic::AtomicBool;

use axum::Json;
use axum::extract::{Path, State};
use axum::response::sse::{Event, Sse};
use futures_util::stream::Stream;

use gglib_app_services::EngineStatus;
use gglib_core::domain::RuntimeKind;
use gglib_runtime::engine::UninstallOutcome;

use super::install_slot::{holding_install_slot, install_stream};
use crate::error::HttpError;
use crate::state::AppState;

/// One llama.cpp install or removal at a time, process-wide.
static LLAMA_IN_FLIGHT: AtomicBool = AtomicBool::new(false);

/// One image runtime install or removal at a time, process-wide.
static SD_IN_FLIGHT: AtomicBool = AtomicBool::new(false);

/// `kind`'s slot.
const fn slot(kind: RuntimeKind) -> &'static AtomicBool {
    match kind {
        RuntimeKind::Llama => &LLAMA_IN_FLIGHT,
        RuntimeKind::StableDiffusion => &SD_IN_FLIGHT,
    }
}

/// What is installed and how, where it stands against its pin, what an
/// install would download and what to say about it, and the model running
/// on it, if any. Local: no network.
pub(crate) async fn status(
    State(state): State<AppState>,
    Path(kind): Path<RuntimeKind>,
) -> Result<Json<EngineStatus>, HttpError> {
    Ok(Json(
        state
            .setup
            .engine_status(state.runtime.as_ref(), kind)
            .await?,
    ))
}

/// Install the pinned download, streaming progress over SSE. It is also the
/// update: an install over an older engine replaces it. Refused while a
/// model runs on that engine, while another install or a removal holds its
/// slot, and where the install would be a source build.
pub(crate) async fn install(
    State(state): State<AppState>,
    Path(kind): Path<RuntimeKind>,
) -> Sse<impl Stream<Item = Result<Event, Infallible>> + Send + 'static> {
    let (setup, runtime) = (state.setup.clone(), state.runtime.clone());
    install_stream(slot(kind), kind.label(), move |tx| async move {
        setup.install(runtime.as_ref(), kind, tx).await
    })
}

/// Remove what the install put in the engine's folder. 409 while a model
/// runs on it, or while an install or another removal holds its slot; the
/// removal holds the slot until it ends, so no install starts under it.
///
/// The confirmation is the app's to run; by the time this is called the
/// decision is made.
pub(crate) async fn uninstall(
    State(state): State<AppState>,
    Path(kind): Path<RuntimeKind>,
) -> Result<Json<UninstallOutcome>, HttpError> {
    let removal = state.setup.uninstall(state.runtime.as_ref(), kind);
    Ok(Json(
        holding_install_slot(slot(kind), kind.label(), removal).await?,
    ))
}

#[cfg(test)]
#[path = "engines_tests.rs"]
mod tests;
