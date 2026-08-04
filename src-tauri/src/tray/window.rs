//! Showing, hiding and positioning the tray panel window.

use tauri::{AppHandle, Manager, PhysicalPosition, Rect, WebviewWindow};
use tracing::debug;

use crate::dock;

/// Window label of the tray panel, as declared in `tauri.conf.json`.
pub const PANEL_LABEL: &str = "tray";

/// Window label of the main application window.
pub const MAIN_LABEL: &str = "main";

/// Show the panel if hidden, hide it if already visible.
///
/// Clicking the tray icon while the panel is open should put it away again,
/// which is what every other menu bar item does.
pub fn toggle_panel(app: &AppHandle, anchor: Option<Rect>) -> tauri::Result<()> {
    let Some(panel) = app.get_webview_window(PANEL_LABEL) else {
        debug!("Tray panel window not found");
        return Ok(());
    };

    if panel.is_visible().unwrap_or(false) {
        return panel.hide();
    }

    // No anchor means no positioning, which is the whole story on Linux: the
    // click event that carries the icon's rectangle never fires there, and
    // `TrayIcon::rect` is documented as always `None`. Falling back to the
    // cursor would be worse than leaving placement to the window manager —
    // under Wayland `cursor_position` returns (0, 0) rather than an error, so
    // the panel would jump to the top-left corner of the screen.
    if let Some(anchor) = anchor {
        position_near(&panel, anchor)?;
    }

    panel.show()?;
    panel.set_focus()
}

/// Place the panel under the tray icon, horizontally centred on it.
///
/// Clamped to the monitor's work area so an icon near the right-hand edge —
/// where menu bar items usually are — does not push the panel off-screen.
fn position_near(panel: &WebviewWindow, anchor: Rect) -> tauri::Result<()> {
    let scale = panel.scale_factor().unwrap_or(1.0);
    let anchor_position: PhysicalPosition<f64> = anchor.position.to_physical(scale);
    let anchor_size = anchor.size.to_physical::<f64>(scale);
    let panel_size = panel.outer_size()?;

    let mut x = anchor_position.x + (anchor_size.width / 2.0) - (f64::from(panel_size.width) / 2.0);
    let mut y = anchor_position.y + anchor_size.height;

    if let Ok(Some(monitor)) = panel.current_monitor() {
        let area = monitor.size();
        let origin = monitor.position();

        let max_x = f64::from(origin.x) + f64::from(area.width) - f64::from(panel_size.width);
        x = x.clamp(f64::from(origin.x), max_x.max(f64::from(origin.x)));

        // On a bottom-of-screen tray (Windows' default), dropping down would
        // land the panel off the bottom edge; put it above the icon instead.
        let below_bottom =
            y + f64::from(panel_size.height) > f64::from(origin.y) + f64::from(area.height);
        if below_bottom {
            y = anchor_position.y - f64::from(panel_size.height);
        }
    }

    panel.set_position(PhysicalPosition::new(x, y))
}

/// Show and focus the main window, restoring it if it was minimised.
///
/// The one way back from menu-bar-only mode, shared by the tray's Open item,
/// the tray icon's double-click and macOS's dock-reopen event — which is why
/// the Dock icon is restored here rather than at each of those call sites.
pub fn show_main(app: &AppHandle) -> tauri::Result<()> {
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
