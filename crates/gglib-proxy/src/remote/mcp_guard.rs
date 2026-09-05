//! `/mcp` is not reachable through the tunnel unless the owner said so.

use axum::{
    Json,
    extract::{Request, State},
    http::StatusCode,
    middleware::Next,
    response::{IntoResponse, Response},
};
use tracing::warn;

use super::Tunnelled;
use crate::models::ErrorResponse;
use crate::server::AppState;

/// Refuse a tunnelled request to `/mcp` unless the tunnel's owner allows it.
///
/// Applied with `route_layer` on `/mcp` alone, inside the bearer guard, so an
/// unauthenticated request is still a 401 and only an authenticated,
/// tunnelled one reaches this decision. With no tunnel owner attached the
/// answer is also no: a marker on a proxy that has no tunnel is a local
/// client forging one, and forging it buys exactly this refusal.
pub(crate) async fn mcp_tunnel_guard(
    State(state): State<AppState>,
    req: Request,
    next: Next,
) -> Response {
    let Some(tunnelled) = req.extensions().get::<Tunnelled>() else {
        return next.run(req).await;
    };
    let allowed = state
        .remote_gateway()
        .is_some_and(|gateway| gateway.mcp_allowed());
    if allowed {
        return next.run(req).await;
    }
    warn!(
        peer = tunnelled.peer.as_deref().unwrap_or("?"),
        "refused a tunnelled request to /mcp; run `gglib remote enable --allow-mcp` to allow it"
    );
    (
        StatusCode::FORBIDDEN,
        Json(ErrorResponse::with_code(
            "The MCP gateway is not reachable through the remote tunnel. Enable it on the \
             serving machine with `gglib remote enable --allow-mcp` if you mean to.",
            "invalid_request_error",
            "mcp_not_allowed_over_tunnel",
        )),
    )
        .into_response()
}
