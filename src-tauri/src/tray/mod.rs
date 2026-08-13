#![doc = include_str!("README.md")]
#[cfg(not(target_os = "linux"))]
mod build;
mod confirm;
mod handlers;
mod icon;
mod ids;
mod items;
#[cfg(target_os = "linux")]
mod layer_shell;
#[cfg(target_os = "linux")]
mod linux;
pub mod placement;
pub mod window;

use tauri::{AppHandle, Manager};

use crate::app::AppState;
use crate::daemon::DaemonSnapshot;

#[cfg(not(target_os = "linux"))]
use build as backend;
/// The backend that actually owns the tray on this platform.
///
/// Selected here and nowhere else. Both modules expose the same three items —
/// `Handle`, `build` and `sync` — so everything above this line is written
/// once, and no `#[cfg]` reaches `main`, `menu::state_sync` or `AppState`.
#[cfg(target_os = "linux")]
use linux as backend;

/// A live tray, kept for as long as the icon should exist.
///
/// Dropping this removes the icon, so it is owned by [`AppState`] rather than
/// left to fall out of scope at the end of setup.
pub struct Tray(backend::Handle);

impl Tray {
    /// Build the tray and register it with the desktop.
    ///
    /// # Errors
    ///
    /// When the platform refuses to give us one — no tray host on Linux, or a
    /// failed icon decode anywhere. Callers treat that as a degraded app, not
    /// a broken one.
    pub fn build(app: &AppHandle) -> Result<Self, String> {
        backend::build(app).map(Self)
    }

    /// Apply the daemon's state to this tray.
    ///
    /// # Errors
    ///
    /// When the backend cannot update the icon, tooltip or menu.
    pub async fn sync(&self, snapshot: &DaemonSnapshot) -> Result<(), String> {
        backend::sync(&self.0, snapshot).await
    }
}

/// Apply the daemon's state to the tray, if one was built.
///
/// A no-op until then, so the initial sync during setup is harmless. Called
/// from `menu::state_sync::sync_all_state` rather than directly, so the tray
/// cannot fall out of step with the macOS application menu.
///
/// # Errors
///
/// Propagates whatever the backend reports.
pub async fn sync(app: &AppHandle, snapshot: &DaemonSnapshot) -> Result<(), String> {
    let state = app.state::<AppState>();
    let guard = state.tray.read().await;

    match guard.as_ref() {
        Some(tray) => tray.sync(snapshot).await,
        None => Ok(()),
    }
}
