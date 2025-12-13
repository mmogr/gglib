//! Proxy management commands.
//!
//! Provides Tauri commands for starting, stopping, and querying the
//! OpenAI-compatible proxy server.

use crate::app::AppState;
use gglib_runtime::proxy::ProxyConfig;

/// Proxy status DTO for frontend consumption.
#[derive(serde::Serialize)]
pub struct ProxyStatusDto {
    pub running: bool,
    pub port: u16,
    pub current_model: Option<String>,
    pub model_port: Option<u16>,
}

impl From<gglib_runtime::proxy::ProxyStatus> for ProxyStatusDto {
    fn from(s: gglib_runtime::proxy::ProxyStatus) -> Self {
        match s {
            gglib_runtime::proxy::ProxyStatus::Stopped => Self {
                running: false,
                port: 8080,
                current_model: None,
                model_port: None,
            },
            gglib_runtime::proxy::ProxyStatus::Crashed => Self {
                running: false,
                port: 8080,
                current_model: None,
                model_port: None,
            },
            gglib_runtime::proxy::ProxyStatus::Running { address } => Self {
                running: true,
                port: address.port(),
                current_model: None,
                model_port: None,
            },
        }
    }
}

/// Arguments for starting the proxy.
#[derive(serde::Deserialize)]
pub struct StartProxyArgs {
    /// Host to bind to (default: 127.0.0.1)
    pub host: Option<String>,
    /// Port to bind to (default: 8080)
    pub port: Option<u16>,
    /// Default context size for models
    pub default_context: Option<u64>,
    /// Base port for llama-server instances
    pub llama_base_port: Option<u16>,
    /// Path to llama-server binary
    pub llama_server_path: Option<String>,
}

#[tauri::command]
pub async fn start_proxy(
    state: tauri::State<'_, AppState>,
    config: Option<StartProxyArgs>,
) -> Result<ProxyStatusDto, String> {
    let args = config.unwrap_or(StartProxyArgs {
        host: None,
        port: None,
        default_context: None,
        llama_base_port: None,
        llama_server_path: None,
    });

    let proxy_config = ProxyConfig {
        host: args.host.unwrap_or_else(|| "127.0.0.1".to_string()),
        port: args.port.unwrap_or(8080),
        default_context: args.default_context.unwrap_or(4096),
    };

    // Backend resolves llama_base_port from override → settings → default
    let llama_base_port_override = args.llama_base_port;
    let llama_server_path = args.llama_server_path.unwrap_or_else(|| {
        gglib_core::paths::llama_server_path()
            .map(|p| p.to_string_lossy().to_string())
            .unwrap_or_else(|_| "llama-server".to_string())
    });

    let addr = state
        .gui
        .proxy_start(proxy_config, llama_base_port_override, llama_server_path)
        .await
        .map_err(|e| e.to_string())?;

    Ok(ProxyStatusDto {
        running: true,
        port: addr.port(),
        current_model: None,
        model_port: None,
    })
}

#[tauri::command]
pub async fn stop_proxy(state: tauri::State<'_, AppState>) -> Result<(), String> {
    state.gui.proxy_stop().await.map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn get_proxy_status(state: tauri::State<'_, AppState>) -> Result<ProxyStatusDto, String> {
    Ok(state.gui.proxy_status().await.into())
}
