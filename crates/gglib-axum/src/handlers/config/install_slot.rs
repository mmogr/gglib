//! One install or removal of an engine at a time, and the stream an install
//! is read through, for llama.cpp and stable-diffusion.cpp alike.
//!
//! Each engine has its own slot. Two installs of one engine would unpack
//! into one download directory, and a removal during an install would
//! delete what it is writing. An install streams [`LlamaProgressEvent`]s
//! under one SSE event name per variant, the payload its JSON, so one reader
//! draws either engine's.

use std::convert::Infallible;
use std::future::Future;
use std::sync::atomic::{AtomicBool, Ordering};

use axum::response::sse::{Event, KeepAlive, Sse};
use futures_util::StreamExt;
use futures_util::stream::Stream;
use tokio::sync::mpsc;

use gglib_app_services::GuiError;
use gglib_runtime::llama::LlamaProgressEvent;

use crate::error::HttpError;

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

/// Run `install` of `product` with a channel whose events are the stream,
/// unless `slot` says another install or a removal is running. A refusal,
/// and a failure, are `failed` events: by then the response is already a
/// stream.
pub(super) fn install_stream<F, Fut>(
    slot: &'static AtomicBool,
    product: &'static str,
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
            // The pipeline reports failure by returning, leaving the wording
            // to whoever owns the channel. This is that owner.
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
            message: format!("A {product} install or removal is already running."),
        });
    }

    let stream = tokio_stream::wrappers::ReceiverStream::new(rx).map(|event| Ok(sse_frame(&event)));
    Sse::new(stream).keep_alive(
        KeepAlive::new()
            .interval(std::time::Duration::from_secs(30))
            .text("ping"),
    )
}

/// Run `removal` of `product` holding `slot`, or answer 409 when an install
/// or another removal holds it. The removal holds it until it ends, so no
/// install starts under it.
pub(super) async fn holding_install_slot<T>(
    slot: &'static AtomicBool,
    product: &str,
    removal: impl Future<Output = Result<T, GuiError>>,
) -> Result<T, HttpError> {
    let Some(_release) = take(slot) else {
        return Err(HttpError::Conflict(format!(
            "A {product} install or removal is running. Wait for it to finish."
        )));
    };
    Ok(removal.await?)
}

/// One install event as an SSE frame: the variant's name, the event's JSON.
fn sse_frame(event: &LlamaProgressEvent) -> Event {
    let event_type = match event {
        LlamaProgressEvent::PhaseStarted { .. } => "phase_started",
        LlamaProgressEvent::Progress { .. } => "progress",
        LlamaProgressEvent::PhaseCompleted { .. } => "phase_completed",
        LlamaProgressEvent::Completed { .. } => "completed",
        LlamaProgressEvent::Failed { .. } => "failed",
    };
    let data = serde_json::to_string(event).unwrap_or_default();
    Event::default().event(event_type).data(data)
}

#[cfg(test)]
#[path = "install_slot_tests.rs"]
mod tests;
