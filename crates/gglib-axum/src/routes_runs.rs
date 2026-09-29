//! The daemon's runs, nested under `/api/runs`: replies it owns until they
//! end, started and read from this machine.

use axum::Router;
use axum::extract::DefaultBodyLimit;
use axum::routing::{get, post};

use crate::handlers;
use crate::state::AppState;

/// Every `/api/runs/*` route. Nested by the caller, so the paths here are
/// relative. Mirrored by `gglib_core::contracts::http::daemon::RUNS_PATH`
/// and its path builders, which `tests/daemon_route_contract.rs` walks.
pub(crate) fn run_routes() -> Router<AppState> {
    Router::new()
        .route("/", get(handlers::runs::list))
        // A run's body is a whole conversation, so it gets the agent route's
        // 4 MiB rather than the default 2.
        .route(
            "/{id}",
            get(handlers::runs::get)
                .put(handlers::runs::put)
                .layer(DefaultBodyLimit::max(4 * 1024 * 1024)),
        )
        .route("/{id}/events", get(handlers::runs::events))
        .route("/{id}/cancel", post(handlers::runs::cancel))
}
