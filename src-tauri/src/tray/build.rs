//! Tray construction for the `muda` backend, used on macOS and Windows.
//!
//! Renders [`super::items::ITEMS`] and routes through
//! [`super::handlers::dispatch`], both shared with the Linux backend so the
//! two cannot drift. Not compiled on Linux, which talks `StatusNotifierItem`
//! directly — see [`super::linux`].

use std::collections::HashMap;

use tauri::menu::MenuEvent;
use tauri::menu::{IsMenuItem, Menu, MenuItem, PredefinedMenuItem};
use tauri::tray::{MouseButton, MouseButtonState, TrayIcon, TrayIconBuilder, TrayIconEvent};
use tauri::{AppHandle, Wry};
use tracing::error;

use crate::daemon::DaemonSnapshot;
use crate::tray::icon;
use crate::tray::items::{ITEMS, Item, is_enabled};
use crate::tray::placement::Anchor;
use crate::tray::{handlers, ids, window};

/// The built menu items, keyed by id so `sync` can find them again.
///
/// A map rather than named fields because the item list lives in `items` now;
/// naming them here would be a second copy of the same structure.
pub(super) struct TrayMenu {
    items: HashMap<&'static str, MenuItem<Wry>>,
}

impl TrayMenu {
    /// Apply the daemon's state to every item that tracks it.
    fn apply(&self, status: &str, snapshot: &DaemonSnapshot) -> tauri::Result<()> {
        for (id, item) in &self.items {
            if *id == ids::STATUS {
                item.set_text(status)?;
            } else {
                item.set_enabled(is_enabled(id, snapshot))?;
            }
        }
        Ok(())
    }
}

/// Build the tray icon, its menu, and its click behaviour.
/// A built tray, kept alive for as long as the icon should exist.
pub(super) struct Handle {
    tray: TrayIcon,
    menu: TrayMenu,
}

/// Apply the daemon's state to the icon, tooltip and menu.
pub(super) async fn sync(handle: &Handle, snapshot: &DaemonSnapshot) -> Result<(), String> {
    let visual = icon::derive(snapshot);
    let image =
        icon::for_state(visual.state).map_err(|e| format!("Failed to decode tray icon: {e}"))?;

    handle
        .tray
        .set_icon(Some(image))
        .map_err(|e| format!("Failed to set tray icon: {e}"))?;
    handle
        .tray
        .set_tooltip(Some(&visual.status))
        .map_err(|e| format!("Failed to set tray tooltip: {e}"))?;

    handle
        .menu
        .apply(&visual.status, snapshot)
        .map_err(|e| format!("Failed to sync tray menu: {e}"))
}

/// Build the tray icon, its menu, and its click behaviour.
pub(super) fn build(app: &AppHandle) -> Result<Handle, String> {
    build_inner(app).map_err(|e| format!("Failed to build system tray: {e}"))
}

fn build_inner(app: &AppHandle) -> tauri::Result<Handle> {
    let mut items: HashMap<&'static str, MenuItem<Wry>> = HashMap::new();
    // Separators are positional and never looked up again, so they are kept
    // only to be borrowed into the menu below.
    let mut separators = Vec::new();
    let mut order: Vec<(Option<&'static str>, usize)> = Vec::new();

    // Nothing has been polled yet, and the default says so: the tray starts
    // out reading "not running" rather than asserting a state it cannot know.
    let initial = DaemonSnapshot::default();

    for item in ITEMS {
        match item {
            // Disabled: a label, not a command. It carries the endpoint into
            // the one surface every platform renders.
            Item::Status => {
                let status = MenuItem::with_id(
                    app,
                    ids::STATUS,
                    icon::derive(&initial).status,
                    is_enabled(ids::STATUS, &initial),
                    None::<&str>,
                )?;
                items.insert(ids::STATUS, status);
                order.push((Some(ids::STATUS), 0));
            }
            Item::Action { id, label } => {
                let built =
                    MenuItem::with_id(app, *id, label, is_enabled(id, &initial), None::<&str>)?;
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

    let initial_visual = icon::derive(&initial);
    let tray = TrayIconBuilder::with_id(icon::TRAY_ID)
        .icon(icon::for_state(initial_visual.state)?)
        // macOS recolours template images for light and dark menu bars. The
        // icons are pure black with the glyph carried by alpha, which is what
        // that mode expects.
        .icon_as_template(true)
        .tooltip(&initial_visual.status)
        .menu(&menu)
        // Left click opens the panel; the menu stays on right click, so the
        // common action does not require reading a list first.
        .show_menu_on_left_click(false)
        .on_menu_event(on_menu_event)
        .on_tray_icon_event(on_icon_event)
        .build(app)?;

    Ok(Handle {
        tray,
        menu: TrayMenu { items },
    })
}

/// Adapt a `muda` menu event to the shared router.
///
/// Lives here rather than in `handlers` because `MenuEvent` is this backend's
/// type; the Linux backend delivers ids straight to `dispatch`.
fn on_menu_event(app: &AppHandle, event: MenuEvent) {
    handlers::dispatch(app, event.id().as_ref());
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
