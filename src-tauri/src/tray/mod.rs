#![doc = include_str!("README.md")]
mod build;
mod handlers;
mod icon;
mod ids;
#[cfg(target_os = "linux")]
mod layer_shell;
pub mod placement;
pub mod window;

pub use build::{TRAY_ID, TrayMenu, build};
pub use icon::derive;

use tauri::{AppHandle, Manager};

use crate::app::AppState;

/// Apply proxy state to the tray icon, tooltip and menu.
///
/// A no-op until the tray has been built, so the initial sync during setup is
/// harmless. Called from `menu::state_sync::sync_all_state` rather than
/// directly, so the tray cannot fall out of step with the application menu.
pub async fn sync(
    app: &AppHandle,
    proxy_running: bool,
    proxy_port: Option<u16>,
) -> Result<(), String> {
    let Some(tray) = app.tray_by_id(TRAY_ID) else {
        return Ok(());
    };

    let visual = derive(proxy_running, proxy_port);

    let image = if visual.active {
        build::active_icon()
    } else {
        build::idle_icon()
    }
    .map_err(|e| format!("Failed to decode tray icon: {e}"))?;

    tray.set_icon(Some(image))
        .map_err(|e| format!("Failed to set tray icon: {e}"))?;
    // A no-op on Linux, where the menu's status item carries this instead.
    tray.set_tooltip(Some(&visual.status))
        .map_err(|e| format!("Failed to set tray tooltip: {e}"))?;

    let state = app.state::<AppState>();
    let menu_guard = state.tray_menu.read().await;
    if let Some(menu) = menu_guard.as_ref() {
        menu.status
            .set_text(&visual.status)
            .and_then(|()| menu.start_proxy.set_enabled(!proxy_running))
            .and_then(|()| menu.stop_proxy.set_enabled(proxy_running))
            .and_then(|()| menu.copy_proxy_url.set_enabled(proxy_running))
            .map_err(|e| format!("Failed to sync tray menu: {e}"))?;
    }

    Ok(())
}
