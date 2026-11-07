//! The Model menu: what can be done to the model in the inspector. Every item
//! is disabled with no model there.

use tauri::{
    AppHandle, Wry,
    menu::{MenuItem, PredefinedMenuItem, Submenu},
};

use super::{Entry, stateful_items};
use crate::menu::ids;

pub(super) const ENTRIES: &[Entry] = &[
    Entry {
        id: ids::LOAD,
        label: "Load",
        accelerator: Some("CmdOrCtrl+Return"),
    },
    Entry {
        id: ids::LOAD_WITH_OPTIONS,
        label: "Load with Options…",
        accelerator: Some("Alt+CmdOrCtrl+Return"),
    },
    Entry {
        id: ids::UNLOAD,
        label: "Unload",
        accelerator: Some("CmdOrCtrl+."),
    },
    Entry {
        id: ids::REMOVE,
        label: "Remove from Library…",
        accelerator: Some("CmdOrCtrl+Backspace"),
    },
];

pub(super) fn submenu(
    app: &AppHandle,
    stateful: &mut Vec<(&'static str, MenuItem<Wry>)>,
) -> tauri::Result<Submenu<Wry>> {
    let items = stateful_items(app, ENTRIES, stateful)?;
    let [load, load_with_options, unload, remove] = items.as_slice() else {
        unreachable!()
    };

    Submenu::with_items(
        app,
        "Model",
        true,
        &[
            load,
            load_with_options,
            unload,
            &PredefinedMenuItem::separator(app)?,
            remove,
        ],
    )
}
