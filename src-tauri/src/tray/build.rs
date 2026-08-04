//! Tray construction for the `muda` backend used on macOS and Windows.
//!
//! Renders [`super::items::ITEMS`] rather than listing the menu itself, so the
//! Linux backend cannot drift from it.

use std::collections::HashMap;

use tauri::image::Image;
use tauri::menu::{IsMenuItem, Menu, MenuItem, PredefinedMenuItem};
use tauri::tray::{MouseButton, MouseButtonState, TrayIcon, TrayIconBuilder, TrayIconEvent};
use tauri::{AppHandle, Wry};
use tracing::error;

use crate::tray::items::{ITEMS, Item, is_enabled};
use crate::tray::placement::Anchor;
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

/// The built menu items, keyed by id so `sync` can find them again.
///
/// A map rather than named fields because the item list lives in `items` now;
/// naming them here would be a second copy of the same structure.
pub struct TrayMenu {
    items: HashMap<&'static str, MenuItem<Wry>>,
}

impl TrayMenu {
    /// Apply proxy state to every item that tracks it.
    pub fn sync(&self, status: &str, proxy_running: bool) -> tauri::Result<()> {
        for (id, item) in &self.items {
            if *id == ids::STATUS {
                item.set_text(status)?;
            } else {
                item.set_enabled(is_enabled(id, proxy_running))?;
            }
        }
        Ok(())
    }
}

/// Build the tray icon, its menu, and its click behaviour.
pub fn build(app: &AppHandle) -> tauri::Result<(TrayIcon, TrayMenu)> {
    let mut items: HashMap<&'static str, MenuItem<Wry>> = HashMap::new();
    // Separators are positional and never looked up again, so they are kept
    // only to be borrowed into the menu below.
    let mut separators = Vec::new();
    let mut order: Vec<(Option<&'static str>, usize)> = Vec::new();

    for item in ITEMS {
        match item {
            // Disabled: a label, not a command. It carries the endpoint into
            // the one surface every platform renders.
            Item::Status => {
                let status = MenuItem::with_id(
                    app,
                    ids::STATUS,
                    icon::derive(false, None).status,
                    false,
                    None::<&str>,
                )?;
                items.insert(ids::STATUS, status);
                order.push((Some(ids::STATUS), 0));
            }
            Item::Action { id, label } => {
                let built =
                    MenuItem::with_id(app, *id, label, is_enabled(id, false), None::<&str>)?;
                items.insert(id, built);
                order.push((Some(id), 0));
            }
            Item::Separator => {
                separators.push(PredefinedMenuItem::separator(app)?);
                order.push((None, separators.len() - 1));
            }
        }
    }

    let refs: Vec<&dyn IsMenuItem<Wry>> = order
        .iter()
        .map(|(id, index)| match id {
            Some(id) => &items[id] as &dyn IsMenuItem<Wry>,
            None => &separators[*index] as &dyn IsMenuItem<Wry>,
        })
        .collect();

    let menu = Menu::with_items(app, &refs)?;
    drop(refs);

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

    Ok((tray, TrayMenu { items }))
}

/// Open the panel on a completed left click.
///
/// Only `Up` is acted on so the panel does not appear under a button that is
/// still held down. Never fires on Linux, which is why that platform uses a
/// different backend entirely — see the module README.
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
    if let Err(e) = window::toggle_panel(app, Anchor::Rect(rect)) {
        error!(error = %e, "Failed to open tray panel");
    }
}
