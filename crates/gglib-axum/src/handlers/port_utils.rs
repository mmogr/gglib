//! Shared port-validation utilities for Axum handlers.
//!
//! Provides [`validate_port`] — an SSRF guard used by both the chat proxy
//! and the agent chat handler before forwarding requests to a llama-server.

use gglib_app_services::types::ServerInfo;

use crate::error::HttpError;
use crate::state::AppState;

/// Minimum allowed port for llama-server connections.
/// Prevents the endpoints from becoming generic SSRF dialers.
const MIN_ALLOWED_PORT: u16 = 1024;

/// Validate that `port` is within the allowed range **and** corresponds to a
/// currently-running llama-server, and say which server that is.
///
/// The [`ServerInfo`] is the one this function looks the port up in anyway.
/// It is returned rather than dropped because the model actually running on
/// the port is the honest answer to "which model is this traffic?" when the
/// request named none — the ordinary local case (#1091). A caller with no use
/// for it writes `validate_port(…).await?;` and the value is discarded.
///
/// # Errors
///
/// Returns [`HttpError::BadRequest`] when:
/// - `port` is below `1024` (privileged / reserved)
/// - No running server is registered on that port
pub(crate) async fn validate_port(state: &AppState, port: u16) -> Result<ServerInfo, HttpError> {
    // Block well-known privileged ports.
    if port < MIN_ALLOWED_PORT {
        return Err(HttpError::BadRequest(format!(
            "Port {port} is below the minimum allowed port ({MIN_ALLOWED_PORT})"
        )));
    }

    // Check that the port belongs to a server we started.
    let servers = state.servers.list_servers().await;
    servers.into_iter().find(|s| s.port == port).ok_or_else(|| {
        HttpError::BadRequest(format!(
            "No running server found on port {port}. Start a server first."
        ))
    })
}
