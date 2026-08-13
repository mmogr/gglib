//! Application state shared across all Tauri commands.

use std::sync::Arc;

use tokio::sync::RwLock;

use crate::daemon::{Daemon, DaemonSnapshot, Refresh};
#[cfg(target_os = "macos")]
use crate::menu::AppMenu;
use crate::tray::Tray;

/// Application state with the daemon connection.
///
/// The desktop app is a dashboard: everything backend-shaped goes through
/// [`Daemon`]'s HTTP API. What remains here is OS-surface state — menu, tray,
/// and the picture of the daemon those surfaces paint from.
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
    /// What the daemon is doing, as of the last poll.
    ///
    /// The single source of truth every surface paints from. Written only by
    /// `daemon::watch`; everything else reads it, and anything that changes
    /// the daemon asks for a fresh poll through [`Self::refresh`] rather than
    /// writing what it expects to be true.
    pub snapshot: Arc<RwLock<DaemonSnapshot>>,
    /// Asks the watcher for an immediate poll.
    pub refresh: Refresh,
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
            snapshot: Arc::new(RwLock::new(DaemonSnapshot::default())),
            refresh: Refresh::default(),
        }
    }
}
