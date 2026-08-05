//! Application state shared across all Tauri commands.

use std::sync::Arc;

use tokio::sync::RwLock;

use crate::daemon::Daemon;
#[cfg(target_os = "macos")]
use crate::menu::AppMenu;
use crate::tray::Tray;

/// Application state with the daemon connection.
///
/// The desktop app is a dashboard: everything backend-shaped goes through
/// [`Daemon`]'s HTTP API. What remains here is OS-surface state — menu,
/// tray, and the mirror of proxy state those surfaces display.
///
/// This struct is managed by Tauri and accessible to all commands
/// via `tauri::State<'_, AppState>`.
pub struct AppState {
    /// The connection to the gglib daemon (external or hosted in-process).
    pub daemon: Arc<Daemon>,
    /// Menu state for dynamic updates (macOS application menu only)
    #[cfg(target_os = "macos")]
    pub menu: Arc<RwLock<Option<AppMenu>>>,
    /// The live tray, whichever backend built it. Held here because
    /// dropping it would remove the icon.
    pub tray: Arc<RwLock<Option<Tray>>>,
    /// Currently selected model ID (for menu state sync)
    pub selected_model_id: Arc<RwLock<Option<i64>>>,
    /// Proxy server enabled state (for menu sync)
    pub proxy_enabled: Arc<RwLock<bool>>,
    /// Proxy server port (for copy URL)
    pub proxy_port: Arc<RwLock<Option<u16>>>,
}

impl AppState {
    /// Create a new application state around a connected daemon.
    pub fn new(daemon: Arc<Daemon>) -> Self {
        Self {
            daemon,
            #[cfg(target_os = "macos")]
            menu: Arc::new(RwLock::new(None)),
            tray: Arc::new(RwLock::new(None)),
            selected_model_id: Arc::new(RwLock::new(None)),
            proxy_enabled: Arc::new(RwLock::new(false)),
            proxy_port: Arc::new(RwLock::new(None)),
        }
    }
}
