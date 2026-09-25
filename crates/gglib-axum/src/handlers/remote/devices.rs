//! The three device routes: invite one, list them, retire one.
//!
//! `invite` has no shape of its own. `RemoteOps::invite` answers with the
//! same `Enabled` an `enable --invite` does, so this route answers with
//! [`RemoteEnableResponse`] and a client needs no second shape to decode.

use axum::Json;
use axum::extract::{Path, State};
use gglib_app_services::{RemoteDevice, RemoteEnableResponse, RemoteForgotten};

use crate::{error::HttpError, state::AppState};

/// `POST /api/remote/invite` — mint a key for one new device, and a code.
///
/// `409` when the tunnel is not up, once a startup resume that is putting it
/// back has been waited for, or when an invite is already open. Answers
/// with the enable response because that is the shape `RemoteOps::invite`
/// returns: the ticket belongs in it, and a pairing view needs one.
pub(crate) async fn invite(
    State(state): State<AppState>,
) -> Result<Json<RemoteEnableResponse>, HttpError> {
    let enabled = state.remote.invite().await?;
    Ok(Json(RemoteEnableResponse::from(enabled)))
}

/// `GET /api/remote/devices` — the roster, newest invite last.
///
/// Answers `200` with an empty list on a machine that has never paired
/// anything, and works with the tunnel down: the roster is settings, and a
/// person deciding what to retire is often doing it precisely because the
/// tunnel is off.
pub(crate) async fn list(
    State(state): State<AppState>,
) -> Result<Json<Vec<RemoteDevice>>, HttpError> {
    Ok(Json(state.remote.list().await?))
}

/// `DELETE /api/remote/devices/{device}` — stop admitting one device.
///
/// Works with the tunnel down, and must: a laptop is lost at a moment
/// nobody chose, and a retirement that needed the tunnel up would be one
/// more thing to do first.
pub(crate) async fn forget(
    State(state): State<AppState>,
    Path(device): Path<String>,
) -> Result<Json<RemoteForgotten>, HttpError> {
    let forgotten = state.remote.forget(&device).await?;
    Ok(Json(RemoteForgotten { forgotten }))
}
