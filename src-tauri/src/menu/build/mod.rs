//! Menu construction: one file per submenu, assembled here in the order
//! macOS shows them — gglib, File, Edit, View, Model, Endpoint, Window, Help.

mod app;
mod endpoint;
mod file;
mod model;
mod standard;
mod view;

use tauri::{
    AppHandle, Wry,
    menu::{Menu, MenuItem},
};

use super::{AppMenu, MenuState, is_enabled, label};

/// One item of ours on a submenu: its id, its words and its key.
///
/// A table rather than calls, so the tests below can hold every id and every
/// key to being declared once without building a menu.
pub(super) struct Entry {
    pub id: &'static str,
    pub label: &'static str,
    pub accelerator: Option<&'static str>,
}

/// Build the complete application menu.
///
/// Returns both the Menu to attach to the app and the `AppMenu` holding the
/// items whose state changes, for `sync_state` to update.
pub(crate) fn build_app_menu(app: &AppHandle) -> Result<(Menu<Wry>, AppMenu), tauri::Error> {
    let mut stateful = Vec::new();

    let menu = Menu::with_items(
        app,
        &[
            &app::submenu(app)?,
            &file::submenu(app)?,
            &standard::edit(app)?,
            &view::submenu(app, &mut stateful)?,
            &model::submenu(app, &mut stateful)?,
            &endpoint::submenu(app, &mut stateful)?,
            &standard::window(app)?,
            &standard::help(app)?,
        ],
    )?;

    Ok((menu, AppMenu { stateful }))
}

/// Build one entry as it should look before the first poll: the state every
/// rule reads as "nothing known", so nothing that needs a daemon or a model is
/// offered yet.
fn item(app: &AppHandle, entry: &Entry) -> tauri::Result<MenuItem<Wry>> {
    let initial = MenuState::default();
    MenuItem::with_id(
        app,
        entry.id,
        label(entry.id, initial).unwrap_or(entry.label),
        is_enabled(entry.id, initial),
        entry.accelerator,
    )
}

/// Build a submenu's entries, keeping each beside its id in `stateful` for
/// `sync_state`.
fn stateful_items(
    app: &AppHandle,
    entries: &[Entry],
    stateful: &mut Vec<(&'static str, MenuItem<Wry>)>,
) -> tauri::Result<Vec<MenuItem<Wry>>> {
    let items = entries
        .iter()
        .map(|entry| item(app, entry))
        .collect::<tauri::Result<Vec<_>>>()?;
    stateful.extend(entries.iter().map(|e| e.id).zip(items.iter().cloned()));
    Ok(items)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::menu::ids;

    fn entries() -> impl Iterator<Item = &'static Entry> {
        [
            app::ENTRIES,
            file::ENTRIES,
            view::ENTRIES,
            model::ENTRIES,
            endpoint::ENTRIES,
            standard::HELP,
        ]
        .into_iter()
        .flatten()
    }

    /// Every declared id is on the menu bar, once: an id with no item is a
    /// handler nothing reaches, and an item twice is two names for one thing.
    #[test]
    fn every_id_is_on_the_menu_once() {
        for id in ids::ALL {
            let count = entries().filter(|e| e.id == id).count();
            assert_eq!(count, 1, "{id} is on the menu {count} times");
        }
        assert_eq!(entries().count(), ids::ALL.len());
    }

    /// One key, one item: two items on ⌘1 and macOS fires whichever it finds
    /// first.
    #[test]
    fn every_key_is_bound_once() {
        let mut keys: Vec<&str> = entries().filter_map(|e| e.accelerator).collect();
        let bound = keys.len();
        keys.sort_unstable();
        keys.dedup();
        assert_eq!(keys.len(), bound, "a key is bound twice");
    }

    /// ⌘1–⌘4 are the three pillars in pitch order, then Chat.
    #[test]
    fn the_number_keys_are_the_places_in_pitch_order() {
        let key = |id: &str| entries().find(|e| e.id == id).and_then(|e| e.accelerator);

        assert_eq!(key(ids::SHOW_LIBRARY), Some("CmdOrCtrl+1"));
        assert_eq!(key(ids::SHOW_ENDPOINT), Some("CmdOrCtrl+2"));
        assert_eq!(key(ids::SHOW_DEVICES), Some("CmdOrCtrl+3"));
        assert_eq!(key(ids::SHOW_CHAT), Some("CmdOrCtrl+4"));
    }
}
