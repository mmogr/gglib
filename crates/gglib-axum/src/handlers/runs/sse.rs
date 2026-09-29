//! A run's events as server-sent events.
//!
//! Each event is `id: <seq>` and `data: <frame>`; the run's end is one
//! `event: run` whose data is the run's final `RunInfo`, and the stream
//! closes after it. It also closes when the daemon's shutdown token fires,
//! since an open SSE response would otherwise hold the graceful shutdown
//! open, as `/api/events` explains.

use std::convert::Infallible;

use axum::response::sse::{Event, KeepAlive, Sse};
use futures_util::StreamExt as _;
use futures_util::stream::Stream;
use gglib_core::ports::{RunEvent, RunEvents};
use tokio_util::sync::CancellationToken;

/// One run event as an SSE event.
fn encode(event: RunEvent) -> Event {
    match event {
        RunEvent::Frame { seq, data } => Event::default().id(seq.to_string()).data(&*data),
        RunEvent::End(info) => Event::default()
            .event("run")
            .data(serde_json::to_string(&info).unwrap_or_default()),
    }
}

/// The SSE response for `events`, ending early on `shutdown`.
pub(super) fn stream(
    events: RunEvents,
    shutdown: Option<CancellationToken>,
) -> Sse<impl Stream<Item = Result<Event, Infallible>> + Send + 'static> {
    let until = async move {
        match shutdown {
            Some(token) => token.cancelled_owned().await,
            None => std::future::pending().await,
        }
    };
    let events = events
        .map(|event| Ok::<_, Infallible>(encode(event)))
        .take_until(until);
    Sse::new(events).keep_alive(KeepAlive::default())
}

#[cfg(test)]
#[path = "sse_tests.rs"]
mod sse_tests;
