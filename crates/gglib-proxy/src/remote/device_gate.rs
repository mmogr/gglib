//! A tunnelled request the edge did not name a device for reaches nothing.
//!
//! This is decision 2's second door for tunnelled traffic. The first door is
//! the tunnel edge, which admits a request bearing a device's own token. The
//! proxy's `bearer_guard` cannot be the second: under
//! `ServeOptions::backend_auth` the edge replaces the client's
//! `Authorization` with the backend's own bearer on every admitted request, so
//! `bearer_guard` validates a header modelpipe wrote microseconds earlier and
//! cannot refuse anything that crossed the tunnel.
//!
//! modelpipe answers pairing at the edge and forwards nothing for it, so
//! every request it admits under `TokenPolicy::Named` names a device. This
//! gate is the one check that could refuse a credential an edge admits
//! without a name — a one-time pairing grant forwarded at any path, say,
//! which would let a guessed code reach `POST /v1/proxy/shutdown`.
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
