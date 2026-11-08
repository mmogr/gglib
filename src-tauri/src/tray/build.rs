//! Tray construction for the `muda` backend, used on macOS and Windows.
//!
//! Renders [`super::items::ITEMS`] and routes through
//! [`super::handlers::dispatch`], both shared with the Linux backend so the
//! two cannot drift. Not compiled on Linux, which talks `StatusNotifierItem`
//! directly — see [`super::linux`].

use tauri::menu::MenuEvent;
use tauri::menu::{IsMenuItem, Menu, MenuItem, PredefinedMenuItem};
use tauri::tray::{MouseButton, MouseButtonState, TrayIcon, TrayIconBuilder, TrayIconEvent};
use tauri::{AppHandle, Wry};
use tracing::error;

use crate::daemon::DaemonSnapshot;
use crate::tray::icon;
use crate::tray::items::{ITEMS, Item, is_enabled, label};
use crate::tray::placement::Anchor;
use crate::tray::{handlers, ids, window};

/// The built menu items, each beside the entry it draws, so `sync` can
/// redraw them.
///
/// Kept beside their [`Item`] rather than in named fields because the item
/// list lives in `items`; naming them here would be a second copy of it.
pub(super) struct TrayMenu {
    items: Vec<(Item, MenuItem<Wry>)>,
}

impl TrayMenu {
    /// Apply the daemon's state to every item: its text (the status line and
    /// the switches change theirs) and whether it is enabled.
    fn apply(&self, status: &str, snapshot: &DaemonSnapshot) -> tauri::Result<()> {
        for (item, built) in &self.items {
            if let Some(text) = label(item, status, snapshot) {
                built.set_text(text)?;
            }
            if let Item::Action { id, .. } | Item::Switch { id, .. } = item {
                built.set_enabled(is_enabled(id, snapshot))?;
            }
        }
        Ok(())
    }
}

/// A built tray, kept alive for as long as the icon should exist.
pub(super) struct Handle {
    tray: TrayIcon,
    menu: TrayMenu,
}

/// Apply the daemon's state to the icon, tooltip and menu.
#[allow(
    clippy::unused_async,
    reason = "one signature for both tray backends, and the Linux one awaits"
)]
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
    let mut items: Vec<(Item, MenuItem<Wry>)> = Vec::new();
    // Separators are positional and never looked up again, so they are kept
    // only to be borrowed into the menu below.
    let mut separators = Vec::new();
    let mut order: Vec<(bool, usize)> = Vec::new();

    // Nothing has been polled yet, and the default says so: the tray starts
    // out reading "not running" rather than asserting a state it cannot know.
    let initial = DaemonSnapshot::default();
    let status = icon::derive(&initial).status;

    for item in ITEMS {
        let (id, enabled) = match *item {
            Item::Separator => {
                separators.push(PredefinedMenuItem::separator(app)?);
                order.push((false, separators.len() - 1));
                continue;
            }
            // Disabled: a label, not a command. It carries the status into
            // the one surface every platform renders.
            Item::Status => (ids::STATUS, is_enabled(ids::STATUS, &initial)),
            Item::Action { id, .. } | Item::Switch { id, .. } => (id, is_enabled(id, &initial)),
        };
        let text = label(item, &status, &initial).unwrap_or_default();
        items.push((
            *item,
            MenuItem::with_id(app, id, text, enabled, None::<&str>)?,
        ));
        order.push((true, items.len() - 1));
    }

    let refs: Vec<&dyn IsMenuItem<Wry>> = order
        .iter()
        .map(|&(is_item, index)| {
            if is_item {
                &items[index].1 as &dyn IsMenuItem<Wry>
            } else {
                &separators[index] as &dyn IsMenuItem<Wry>
            }
        })
        .collect();

    let menu = Menu::with_items(app, &refs)?;
    drop(refs);

    let initial_visual = icon::derive(&initial);
    let tray = TrayIconBuilder::with_id(icon::TRAY_ID)
        .icon(icon::for_state(initial_visual.state)?)
        // macOS recolours template images for light and dark menu bars. The
        // icons are pure black with the glyph carried by alpha, which is what
        // that mode expects. Windows ignores the flag; `for_state` has
        // already tinted the glyph there.
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
#[allow(
    clippy::needless_pass_by_value,
    reason = "Tauri hands a menu event to its handler by value"
)]
fn on_menu_event(app: &AppHandle, event: MenuEvent) {
    handlers::dispatch(app, event.id().as_ref());
}

/// Open the panel on a completed left click.
///
/// Only `Up` is acted on so the panel does not appear under a button that is
/// still held down. Never fires on Linux, which is why that platform uses a
/// different backend entirely — see the module README.
#[allow(
    clippy::needless_pass_by_value,
    reason = "Tauri hands a tray icon event to its handler by value"
)]
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
