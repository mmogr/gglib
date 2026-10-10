//! `POST /api/images/generations`: the proxy's `POST /v1/images/generations`
//! at the daemon's door, the same handler body over the same image driver.
//!
//! Here because the proxy may be stopped or demand a key the CLI does not
//! hold; `gglib image` calls this route, behind the daemon's own guards.
//!
//! `GET /api/images/drawing`: whether a message sent with Draw pressed can
//! draw here, and why not, for the page's Draw button and `/draw`. Its own
//! route rather than a field of `/api/builtin/tools`, whose list the page
//! lets a person switch tools on from: drawing is offered by the server, for
//! a message sent with `draw: true`, never by a tool picker.

use std::sync::Arc;

use axum::Json;
use axum::body::Bytes;
use axum::extract::{Query, State};
use axum::response::Response;
use gglib_core::contracts::http::images::{DrawingAvailability, DrawingQuery};
use gglib_core::services::drawing_availability;

use crate::state::AppState;

/// Draw, answering as the proxy's route does.
pub(crate) async fn generations(State(state): State<AppState>, body: Bytes) -> Response {
    gglib_proxy::images::generations(Some(Arc::clone(&state.images)), body).await
}

/// Whether drawing is available, for the chat's model `far` and whether it
/// `calls_tools`. Always 200: `code`, when it is not, is the code a request
/// to draw would be refused with, not an error of this one.
pub(crate) async fn drawing(
    State(state): State<AppState>,
    Query(query): Query<DrawingQuery>,
) -> Json<DrawingAvailability> {
    Json(drawing_availability(Some(state.images.as_ref()), query.far, query.calls_tools).await)
}
