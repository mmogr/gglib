//! Tray construction.

use tauri::image::Image;
use tauri::menu::{Menu, MenuItem, PredefinedMenuItem};
use tauri::tray::{MouseButton, MouseButtonState, TrayIcon, TrayIconBuilder, TrayIconEvent};
use tauri::{AppHandle, Wry};
use tracing::error;

use crate::tray::{handlers, icon, ids, window};

/// Identifier used to look the tray back up via `AppHandle::tray_by_id`.
pub const TRAY_ID: &str = "gglib";

/// Decoded idle icon (proxy stopped).
pub fn idle_icon() -> tauri::Result<Image<'static>> {
    Image::from_bytes(include_bytes!("../../icons/tray-idle.png"))
}

/// Decoded active icon (proxy serving).
pub fn active_icon() -> tauri::Result<Image<'static>> {
    Image::from_bytes(include_bytes!("../../icons/tray-active.png"))
}

/// Items whose state tracks the proxy.
pub struct TrayMenu {
    /// Disabled header showing where the endpoint is, updated by `tray::sync`.
    pub status: MenuItem<Wry>,
    pub start_proxy: MenuItem<Wry>,
    pub stop_proxy: MenuItem<Wry>,
    pub copy_proxy_url: MenuItem<Wry>,
}

/// Build the tray icon, its menu, and its click behaviour.
///
/// Every action lives on the menu, including the ones that also have a click
/// gesture. Linux's AppIndicator delivers no click events at all, so a feature
/// reachable only by clicking would simply not exist there.
pub fn build(app: &AppHandle) -> tauri::Result<(TrayIcon, TrayMenu)> {
    // Disabled: a label, not a command. It carries the endpoint into the one
    // surface every platform renders — tooltips are a documented no-op on
    // Linux, so without this the port is simply invisible there.
    let status = MenuItem::with_id(
        app,
        ids::STATUS,
        icon::derive(false, None).status,
        false,
        None::<&str>,
    )?;
    let open_panel = MenuItem::with_id(app, ids::OPEN_PANEL, "Proxy Panel", true, None::<&str>)?;
    let start_proxy = MenuItem::with_id(app, ids::START_PROXY, "Start Proxy", true, None::<&str>)?;
    let stop_proxy = MenuItem::with_id(app, ids::STOP_PROXY, "Stop Proxy", false, None::<&str>)?;
    let copy_proxy_url = MenuItem::with_id(
        app,
        ids::COPY_PROXY_URL,
        "Copy Endpoint URL",
        false,
        None::<&str>,
    )?;
    let open_main = MenuItem::with_id(app, ids::OPEN_MAIN, "Open gglib", true, None::<&str>)?;
    let preferences = MenuItem::with_id(app, ids::PREFERENCES, "Preferences…", true, None::<&str>)?;
    let quit = MenuItem::with_id(app, ids::QUIT, "Quit gglib", true, None::<&str>)?;

    let menu = Menu::with_items(
        app,
        &[
            &status,
            &PredefinedMenuItem::separator(app)?,
            &open_panel,
            &PredefinedMenuItem::separator(app)?,
            &start_proxy,
            &stop_proxy,
            &copy_proxy_url,
            &PredefinedMenuItem::separator(app)?,
            &open_main,
            &preferences,
            &PredefinedMenuItem::separator(app)?,
            &quit,
        ],
    )?;

    let tray = TrayIconBuilder::with_id(TRAY_ID)
        .icon(idle_icon()?)
        // macOS recolours template images for light and dark menu bars. The
        // icons are pure black with the glyph carried by alpha, which is what
        // that mode expects.
        .icon_as_template(true)
        .tooltip("gglib — proxy stopped")
        .menu(&menu)
        // Left click opens the panel; the menu stays on right click, so the
        // common action does not require reading a list first.
        .show_menu_on_left_click(false)
        .on_menu_event(handlers::handle)
        .on_tray_icon_event(on_icon_event)
        .build(app)?;

    Ok((
        tray,
        TrayMenu {
            status,
            start_proxy,
            stop_proxy,
            copy_proxy_url,
        },
    ))
}

/// Open the panel on a completed left click.
///
/// Only `Up` is acted on so the panel does not appear under a button that is
/// still held down. Never fires on Linux — see the module README.
fn on_icon_event(tray: &TrayIcon, event: TrayIconEvent) {
    let TrayIconEvent::Click {
        button: MouseButton::Left,
        button_state: MouseButtonState::Up,
        rect,
        ..
    } = event
    else {
        return;
    };

    let app = tray.app_handle();
    if let Err(e) = window::toggle_panel(app, Some(rect)) {
        error!(error = %e, "Failed to open tray panel");
    }
}
