//! Proxy handlers - OpenAI-compatible proxy management.

use axum::{Json, extract::State};

use crate::{error::HttpError, state::AppState};
use gglib_core::paths::llama_server_path;
use gglib_runtime::proxy::ProxyConfig as RuntimeProxyConfig;
use gglib_runtime::proxy::ProxyStatus as RuntimeProxyStatus;

/// Proxy status response.
/// Matches Tauri's ProxyStatus for frontend compatibility.
#[derive(Debug, Clone, serde::Serialize)]
pub struct ProxyStatus {
    pub running: bool,
    pub port: Option<u16>,
    pub current_model: Option<String>,
    pub model_port: Option<u16>,
}

/// Optional configuration for starting the proxy.
#[derive(Debug, Clone, Default, serde::Deserialize)]
pub struct StartProxyConfig {
    pub host: Option<String>,
    pub port: Option<u16>,
    pub llama_base_port: Option<u16>,
    pub default_context: Option<u64>,
}

/// Convert runtime ProxyStatus to API ProxyStatus.
fn to_api_status(s: RuntimeProxyStatus) -> ProxyStatus {
    match s {
        RuntimeProxyStatus::Stopped => ProxyStatus {
            running: false,
            port: None,
            current_model: None,
            model_port: None,
        },
        RuntimeProxyStatus::Running { address } => ProxyStatus {
            running: true,
            port: Some(address.port()),
            current_model: None,
            model_port: None,
        },
        RuntimeProxyStatus::Crashed => ProxyStatus {
            running: false,
            port: None,
            current_model: None,
            model_port: None,
        },
    }
}

/// Fetch current proxy status from backend.
async fn fetch_status(state: &AppState) -> ProxyStatus {
    let s = state.gui.proxy_status().await;
    to_api_status(s)
}

/// Convert handler config to runtime config with defaults.
fn to_runtime_config(cfg: &StartProxyConfig) -> RuntimeProxyConfig {
    RuntimeProxyConfig {
        host: cfg.host.clone().unwrap_or_else(|| "127.0.0.1".to_string()),
        port: cfg.port.unwrap_or(11444),
        default_context: cfg.default_context.unwrap_or(4096),
    }
}

/// Get current proxy status.
pub async fn status(State(state): State<AppState>) -> Json<ProxyStatus> {
    Json(fetch_status(&state).await)
}

/// Start the proxy (idempotent).
pub async fn start(
    State(state): State<AppState>,
    Json(cfg): Json<Option<StartProxyConfig>>,
) -> Result<Json<ProxyStatus>, HttpError> {
    let cfg = cfg.unwrap_or_default();

    // Resolve llama-server path on demand
    let llama_path = llama_server_path()
        .map_err(|e| HttpError::Internal(format!("Failed to resolve llama-server path: {}", e)))?
        .to_string_lossy()
        .into_owned();

    let runtime_cfg = to_runtime_config(&cfg);

    // Idempotent: if already running (Conflict), treat as success
    match state
        .gui
        .proxy_start(runtime_cfg, cfg.llama_base_port, llama_path)
        .await
    {
        Ok(_addr) => {}
        Err(e) => {
            let http: HttpError = e.into();
            if !matches!(http, HttpError::Conflict(_)) {
                return Err(http);
            }
        }
    }

    Ok(Json(fetch_status(&state).await))
}

/// Stop the proxy (idempotent).
pub async fn stop(State(state): State<AppState>) -> Result<Json<ProxyStatus>, HttpError> {
    // Idempotent: if not running (Conflict), treat as success
    match state.gui.proxy_stop().await {
        Ok(()) => {}
        Err(e) => {
            let http: HttpError = e.into();
            if !matches!(http, HttpError::Conflict(_)) {
                return Err(http);
            }
        }
    }

    Ok(Json(fetch_status(&state).await))
}
