//! The Endpoint menu: turning it on or off and copying its address, both
//! handled here in Rust, and the way to connect a client to it. Copy and
//! Connect are offered only while the endpoint is on.

use tauri::{
    AppHandle, Wry,
    menu::{MenuItem, PredefinedMenuItem, Submenu},
};

use super::{Entry, stateful_items};
use crate::menu::{TURN_ON, ids};

/// All three follow the endpoint's state.
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
    let items = stateful_items(app, ENTRIES, stateful)?;
    let [switch, copy, connect] = items.as_slice() else {
        unreachable!()
    };

    Submenu::with_items(
        app,
        "Endpoint",
        true,
        &[switch, copy, &PredefinedMenuItem::separator(app)?, connect],
    )
}
