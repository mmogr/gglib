//! The remote tunnel's routes, nested under `/api/remote` (ADR 0012).
//!
//! Two sides, as the tunnel has: `enable`/`disable`/`status` and the device
//! routes are this machine serving, `join`/`disconnect`/`kill` are this
//! machine reaching another one.
//!
//! The routes that change who is trusted answer only the daemon token;
//! `crate::trust` says why.

use axum::Router;
use axum::middleware;
use axum::routing::{delete, get, post};
use gglib_core::access::DaemonToken;

use crate::handlers;
use crate::state::AppState;
use crate::trust::trust_guard;

/// Every `/api/remote/*` route.
///
/// Nested under `/api/remote` by the caller, so the paths here are relative.
/// They are mirrored by `gglib_core::contracts::http::daemon::REMOTE_*_PATH`
/// and walked by `tests/daemon_route_contract.rs`; a path added here without
/// an entry there is silently unchecked.
///
/// `token` is what the routes that change who is trusted ask for, the ones
/// `gglib_core::contracts::http::daemon::TRUST_ROUTES` lists and `forget`.
/// `None` shuts them.
pub(crate) fn remote_routes(token: Option<DaemonToken>) -> Router<AppState> {
    let trusted = Router::new()
        // This machine as the desktop: the tunnel in front of its own proxy,
        // and who may use it. `/devices` and `/devices/{device}` are siblings,
        // as `/mcp/servers` and `/mcp/servers/{id}` already are.
        .route("/enable", post(handlers::remote::enable))
        .route("/invite", post(handlers::remote::invite))
        .route("/devices/{device}", delete(handlers::remote::forget))
        // This machine as the laptop: a loopback port here that is another
        // machine's proxy.
        .route("/join", post(handlers::remote::join))
        .route("/disconnect", post(handlers::remote::disconnect))
        .route("/kill", post(handlers::remote::kill))
        .route_layer(middleware::from_fn_with_state(token, trust_guard));
    Router::new()
        .route("/disable", post(handlers::remote::disable))
        .route("/status", get(handlers::remote::status))
        .route("/devices", get(handlers::remote::list))
        .merge(trusted)
}
