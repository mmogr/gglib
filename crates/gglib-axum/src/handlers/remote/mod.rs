#![doc = include_str!("README.md")]

mod attachments;
mod chats;
mod devices;
#[cfg(test)]
pub(crate) mod fake_far;
mod join;
mod models;

pub(crate) use attachments::{fetch_attachment, upload_attachment};
pub(crate) use chats::{
    add_turn, cancel_run, drawing, list_chats, list_runs, open_chat, run_events,
};
pub(crate) use devices::{forget, invite, list};
pub(crate) use join::{disconnect, join, kill};
pub(crate) use models::{far_error, list_models, load_model, model_detail};

use axum::{Json, extract::State};
use gglib_app_services::{GuiError, RemoteEnableBody, RemoteEnableResponse, RemoteStatus};
use gglib_mcp::Unopenable;

use crate::{error::HttpError, state::AppState};

/// `POST /api/remote/enable` — bring the tunnel up and arm a pairing.
///
/// The one response that carries the ticket and the code. It is not
/// idempotent on purpose: a second call while enabled is a `409`, because
/// answering it would mean either minting a second code for a live session
/// or re-reading the first, and neither is what a person who typed `enable`
/// twice expects.
///
/// A call that arrives while the daemon's own startup resume is putting the
/// tunnel back is not that second call. It waits, for up to twenty seconds,
/// and is answered by the session the resume brought up, or arms its own if
/// the resume brought none (`RemoteOps::enable`).
///
/// The MCP servers it names are checked first, against this machine's: a
/// name that opens no one server is a `400`, servers that cannot be read a
/// `500`, and either way nothing is armed or stored.
/// A resume or a re-arm does not check again; a name gone since resolves to
/// nothing when a run starts.
pub(crate) async fn enable(
    State(state): State<AppState>,
    Json(body): Json<Option<RemoteEnableBody>>,
) -> Result<Json<RemoteEnableResponse>, HttpError> {
    let request = body.unwrap_or_default().into_request();
    state
        .mcp
        .refuse_unopenable(&request.mcp_servers)
        .await
        .map_err(|refused| match refused {
            Unopenable::Name(why) => GuiError::ValidationFailed(why),
            Unopenable::Unread(e) => {
                GuiError::Internal(format!("could not read the MCP servers: {e}"))
            }
        })?;
    let enabled = state.remote.enable(request).await?;
    Ok(Json(RemoteEnableResponse::from(enabled)))
}

/// `POST /api/remote/disable` — take the tunnel down; the ticket dies.
///
/// Idempotent: a tunnel that is already down is the outcome asked for, so
/// "not enabled" is answered with the status rather than a conflict.
pub(crate) async fn disable(
    State(state): State<AppState>,
) -> Result<Json<RemoteStatus>, HttpError> {
    match state.remote.disable().await {
        Ok(()) | Err(gglib_app_services::GuiError::Conflict(_)) => {}
        Err(e) => return Err(e.into()),
    }
    Ok(Json(state.remote.status().await))
}

/// `GET /api/remote/status` — the tunnel as the surfaces render it.
pub(crate) async fn status(State(state): State<AppState>) -> Json<RemoteStatus> {
    Json(state.remote.status().await)
}

#[cfg(test)]
#[path = "enable_tests.rs"]
mod enable_tests;
