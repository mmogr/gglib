//! Application state shared across all Tauri commands.

use std::sync::Arc;

use gglib_app_services::{DownloadOps, ServerOps};
use gglib_axum::EmbeddedApiInfo;
use tauri::async_runtime::JoinHandle;
use tokio::sync::RwLock;

use crate::menu::AppMenu;

/// Application state with shared backend.
///
/// This struct is managed by Tauri and accessible to all commands
/// via `tauri::State<'_, AppState>`.
pub struct AppState {
    /// Server lifecycle operations.
    pub servers: Arc<ServerOps>,
    /// Download queue operations.
    pub downloads: Arc<DownloadOps>,
    /// Embedded API server info (port and auth token)
    pub embedded_api: EmbeddedApiInfo,
    /// Menu state for dynamic updates
    pub menu: Arc<RwLock<Option<AppMenu>>>,
    /// Currently selected model ID (for menu state sync)
    pub selected_model_id: Arc<RwLock<Option<i64>>>,
    /// Proxy server enabled state (for menu sync)
    pub proxy_enabled: Arc<RwLock<bool>>,
    /// Proxy server port (for copy URL)
    pub proxy_port: Arc<RwLock<Option<u16>>>,
    /// Background task handles for proper cleanup
    pub background_tasks: Arc<RwLock<BackgroundTasks>>,
}

/// Background task handles that need to be aborted on shutdown.
pub struct BackgroundTasks {
    /// Embedded API server task
    pub embedded_server: Option<JoinHandle<()>>,
    /// Server log event emitter task
    pub log_emitter: Option<JoinHandle<()>>,
}

impl AppState {
    /// Create a new application state.
    pub fn new(
        servers: Arc<ServerOps>,
        downloads: Arc<DownloadOps>,
        embedded_api: EmbeddedApiInfo,
    ) -> Self {
        Self {
            servers,
            downloads,
            embedded_api,
            menu: Arc::new(RwLock::new(None)),
            selected_model_id: Arc::new(RwLock::new(None)),
            proxy_enabled: Arc::new(RwLock::new(false)),
            proxy_port: Arc::new(RwLock::new(None)),
            background_tasks: Arc::new(RwLock::new(BackgroundTasks {
                embedded_server: None,
                log_emitter: None,
            })),
        }
    }
}
