//! Shared port-validation utilities for Axum handlers.
//!
//! Provides [`validate_port`] — an SSRF guard used by both the chat proxy
//! and the agent chat handler before forwarding requests to a llama-server.

use crate::error::HttpError;
use crate::state::AppState;

/// Allowed port range for llama-server connections.
/// Prevents the endpoints from becoming generic SSRF dialers.
const MIN_ALLOWED_PORT: u16 = 1024;
const MAX_ALLOWED_PORT: u16 = 65535;

/// Validate that `port` is within the allowed range **and** corresponds to a
/// currently-running llama-server.
///
/// # Errors
///
/// Returns [`HttpError::BadRequest`] when:
/// - `port` is outside `1024–65535`
/// - No running server is registered on that port
pub(crate) async fn validate_port(state: &AppState, port: u16) -> Result<(), HttpError> {
    // Basic range check — blocks well-known privileged ports and invalid values.
    if !(MIN_ALLOWED_PORT..=MAX_ALLOWED_PORT).contains(&port) {
        return Err(HttpError::BadRequest(format!(
            "Port {port} is outside the allowed range ({MIN_ALLOWED_PORT}–{MAX_ALLOWED_PORT})"
        )));
    }

    // Check that the port belongs to a server we started.
    let servers = state.gui.list_servers().await;
    if !servers.iter().any(|s| s.port == port) {
        return Err(HttpError::BadRequest(format!(
            "No running server found on port {port}. Start a server first."
        )));
    }

    Ok(())
}
