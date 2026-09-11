//! The remote tunnel's routes, nested under `/api/remote` (ADR 0012).
//!
//! A module of its own rather than more of `routes.rs`, which is at the size
//! `scripts/check_rust_complexity.sh` recorded for it and may not grow. The
//! `model_routes`/`config_routes` precedent extracts *within* that file,
//! which does not help; a sibling does, and leaves `api_routes` one `.nest`
//! line where six routes were.
//!
//! Two sides, as the tunnel has: `enable`/`disable`/`status` and the device
//! routes are this machine serving, `connect`/`disconnect`/`kill` are this
//! machine reaching another one.

use axum::Router;
use axum::routing::{delete, get, post};

use crate::handlers;
use crate::state::AppState;

/// Every `/api/remote/*` route.
///
/// Nested under `/api/remote` by the caller, so the paths here are relative.
/// They are mirrored by `gglib_core::contracts::http::daemon::REMOTE_*_PATH`
/// and walked by `tests/daemon_route_contract.rs`; a path added here without
/// an entry there is silently unchecked.
pub(crate) fn remote_routes() -> Router<AppState> {
    Router::new()
        // This machine as the desktop: the tunnel in front of its own proxy.
        .route("/enable", post(handlers::remote::enable))
        .route("/disable", post(handlers::remote::disable))
        .route("/status", get(handlers::remote::status))
        // Who may use it. `/devices` and `/devices/{device}` are siblings, as
        // `/mcp/servers` and `/mcp/servers/{id}` already are.
        .route("/invite", post(handlers::remote::invite))
        .route("/devices", get(handlers::remote::list))
        .route("/devices/{device}", delete(handlers::remote::forget))
        // This machine as the laptop: a loopback port here that is another
        // machine's proxy.
        .route("/connect", post(handlers::remote::connect))
        .route("/disconnect", post(handlers::remote::disconnect))
        .route("/kill", post(handlers::remote::kill))
}
