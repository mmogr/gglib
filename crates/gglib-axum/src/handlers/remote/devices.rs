//! The three device routes: invite one, list them, retire one.
//!
//! Their DTOs live here beside them rather than in `wire.rs`, which is
//! fifteen lines from the 300-line budget `scripts/check_rust_complexity.sh`
//! allows a file not already in its baseline. The split is along a subject
//! either way: `wire.rs` is the tunnel — enable, status, connect — and this
//! is who may use it.
//!
//! `invite` has no DTO of its own. `RemoteOps::invite` answers with the same
//! `Enabled` an `enable --invite` does, so this route answers with
//! [`RemoteEnableResponse`] and a client needs no second shape to decode.

use axum::Json;
use axum::extract::{Path, State};
use gglib_app_services::DeviceView;

use super::wire::RemoteEnableResponse;
use crate::{error::HttpError, state::AppState};

/// One device this machine has issued a key to.
#[derive(Debug, Clone, serde::Serialize)]
#[cfg_attr(feature = "ts-bindings", derive(ts_rs::TS), ts(export))]
pub(crate) struct RemoteDevice {
    /// The name the tunnel edge holds this device's key under, and the value
    /// it sends back on every request. Not a secret.
    pub id: String,
    /// What the device called itself when it joined, if it said.
    #[cfg_attr(feature = "ts-bindings", ts(optional))]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub label: Option<String>,
    /// Unix milliseconds at which this device's invite was minted.
    #[cfg_attr(feature = "ts-bindings", ts(type = "number"))]
    pub joined_at: i64,
    /// Unix milliseconds at which a device redeemed the invite, or `null` if
    /// none ever has.
    ///
    /// A row with no `redeemed_at` **and** no `last_seen` is an invite nobody
    /// took, and a surface should say so rather than render it as a device.
    /// Both halves matter: this is written by a background task, so a device
    /// that has plainly made requests must not be called never-joined
    /// because the one advisory write was lost.
    #[cfg_attr(feature = "ts-bindings", ts(type = "number | null"))]
    pub redeemed_at: Option<i64>,
    /// Unix milliseconds of the last request that arrived under its key.
    /// Advisory, and written at most once a minute per device.
    #[cfg_attr(feature = "ts-bindings", ts(type = "number | null"))]
    pub last_seen: Option<i64>,
    /// Whether the edge is admitting it right now, or `null` when the tunnel
    /// is down — nothing admits then, and `false` would read as "this one
    /// device was dropped".
    #[cfg_attr(feature = "ts-bindings", ts(type = "boolean | null"))]
    pub admitted: Option<bool>,
}

impl From<DeviceView> for RemoteDevice {
    fn from(d: DeviceView) -> Self {
        Self {
            id: d.id,
            label: d.label,
            joined_at: d.joined_at,
            redeemed_at: d.redeemed_at,
            last_seen: d.last_seen,
            admitted: d.admitted,
        }
    }
}

/// What `DELETE /api/remote/devices/{device}` did.
#[derive(Debug, Clone, serde::Serialize)]
#[cfg_attr(feature = "ts-bindings", derive(ts_rs::TS), ts(export))]
pub(crate) struct RemoteForgotten {
    /// Whether this machine held anything under that name.
    ///
    /// `false` is a `200`, not a `404`: retiring a device that is already
    /// gone is the outcome asked for. A surface that wants to say "no such
    /// device" has this to say it with; one that just wants the device gone
    /// can ignore it.
    pub forgotten: bool,
}

/// `POST /api/remote/invite` — mint a key for one new device, and a code.
///
/// `409` when the tunnel is not up or an invite is already open. Answers
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
    let devices = state.remote.list().await?;
    Ok(Json(devices.into_iter().map(RemoteDevice::from).collect()))
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
