//! The Endpoint menu: turning it on or off and copying its address, both
//! handled here in Rust, and the way to connect a client to it.

use tauri::{
    AppHandle, Wry,
    menu::{MenuItem, PredefinedMenuItem, Submenu},
};

use super::{Entry, item, stateful_items};
use crate::menu::{TURN_ON, ids};

/// Turn On / Off and Copy follow the endpoint's state; Connect does not.
pub(super) const ENTRIES: &[Entry] = &[
    Entry {
        id: ids::ENDPOINT_SWITCH,
        label: TURN_ON,
        accelerator: Some("CmdOrCtrl+P"),
    },
    Entry {
        id: ids::COPY_ENDPOINT_URL,
        label: "Copy Endpoint URL",
        accelerator: Some("CmdOrCtrl+Shift+C"),
    },
    Entry {
        id: ids::CONNECT_CLIENT,
        label: "Connect a Client…",
        accelerator: None,
    },
];

pub(super) fn submenu(
    app: &AppHandle,
    stateful: &mut Vec<(&'static str, MenuItem<Wry>)>,
) -> tauri::Result<Submenu<Wry>> {
    let [stateful_entries @ .., connect] = ENTRIES else {
        unreachable!()
    };
    let items = stateful_items(app, stateful_entries, stateful)?;
    let [switch, copy] = items.as_slice() else {
        unreachable!()
    };

    Submenu::with_items(
        app,
        "Endpoint",
        true,
        &[
            switch,
            copy,
            &PredefinedMenuItem::separator(app)?,
            &item(app, connect)?,
        ],
    )
}
