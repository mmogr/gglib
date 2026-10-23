//! `/mcp` is not reachable through the tunnel.

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

/// Refuse every tunnelled request to `/mcp`.
///
/// Applied with `route_layer` on `/mcp` alone, inside the bearer guard, so an
/// unauthenticated request is still a 401 and only an authenticated,
/// tunnelled one reaches this refusal. No grant opens it: `invoke_tool`
/// starts the MCP servers configured on this machine, every one of them, and
/// a paired device reaches the servers `gglib remote enable --allow-mcp`
/// names through its runs instead. A marker on a proxy that has no tunnel is
/// a local client forging one, and forging it buys exactly this refusal.
pub(crate) async fn mcp_tunnel_guard(req: Request, next: Next) -> Response {
    let Some(tunnelled) = req.extensions().get::<Tunnelled>() else {
        return next.run(req).await;
    };
    warn!(
        peer = tunnelled.peer.as_deref().unwrap_or("?"),
        "refused a tunnelled request to /mcp, which the tunnel never reaches"
    );
    (
        StatusCode::FORBIDDEN,
        Json(ErrorResponse::with_code(
            "The MCP gateway is not reachable through the remote tunnel. A paired device's \
             chats reach the MCP servers `gglib remote enable --allow-mcp <server>` names, \
             through their runs.",
            "invalid_request_error",
            "mcp_not_allowed_over_tunnel",
        )),
    )
        .into_response()
}
