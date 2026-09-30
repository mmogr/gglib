//! Only a device the tunnel edge named reaches the hub's chats.

use axum::Json;
use axum::extract::Request;
use axum::http::StatusCode;
use axum::middleware::Next;
use axum::response::{IntoResponse, Response};

use crate::models::ErrorResponse;
use crate::remote::Tunnelled;

/// Refuse any request that is not tunnelled from a named device.
///
/// Applied with `route_layer` on `/v1/chats` alone, inside the bearer guard
/// and the device gate, so an unauthenticated request is still a 401. A
/// local request is refused as well: this machine reads its chats at `/api`,
/// and a client holding the proxy's key on a LAN bind must not read every
/// chat. The marker can be forged by a client that reaches the proxy
/// directly (see `runs::scope`); forging it buys what the named device may
/// read, no more.
pub(crate) async fn named_device_only(req: Request, next: Next) -> Response {
    let named = req
        .extensions()
        .get::<Tunnelled>()
        .is_some_and(|t| t.device.is_some());
    if named {
        return next.run(req).await;
    }
    (
        StatusCode::FORBIDDEN,
        Json(ErrorResponse::with_code(
            "The hub's chats are read by a paired device, through the tunnel.",
            "invalid_request_error",
            "device_not_named",
        )),
    )
        .into_response()
}
