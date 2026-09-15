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
//! That mattered more when it was written. While gglib paired through a
//! one-time grant, modelpipe rewrote the header on the request a grant
//! admitted too, and a grant was one request at any path the guesser liked;
//! this gate was what kept a guessed code from reaching
//! `POST /v1/proxy/shutdown`. modelpipe 0.6 answers pairing at the edge and
//! forwards nothing for it, so every request it admits under
//! `TokenPolicy::Named` names a device. The gate stays as the one check left
//! that could refuse a credential a later edge admits without a name.
//!
//! The discriminator is the device header, which the edge writes when a
//! *named* token admitted, so under `Named` every legitimate tunnelled
//! request carries one, and its absence means a client that reached the
//! proxy directly forging the markers, which this refuses too.
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
