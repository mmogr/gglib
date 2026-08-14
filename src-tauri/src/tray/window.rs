//! Showing, hiding and positioning the tray panel window.

use tauri::{AppHandle, Manager};
use tracing::debug;

use crate::dock;
use crate::tray::placement::{self, Anchor};

/// Window label of the tray panel, as declared in `tauri.conf.json`.
pub(crate) const PANEL_LABEL: &str = "tray";

/// Window label of the main application window.
pub(crate) const MAIN_LABEL: &str = "main";

/// Show the panel if hidden, hide it if already visible.
///
/// Clicking the tray icon while the panel is open should put it away again,
/// which is what every other menu bar item does.
pub(crate) fn toggle_panel(app: &AppHandle, anchor: Anchor) -> tauri::Result<()> {
    let Some(panel) = app.get_webview_window(PANEL_LABEL) else {
        debug!("Tray panel window not found");
        return Ok(());
    };

    if panel.is_visible().unwrap_or(false) {
        return panel.hide();
    }

    placement::place(&panel, anchor)?;

    panel.show()?;
    panel.set_focus()
}

/// Show and focus the main window, restoring it if it was minimised.
///
/// The one way back from menu-bar-only mode, shared by the tray's Open item,
/// the tray icon's double-click and macOS's dock-reopen event — which is why
/// the Dock icon is restored here rather than at each of those call sites.
pub(crate) fn show_main(app: &AppHandle) -> tauri::Result<()> {
    let Some(main) = app.get_webview_window(MAIN_LABEL) else {
        debug!("Main window not found");
        return Ok(());
    };

    // Before showing, not after: an accessory app cannot become frontmost, so
    // a window shown first would open behind whatever the user was in.
    dock::show(app);

    if main.is_minimized().unwrap_or(false) {
        main.unminimize()?;
    }

    main.show()?;
    main.set_focus()
}
