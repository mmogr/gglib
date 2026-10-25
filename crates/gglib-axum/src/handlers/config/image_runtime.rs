//! The image runtime, stable-diffusion.cpp's `sd-server`, over HTTP: the web
//! face of `gglib config sd install|status|uninstall`.
//!
//! The install and the removal hold one slot, as llama.cpp's do theirs
//! (`install_slot`), and stream the very events llama.cpp's install does,
//! under the same SSE event names, so one reader draws both.

use std::convert::Infallible;
use std::sync::atomic::AtomicBool;

use axum::Json;
use axum::extract::State;
use axum::response::sse::{Event, Sse};
use futures_util::stream::Stream;

use gglib_app_services::ImageRuntimeStatus;
use gglib_core::domain::RuntimeKind;
use gglib_runtime::engine::UninstallOutcome;

use super::install_slot::{holding_install_slot, install_stream};
use crate::error::HttpError;
use crate::state::AppState;

/// The product the slot's refusals name.
const PRODUCT: &str = RuntimeKind::StableDiffusion.label();

/// One image runtime install or removal at a time, process-wide: two
/// installs would unpack into one download directory, and a removal during
/// an install would delete what it is writing.
static INSTALL_IN_FLIGHT: AtomicBool = AtomicBool::new(false);

/// Install the pinned pre-built `sd-server`, streaming progress over SSE.
/// Refused while an image model runs on it.
pub(crate) async fn install_sd(
    State(state): State<AppState>,
) -> Sse<impl Stream<Item = Result<Event, Infallible>> + Send + 'static> {
    let (setup, runtime) = (state.setup.clone(), state.runtime.clone());
    install_stream(&INSTALL_IN_FLIGHT, PRODUCT, move |tx| async move {
        setup.install_sd(runtime.as_ref(), tx).await
    })
}

/// What is installed, what an install would download (and what to say about
/// it), and the image model running on it, if any.
pub(crate) async fn sd_status(
    State(state): State<AppState>,
) -> Result<Json<ImageRuntimeStatus>, HttpError> {
    Ok(Json(state.setup.sd_status(state.runtime.as_ref()).await?))
}

/// Remove what the image runtime's install put in `.sd/`. 409 while an image model runs on it, or while an
/// install or another removal holds the install slot; the removal holds it
/// until it ends, so no install starts under it.
pub(crate) async fn uninstall_sd(
    State(state): State<AppState>,
) -> Result<Json<UninstallOutcome>, HttpError> {
    let removal = state
        .setup
        .uninstall(state.runtime.as_ref(), RuntimeKind::StableDiffusion);
    Ok(Json(
        holding_install_slot(&INSTALL_IN_FLIGHT, PRODUCT, removal).await?,
    ))
}
