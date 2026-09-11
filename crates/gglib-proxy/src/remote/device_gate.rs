//! A tunnelled request the edge did not name a device for reaches nothing.
//!
//! This is decision 2's second door, put back for tunnelled traffic. The
//! first door is the tunnel edge, which admits a request bearing a device's
//! own token; the proxy's `bearer_guard` used to be the second, and
//! `ServeOptions::backend_auth` took that away — the edge now replaces the
//! client's `Authorization` with the backend's own bearer on every admitted
//! request, so `bearer_guard` validates a header modelpipe wrote microseconds
//! earlier and can no longer refuse anything that crossed the tunnel.
//!
//! That matters because of what else `backend_auth` applies to. modelpipe
//! rewrites the header for *every* admitted request, the one a **pairing
//! grant** admits included, and a grant is one request at any path the
//! guesser likes — the edge cannot scope it. So without this gate, one
//! correctly guessed six-digit code would buy a single fully authenticated
//! request to any protected route, `POST /v1/proxy/shutdown` among them,
//! which is irreversible without physical access to the machine.
//!
//! The discriminator is the device header, and it works only because the
//! listener runs `TokenPolicy::Named`: the edge writes that header when a
//! *named* token admitted and never for a grant, so under `Named` every
//! legitimate tunnelled request carries one and the absence of it means a
//! grant — or a local process forging the markers, which is the other thing
//! this refuses.
//!
//! Restrictive only, exactly like [`mcp_tunnel_guard`](super::mcp_tunnel_guard):
//! nothing is ever *granted* on the strength of a header a client could
//! write. Forging one buys a refusal, not an admission.

use axum::{
    Json,
    extract::Request,
    http::StatusCode,
    middleware::Next,
    response::{IntoResponse, Response},
};
use tracing::warn;

use super::Tunnelled;
use crate::models::ErrorResponse;

/// Refuse a tunnelled request that no device token admitted.
///
/// Applied with `route_layer` on the protected group *before* the bearer
/// guard, so it runs inside it: a request that fails the bearer never reaches
/// here, and one that passes it still has to name a device.
///
/// Local requests carry no [`Tunnelled`] extension at all and pass straight
/// through — this says nothing about them, and the loopback proxy's own
/// clients are unaffected.
pub(crate) async fn device_gate(req: Request, next: Next) -> Response {
    let Some(tunnelled) = req.extensions().get::<Tunnelled>() else {
        return next.run(req).await;
    };
    if tunnelled.device.is_some() {
        return next.run(req).await;
    }
    warn!(
        peer = tunnelled.peer.as_deref().unwrap_or("?"),
        path = %req.uri().path(),
        "refused a tunnelled request that no device token admitted"
    );
    (
        StatusCode::FORBIDDEN,
        Json(ErrorResponse::with_code(
            "This request did not arrive on a device key. Pair this device by \
             running `gglib remote invite` on the serving machine and \
             redeeming the code it prints.",
            "invalid_request_error",
            "device_not_paired",
        )),
    )
        .into_response()
}
