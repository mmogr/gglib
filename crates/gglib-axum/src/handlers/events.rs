//! SSE events handler - real-time event streaming.
//!
//! Streams application events (downloads, servers, etc.) to connected clients.

use std::convert::Infallible;

use axum::extract::State;
use axum::response::sse::{Event, Sse};
use futures_util::stream::Stream;

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
