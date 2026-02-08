//! Application state shared across all Tauri commands.

use gglib_axum::EmbeddedApiInfo;
use gglib_tauri::gui_backend::GuiBackend;
use gglib_voice::pipeline::VoicePipeline;
use std::sync::Arc;
use tauri::async_runtime::JoinHandle;
use tokio::sync::RwLock;

use crate::menu::AppMenu;

/// Application state with shared backend.
///
/// This struct is managed by Tauri and accessible to all commands
/// via `tauri::State<'_, AppState>`.
pub struct AppState {
    /// Shared GUI backend (new architecture from gglib-tauri)
    pub gui: Arc<GuiBackend>,
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
    /// Voice pipeline (None when voice mode is inactive)
    pub voice_pipeline: Arc<RwLock<Option<VoicePipeline>>>,
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
    pub fn new(gui: Arc<GuiBackend>, embedded_api: EmbeddedApiInfo) -> Self {
        Self {
            gui,
            embedded_api,
            menu: Arc::new(RwLock::new(None)),
            selected_model_id: Arc::new(RwLock::new(None)),
            proxy_enabled: Arc::new(RwLock::new(false)),
            proxy_port: Arc::new(RwLock::new(None)),
            background_tasks: Arc::new(RwLock::new(BackgroundTasks {
                embedded_server: None,
                log_emitter: None,
            })),
            voice_pipeline: Arc::new(RwLock::new(None)),
        }
    }
}
