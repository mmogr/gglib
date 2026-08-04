//! Application state shared across all Tauri commands.

use std::sync::Arc;

use gglib_app_services::{DownloadOps, ProxyOps, ServerOps};
use gglib_axum::EmbeddedApiInfo;
use gglib_axum::sse::SseBroadcaster;
use gglib_core::services::AppCore;
use tauri::async_runtime::JoinHandle;
use tokio::sync::RwLock;

#[cfg(target_os = "macos")]
use crate::menu::AppMenu;
use crate::tray::Tray;

/// Application state with shared backend.
///
/// This struct is managed by Tauri and accessible to all commands
/// via `tauri::State<'_, AppState>`.
pub struct AppState {
    /// Server lifecycle operations.
    pub servers: Arc<ServerOps>,
    /// Download queue operations.
    pub downloads: Arc<DownloadOps>,
    /// Proxy lifecycle operations, shared with the embedded Axum server so
    /// the tray and the UI drive the same supervisor.
    pub proxy: Arc<ProxyOps>,
    /// Core application facade, for reading settings outside a request.
    pub core: Arc<AppCore>,
    /// Embedded API server info (port and auth token)
    pub embedded_api: EmbeddedApiInfo,
    /// Lifecycle event broadcaster, shared with the embedded Axum server so
    /// proxy changes driven from the tray still reach the UI.
    pub sse: Arc<SseBroadcaster>,
    /// Menu state for dynamic updates (macOS application menu only)
    #[cfg(target_os = "macos")]
    pub menu: Arc<RwLock<Option<AppMenu>>>,
    /// Tray menu items whose enabled state tracks the proxy
    /// The live tray, whichever backend built it. Held here because
    /// dropping it would remove the icon.
    pub tray: Arc<RwLock<Option<Tray>>>,
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
        proxy: Arc<ProxyOps>,
        core: Arc<AppCore>,
        embedded_api: EmbeddedApiInfo,
        sse: Arc<SseBroadcaster>,
    ) -> Self {
        Self {
            servers,
            downloads,
            proxy,
            core,
            embedded_api,
            sse,
            #[cfg(target_os = "macos")]
            menu: Arc::new(RwLock::new(None)),
            tray: Arc::new(RwLock::new(None)),
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
