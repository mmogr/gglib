//! Proxy handlers - OpenAI-compatible proxy management.

use axum::{Json, extract::State};

use crate::{error::HttpError, state::AppState};
use gglib_core::ports::AppEventEmitter;
use gglib_core::settings::DEFAULT_PROXY_PORT;
use gglib_runtime::proxy::ProxyConfig as RuntimeProxyConfig;
use gglib_runtime::proxy::ProxyStatus as RuntimeProxyStatus;
use gglib_runtime::server_config::{ServerConfigOptions, resolve_context_size};

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
    let s = state.proxy.status().await;
    to_api_status(s)
}

/// Convert handler config to runtime config with defaults.
fn to_runtime_config(
    cfg: &StartProxyConfig,
    settings_default: Option<u64>,
) -> Result<RuntimeProxyConfig, HttpError> {
    let default_context = resolve_context_size(&ServerConfigOptions {
        context_size: cfg.default_context,
        global_default_ctx: settings_default,
        ..Default::default()
    });

    Ok(RuntimeProxyConfig {
        host: cfg.host.clone().unwrap_or_else(|| "127.0.0.1".to_string()),
        port: cfg.port.unwrap_or(DEFAULT_PROXY_PORT),
        default_context,
    })
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

    // Resolve context size through the shared 3-level fallback chain
    // (flag > settings default > hard-coded default), matching CLI behavior.
    let settings = state.settings.get().await?;
    let runtime_cfg = to_runtime_config(&cfg, settings.default_context_size)?;

    // Idempotent: if already running (Conflict), treat as success
    match state.proxy.start(runtime_cfg).await {
        Ok(_addr) => {}
        Err(e) => {
            let http: HttpError = e.into();
            if !matches!(http, HttpError::Conflict(_)) {
                return Err(http);
            }
        }
    }

    let status = fetch_status(&state).await;

    // Emit proxy started event if proxy is now running
    if status.running
        && let Some(port) = status.port
    {
        state
            .sse
            .emit(gglib_core::events::AppEvent::proxy_started(port));
    }

    Ok(Json(status))
}

/// Stop the proxy (idempotent).
pub async fn stop(State(state): State<AppState>) -> Result<Json<ProxyStatus>, HttpError> {
    // Idempotent: if not running (Conflict), treat as success
    match state.proxy.stop().await {
        Ok(()) => {
            // Emit proxy stopped event on clean shutdown
            state
                .sse
                .emit(gglib_core::events::AppEvent::proxy_stopped());
        }
        Err(e) => {
            let http: HttpError = e.into();
            if !matches!(http, HttpError::Conflict(_)) {
                return Err(http);
            }
        }
    }

    Ok(Json(fetch_status(&state).await))
}
