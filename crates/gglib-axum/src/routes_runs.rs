//! The daemon's runs, nested under `/api/runs`: replies it owns until they
//! end, started and read from this machine.

use axum::Router;
use axum::extract::DefaultBodyLimit;
use axum::routing::{get, post};
use gglib_core::contracts::http::MAX_BODY_BYTES;

use crate::handlers;
use crate::state::AppState;

/// Every `/api/runs/*` route. Nested by the caller, so the paths here are
/// relative. Mirrored by `gglib_core::contracts::http::daemon::RUNS_PATH`
/// and its path builders, which `tests/daemon_route_contract.rs` walks.
pub(crate) fn run_routes() -> Router<AppState> {
    Router::new()
        .route("/", get(handlers::runs::list))
        // A chat run's body is the whole request, images inline, so it
        // gets what the proxy's own run door takes. An agent run's body
        // comes through the same route and gets the same.
        .route(
            "/{id}",
            get(handlers::runs::get)
                .put(handlers::runs::put)
                .layer(DefaultBodyLimit::max(MAX_BODY_BYTES)),
        )
        .route("/{id}/events", get(handlers::runs::events))
        .route("/{id}/cancel", post(handlers::runs::cancel))
}
