//! Shared port-validation utilities for Axum handlers.
//!
//! Provides [`validate_port`] — an SSRF guard used by both the chat proxy
//! and the agent chat handler before forwarding requests to a llama-server.

use crate::error::HttpError;
use crate::state::AppState;

/// Minimum allowed port for llama-server connections.
/// Prevents the endpoints from becoming generic SSRF dialers.
const MIN_ALLOWED_PORT: u16 = 1024;

/// Validate that `port` is within the allowed range **and** corresponds to a
/// currently-running llama-server.
///
/// # Errors
///
/// Returns [`HttpError::BadRequest`] when:
/// - `port` is below `1024` (privileged / reserved)
/// - No running server is registered on that port
pub(crate) async fn validate_port(state: &AppState, port: u16) -> Result<(), HttpError> {
    // Block well-known privileged ports.
    if port < MIN_ALLOWED_PORT {
        return Err(HttpError::BadRequest(format!(
            "Port {port} is below the minimum allowed port ({MIN_ALLOWED_PORT})"
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
