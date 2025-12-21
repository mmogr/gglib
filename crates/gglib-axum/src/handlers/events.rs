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
/// - MCP server events
pub async fn stream(
    State(state): State<AppState>,
) -> Sse<impl Stream<Item = Result<Event, Infallible>> + Send + 'static> {
    state.sse.clone().subscribe()
}
