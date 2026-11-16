//! Application state shared across all Tauri commands.

use std::sync::Arc;

use tokio::sync::RwLock;

use crate::daemon::{Daemon, DaemonSnapshot, Launch, Refresh};
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
pub(crate) struct AppState {
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
    /// The application state, and the launch of its daemon that `setup`
    /// starts ([`Daemon::connect_or_launch`]).
    ///
    /// Takes nothing, because it is managed on the builder, before `build`
    /// creates the windows: a command a page sends can never find it
    /// missing ("state not managed"). What is not known yet, the daemon,
    /// is asked through [`Daemon::service`], which waits for the launch.
    pub(crate) fn new() -> (Self, Launch) {
        let (daemon, launch) = Daemon::new();
        let state = Self {
            daemon: Arc::new(daemon),
            #[cfg(target_os = "macos")]
            menu: Arc::new(RwLock::new(None)),
            tray: Arc::new(RwLock::new(None)),
            selected_model_id: Arc::new(RwLock::new(None)),
            snapshot: Arc::new(RwLock::new(DaemonSnapshot::starting())),
            refresh: Refresh::default(),
        };
        (state, launch)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The state is built with no app, no window and no daemon, so nothing
    /// stops it being managed before the first window exists, and its
    /// daemon is unknown until the launch it comes with settles.
    #[tokio::test]
    async fn the_state_needs_no_window_and_no_daemon() {
        let (state, launch) = AppState::new();
        assert!(state.daemon.service.is_starting());
        assert!(state.snapshot.read().await.starting);

        launch.settle(Err("no daemon in a unit test".to_owned()));
        assert!(!state.daemon.service.is_starting());
    }
}
