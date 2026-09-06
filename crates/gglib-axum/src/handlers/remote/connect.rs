//! The connect side's handlers: this machine reaching another.

use axum::{Json, extract::State};
use gglib_app_services::GuiError;

use super::wire::{RemoteConnectBody, RemoteConnectResponse, RemoteKillBody, RemoteStatus};
use crate::{error::HttpError, state::AppState};

/// `POST /api/remote/connect` — bind a loopback port here that is the far
/// machine's proxy, redeeming a pairing code for its key when one is given.
///
/// Not idempotent, like `enable` and for the same reason turned around: a
/// second `connect` while connected is a `409` rather than a silent reuse,
/// because the second call may name a different machine.
pub(crate) async fn connect(
    State(state): State<AppState>,
    Json(body): Json<Option<RemoteConnectBody>>,
) -> Result<Json<RemoteConnectResponse>, HttpError> {
    let request = body.unwrap_or_default().into_request();
    let connected = state.remote.connect(request).await?;
    Ok(Json(RemoteConnectResponse::from(connected)))
}

/// `POST /api/remote/disconnect` — close the loopback port. Idempotent: not
/// connected is the outcome asked for.
pub(crate) async fn disconnect(
    State(state): State<AppState>,
) -> Result<Json<RemoteStatus>, HttpError> {
    match state.remote.disconnect().await {
        Ok(()) | Err(GuiError::Conflict(_)) => {}
        Err(e) => return Err(e.into()),
    }
    Ok(Json(RemoteStatus::from(state.remote.status().await)))
}

/// `POST /api/remote/kill` — stop the far daemon through the tunnel, then
/// disconnect. The body must carry `{"confirm":"shutdown"}`; anything else is
/// a `400` that changes nothing, because the far side cannot be restarted from
/// here.
pub(crate) async fn kill(
    State(state): State<AppState>,
    Json(body): Json<Option<RemoteKillBody>>,
) -> Result<Json<RemoteStatus>, HttpError> {
    let confirmed = body
        .and_then(|b| b.confirm)
        .is_some_and(|word| word == "shutdown");
    if !confirmed {
        return Err(GuiError::ValidationFailed(
            "stopping the remote is not reversible from here — send {\"confirm\":\"shutdown\"} \
             to mean it"
                .to_owned(),
        )
        .into());
    }
    state.remote.kill_remote().await?;
    Ok(Json(RemoteStatus::from(state.remote.status().await)))
}
