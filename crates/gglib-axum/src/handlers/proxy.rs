//! Proxy handlers - OpenAI-compatible proxy management.
//!
//! NOTE: Proxy is temporarily disabled during Phase 2 refactor (#221).
//! The proxy functionality will be re-enabled once it's extracted to a proper crate.
//! These handlers mirror the Tauri command behavior for API parity.

use axum::Json;

use crate::error::HttpError;

/// Proxy status response.
/// Matches Tauri's ProxyStatus for frontend compatibility.
#[derive(Debug, Clone, serde::Serialize)]
pub struct ProxyStatus {
    pub running: bool,
    pub port: Option<u16>,
    pub current_model: Option<String>,
    pub model_port: Option<u16>,
}

impl ProxyStatus {
    /// Create a "not running" status.
    pub fn disabled() -> Self {
        Self {
            running: false,
            port: None,
            current_model: None,
            model_port: None,
        }
    }
}

/// Optional configuration for starting the proxy.
#[derive(Debug, serde::Deserialize)]
pub struct StartProxyConfig {
    pub host: Option<String>,
    pub port: Option<u16>,
    pub start_port: Option<u16>,
    pub default_context: Option<u64>,
}

/// Get current proxy status.
/// Returns "not running" during Phase 2 refactor.
pub async fn status() -> Json<ProxyStatus> {
    Json(ProxyStatus::disabled())
}

/// Start the proxy.
/// Temporarily disabled during Phase 2 refactor (#221).
pub async fn start(
    Json(_config): Json<Option<StartProxyConfig>>,
) -> Result<Json<ProxyStatus>, HttpError> {
    Err(HttpError::ServiceUnavailable(
        "Proxy temporarily disabled during Phase 2 refactor".to_string(),
    ))
}

/// Stop the proxy.
/// Temporarily disabled during Phase 2 refactor (#221).
pub async fn stop() -> Result<Json<()>, HttpError> {
    Err(HttpError::ServiceUnavailable(
        "Proxy temporarily disabled during Phase 2 refactor".to_string(),
    ))
}
