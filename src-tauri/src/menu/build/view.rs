//! The View menu: the places, in the sidebar's order, then the inspector and
//! full screen. Hide Inspector is offered only with a model inspected.

use tauri::{
    AppHandle, Wry,
    menu::{MenuItem, PredefinedMenuItem, Submenu},
};

use super::{Entry, item, stateful_items};
use crate::menu::ids;

pub(super) const ENTRIES: &[Entry] = &[
    // ⌘1–⌘3: the three pillars in pitch order; ⌘4 the chat.
    Entry {
        id: ids::SHOW_LIBRARY,
        label: "Library",
        accelerator: Some("CmdOrCtrl+1"),
    },
    Entry {
        id: ids::SHOW_ENDPOINT,
        label: "Endpoint",
        accelerator: Some("CmdOrCtrl+2"),
    },
    Entry {
        id: ids::SHOW_DEVICES,
        label: "Devices",
        accelerator: Some("CmdOrCtrl+3"),
    },
    Entry {
        id: ids::SHOW_CHAT,
        label: "Chat",
        accelerator: Some("CmdOrCtrl+4"),
    },
    Entry {
        id: ids::SHOW_DIAGNOSTICS,
        label: "Diagnostics",
        accelerator: Some("Alt+CmdOrCtrl+D"),
    },
    Entry {
        id: ids::HIDE_INSPECTOR,
        label: "Hide Inspector",
        accelerator: Some("CmdOrCtrl+I"),
    },
];

pub(super) fn submenu(
    app: &AppHandle,
    stateful: &mut Vec<(&'static str, MenuItem<Wry>)>,
) -> tauri::Result<Submenu<Wry>> {
    let [library, endpoint, devices, chat, diagnostics, hide] = ENTRIES else {
        unreachable!()
    };
    let hide = stateful_items(app, std::slice::from_ref(hide), stateful)?;

    Submenu::with_items(
        app,
        "View",
        true,
        &[
            &item(app, library)?,
            &item(app, endpoint)?,
            &item(app, devices)?,
            &item(app, chat)?,
            &PredefinedMenuItem::separator(app)?,
            &item(app, diagnostics)?,
            &PredefinedMenuItem::separator(app)?,
            &hide[0],
            &PredefinedMenuItem::fullscreen(app, Some("Enter Full Screen"))?,
        ],
    )
}
