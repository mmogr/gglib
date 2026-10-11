//! The remote tunnel's routes, nested under `/api/remote` (ADR 0012).
//!
//! Two sides, as the tunnel has: `enable`/`disable`/`status` and the device
//! routes are this machine serving, `join`/`disconnect`/`kill` and the far
//! machine's chats, runs and models are this machine reaching another one.

use axum::Router;
use axum::routing::{delete, get, post, put};

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
        .route("/join", post(handlers::remote::join))
        .route("/disconnect", post(handlers::remote::disconnect))
        .route("/kill", post(handlers::remote::kill))
        // The far machine's chats and runs, for this machine's chat page:
        // each forwarded through the tunnel with the stored key.
        .route("/chats", get(handlers::remote::list_chats))
        .route("/chats/{id}", get(handlers::remote::open_chat))
        .route(
            "/chats/{id}/turns/{run_id}",
            put(handlers::remote::add_turn),
        )
        // The images those chats' turns carry, stored on the far machine.
        .route(
            "/attachments",
            post(handlers::remote::upload_attachment).layer(handlers::attachments::body_limit()),
        )
        .route("/attachments/{id}", get(handlers::remote::fetch_attachment))
        .route("/runs", get(handlers::remote::list_runs))
        .route("/runs/{run_id}/events", get(handlers::remote::run_events))
        .route("/runs/{run_id}/cancel", post(handlers::remote::cancel_run))
        // Whether the far machine can draw, for a far chat's Draw button.
        .route("/images/drawing", get(handlers::remote::drawing))
        // The far machine's models: read through the tunnel with the stored
        // key, and loaded, never changed.
        .route("/models", get(handlers::remote::list_models))
        .route("/models/{model}", get(handlers::remote::model_detail))
        .route("/models/{model}/load", post(handlers::remote::load_model))
}
