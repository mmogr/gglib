//! The image runtime, stable-diffusion.cpp's `sd-server`, over HTTP: the web
//! face of `gglib config sd install|status|uninstall`.
//!
//! The install streams the very events llama.cpp's does, under the same SSE
//! event names (`setup::install_event_to_sse`), so one reader draws both.

use std::convert::Infallible;
use std::future::Future;
use std::sync::atomic::{AtomicBool, Ordering};

use axum::Json;
use axum::extract::State;
use axum::response::sse::{Event, KeepAlive, Sse};
use futures_util::StreamExt;
use futures_util::stream::Stream;
use tokio::sync::mpsc;

use gglib_app_services::GuiError;
use gglib_app_services::ImageRuntimeStatus;
use gglib_runtime::llama::{LlamaProgressEvent, UninstallOutcome};

use super::setup::install_event_to_sse;
use crate::error::HttpError;
use crate::state::AppState;

/// One image runtime install or removal at a time, process-wide: two
/// installs would unpack into one download directory, and a removal during
/// an install would delete what it is writing.
static INSTALL_IN_FLIGHT: AtomicBool = AtomicBool::new(false);

/// Frees a taken install slot when dropped, however the work holding it ends.
struct Release(&'static AtomicBool);

impl Drop for Release {
    fn drop(&mut self) {
        self.0.store(false, Ordering::SeqCst);
    }
}

/// Take `slot`, or `None` when an install or a removal holds it already.
///
/// The release is built only once the slot is taken: a `Release` built and
/// dropped on a refusal would free the slot its holder still holds.
fn take(slot: &'static AtomicBool) -> Option<Release> {
    let taken = slot
        .compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst)
        .is_ok();
    taken.then(|| Release(slot))
}

/// Install the pinned pre-built `sd-server`, streaming progress over SSE.
pub(crate) async fn install_sd(
    State(state): State<AppState>,
) -> Sse<impl Stream<Item = Result<Event, Infallible>> + Send + 'static> {
    let setup = state.setup.clone();
    install_stream(&INSTALL_IN_FLIGHT, move |tx| async move {
        setup.install_sd(tx).await
    })
}

/// Run `install` with a channel whose events are the stream, unless `slot`
/// says another install or a removal is running. A refusal, and a failure,
/// are `failed` events: by then the response is already a stream.
pub(super) fn install_stream<F, Fut>(
    slot: &'static AtomicBool,
    install: F,
) -> Sse<impl Stream<Item = Result<Event, Infallible>> + Send + 'static>
where
    F: FnOnce(mpsc::Sender<LlamaProgressEvent>) -> Fut + Send + 'static,
    Fut: Future<Output = Result<(), GuiError>> + Send + 'static,
{
    let (tx, rx) = mpsc::channel::<LlamaProgressEvent>(64);

    if let Some(release) = take(slot) {
        tokio::spawn(async move {
            let _release = release;
            if let Err(e) = install(tx.clone()).await {
                let _ = tx
                    .send(LlamaProgressEvent::Failed {
                        message: e.to_string(),
                    })
                    .await;
            }
        });
    } else {
        let _ = tx.try_send(LlamaProgressEvent::Failed {
            message: "An image runtime install or removal is already running.".to_owned(),
        });
    }

    let stream = tokio_stream::wrappers::ReceiverStream::new(rx).map(install_event_to_sse);
    Sse::new(stream).keep_alive(
        KeepAlive::new()
            .interval(std::time::Duration::from_secs(30))
            .text("ping"),
    )
}

/// What is installed, what an install would download (and what to say about
/// it), and the image model running on it, if any.
pub(crate) async fn sd_status(
    State(state): State<AppState>,
) -> Result<Json<ImageRuntimeStatus>, HttpError> {
    Ok(Json(state.setup.sd_status(state.runtime.as_ref()).await?))
}

/// Remove `.sd/` whole. 409 while an image model runs on it, or while an
/// install or another removal holds the install slot; the removal holds it
/// until it ends, so no install starts under it.
pub(crate) async fn uninstall_sd(
    State(state): State<AppState>,
) -> Result<Json<UninstallOutcome>, HttpError> {
    let removal = state.setup.uninstall_sd(state.runtime.as_ref());
    Ok(Json(
        holding_install_slot(&INSTALL_IN_FLIGHT, removal).await?,
    ))
}

/// Run `removal` holding `slot`, or answer 409 when an install or another
/// removal holds it.
pub(super) async fn holding_install_slot<T>(
    slot: &'static AtomicBool,
    removal: impl Future<Output = Result<T, GuiError>>,
) -> Result<T, HttpError> {
    let Some(_release) = take(slot) else {
        return Err(HttpError::Conflict(
            "An image runtime install or removal is running. Wait for it to finish.".to_owned(),
        ));
    };
    Ok(removal.await?)
}

#[cfg(test)]
#[path = "image_runtime_tests.rs"]
mod tests;
