//! The File menu: getting models into the library, and a new chat.

use tauri::{
    AppHandle, Wry,
    menu::{IsMenuItem, Submenu},
};

use super::{Entry, item};
use crate::menu::ids;

pub(super) const ENTRIES: &[Entry] = &[
    Entry {
        id: ids::GET_MODELS,
        label: "Get Models…",
        accelerator: Some("CmdOrCtrl+D"),
    },
    Entry {
        id: ids::ADD_FILE,
        label: "Add a GGUF File…",
        accelerator: Some("CmdOrCtrl+O"),
    },
    Entry {
        id: ids::NEW_CHAT,
        label: "New Chat",
        accelerator: Some("CmdOrCtrl+N"),
    },
];

pub(super) fn submenu(app: &AppHandle) -> tauri::Result<Submenu<Wry>> {
    let items = ENTRIES
        .iter()
        .map(|entry| item(app, entry))
        .collect::<tauri::Result<Vec<_>>>()?;
    let refs: Vec<&dyn IsMenuItem<Wry>> = items.iter().map(|i| i as &dyn IsMenuItem<Wry>).collect();

    Submenu::with_items(app, "File", true, &refs)
}
