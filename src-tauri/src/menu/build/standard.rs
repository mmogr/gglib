//! The menus every Mac app has: Edit, Window, and Help with gglib's
//! documentation.

use tauri::{
    AppHandle, Wry,
    menu::{PredefinedMenuItem, Submenu},
};

use super::{Entry, item};
use crate::menu::ids;

pub(super) const HELP: &[Entry] = &[Entry {
    id: ids::OPEN_DOCS,
    label: "gglib Documentation",
    accelerator: None,
}];

/// Edit: the standard text items, which the webview's fields answer.
pub(super) fn edit(app: &AppHandle) -> tauri::Result<Submenu<Wry>> {
    Submenu::with_items(
        app,
        "Edit",
        true,
        &[
            &PredefinedMenuItem::undo(app, None)?,
            &PredefinedMenuItem::redo(app, None)?,
            &PredefinedMenuItem::separator(app)?,
            &PredefinedMenuItem::cut(app, None)?,
            &PredefinedMenuItem::copy(app, None)?,
            &PredefinedMenuItem::paste(app, None)?,
            &PredefinedMenuItem::separator(app)?,
            &PredefinedMenuItem::select_all(app, None)?,
        ],
    )
}

pub(super) fn window(app: &AppHandle) -> tauri::Result<Submenu<Wry>> {
    Submenu::with_items(
        app,
        "Window",
        true,
        &[
            &PredefinedMenuItem::minimize(app, Some("Minimize"))?,
            &PredefinedMenuItem::maximize(app, Some("Zoom"))?,
            &PredefinedMenuItem::separator(app)?,
            &PredefinedMenuItem::close_window(app, Some("Close Window"))?,
        ],
    )
}

pub(super) fn help(app: &AppHandle) -> tauri::Result<Submenu<Wry>> {
    let [docs] = HELP else { unreachable!() };
    Submenu::with_items(app, "Help", true, &[&item(app, docs)?])
}
