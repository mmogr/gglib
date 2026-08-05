//! Daemon lifecycle handlers.
//!
//! One route: `POST /api/daemon/shutdown`, the API-side twin of SIGTERM.
//! It exists so clients that cannot signal the daemon — the desktop app's
//! tray, `gglib daemon stop` — can still stop it cleanly through the same
//! ordered teardown the signal path runs.

use axum::{Json, extract::State, http::StatusCode};
use serde_json::{Value, json};

use crate::state::AppState;

/// Ask the daemon to shut down.
///
/// Responds `202 Accepted` immediately; the teardown (proxy drain, child
/// shutdown, pidfile audit) runs after the HTTP server stops accepting.
/// `409 Conflict` when this server is not hosted by `run_daemon` — an
/// embedded or test instance has no daemon lifecycle to end.
pub async fn shutdown(State(state): State<AppState>) -> (StatusCode, Json<Value>) {
    match &state.daemon_shutdown {
        Some(token) => {
            tracing::info!("shutdown requested via POST /api/daemon/shutdown");
            token.cancel();
            (StatusCode::ACCEPTED, Json(json!({ "stopping": true })))
        }
        None => (
            StatusCode::CONFLICT,
            Json(json!({ "error": "this server is not running as the gglib daemon" })),
        ),
    }
}
