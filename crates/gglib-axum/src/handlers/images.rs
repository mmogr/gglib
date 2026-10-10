//! `POST /api/images/generations`: the proxy's `POST /v1/images/generations`
//! at the daemon's door, the same handler body over the same image driver.
//!
//! Here because the proxy may be stopped or demand a key the CLI does not
//! hold; `gglib image` calls this route, behind the daemon's own guards.

use std::sync::Arc;

use axum::body::Bytes;
use axum::extract::State;
use axum::response::Response;

use crate::state::AppState;

/// Draw, answering as the proxy's route does.
pub(crate) async fn generations(State(state): State<AppState>, body: Bytes) -> Response {
    gglib_proxy::images::generations(Some(Arc::clone(&state.images)), body).await
}
