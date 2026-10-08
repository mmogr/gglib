//! SSE events handler - real-time event streaming.
//!
//! Streams application events (downloads, servers, etc.) to connected clients,
//! and takes the event a `gglib` command posts for a change it made itself.

use std::convert::Infallible;

use axum::Json;
use axum::extract::State;
use axum::http::StatusCode;
use axum::response::sse::{Event, Sse};
use futures_util::stream::Stream;
use gglib_core::events::AppEvent;
use gglib_core::ports::AppEventEmitter as _;

use crate::state::AppState;

/// SSE events stream endpoint.
///
/// Clients connect to this endpoint to receive real-time updates about:
/// - Download progress and completion
/// - Server start/stop events
/// - Model lifecycle, verification, and proxy events
///
/// The stream ends when the daemon's shutdown token fires. Without that, an open
/// `/api/events` connection would hold `axum::serve().with_graceful_shutdown()`
/// open forever — it waits for in-flight connections to drain, and an SSE stream
/// never drains on its own. gglib-proxy already does this for its own dashboard
/// stream; this is the daemon catching up.
///
/// `daemon_shutdown` is `None` when the router is built outside the daemon (the
/// integration-test harness does exactly that), in which case the stream is
/// unbounded — correct, because there is no graceful shutdown to block.
pub(crate) async fn stream(
    State(state): State<AppState>,
) -> Sse<impl Stream<Item = Result<Event, Infallible>> + Send + 'static> {
    let shutdown = state.daemon_shutdown.clone();
    let broadcaster = state.sse.clone();
    broadcaster.subscribe_until(async move {
        match shutdown {
            Some(token) => token.cancelled_owned().await,
            None => std::future::pending::<()>().await,
        }
    })
}

/// Send on the stream an event that was emitted in another process.
///
/// A `gglib model …` command that changes the library through `ModelOps`
/// runs the one this daemon's routes run, in a process of its own that no
/// client is attached to. It posts here what its `ModelOps` emitted, so a
/// client of this daemon is told of the change as it is told of one made
/// through the daemon: the same event, sent the same way.
///
/// The event is sent as it arrived, whichever event it is. Nothing here
/// reads the library, so the caller is taken at its word, as any caller the
/// bearer guard lets through is.
pub(crate) async fn relay(
    State(state): State<AppState>,
    Json(event): Json<AppEvent>,
) -> StatusCode {
    state.sse.emit(event);
    StatusCode::NO_CONTENT
}
