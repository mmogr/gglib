#![doc = include_str!("README.md")]

mod wire;

pub(crate) use wire::{RemoteEnableBody, RemoteEnableResponse, RemoteStatus};

use axum::{Json, extract::State};

use crate::{error::HttpError, state::AppState};

/// `POST /api/remote/enable` — bring the tunnel up and arm a pairing.
///
/// The one response that carries the ticket and the code. It is not
/// idempotent on purpose: a second call while enabled is a `409`, because
/// answering it would mean either minting a second code for a live session
/// or re-reading the first, and neither is what a person who typed `enable`
/// twice expects.
pub(crate) async fn enable(
    State(state): State<AppState>,
    Json(body): Json<Option<RemoteEnableBody>>,
) -> Result<Json<RemoteEnableResponse>, HttpError> {
    let request = body.unwrap_or_default().into_request();
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
    Ok(Json(RemoteStatus::from(state.remote.status().await)))
}

/// `GET /api/remote/status` — the tunnel as the surfaces render it.
pub(crate) async fn status(State(state): State<AppState>) -> Json<RemoteStatus> {
    Json(RemoteStatus::from(state.remote.status().await))
}
