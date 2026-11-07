//! Menu item IDs for event handling, submenu by submenu.

// gglib menu
pub(crate) const SETTINGS: &str = "settings";

// File menu
pub(crate) const GET_MODELS: &str = "get_models";
pub(crate) const ADD_FILE: &str = "add_file";
pub(crate) const NEW_CHAT: &str = "new_chat";

// View menu
pub(crate) const SHOW_LIBRARY: &str = "show_library";
pub(crate) const SHOW_ENDPOINT: &str = "show_endpoint";
pub(crate) const SHOW_DEVICES: &str = "show_devices";
pub(crate) const SHOW_CHAT: &str = "show_chat";
pub(crate) const SHOW_DIAGNOSTICS: &str = "show_diagnostics";
pub(crate) const HIDE_INSPECTOR: &str = "hide_inspector";

// Model menu: the inspected model
pub(crate) const LOAD: &str = "load";
pub(crate) const LOAD_WITH_OPTIONS: &str = "load_with_options";
pub(crate) const UNLOAD: &str = "unload";
pub(crate) const REMOVE: &str = "remove";

// Endpoint menu
/// Turn On / Off: one item, its verb from the snapshot.
pub(crate) const ENDPOINT_SWITCH: &str = "endpoint_switch";
pub(crate) const COPY_ENDPOINT_URL: &str = "copy_endpoint_url";
pub(crate) const CONNECT_CLIENT: &str = "connect_client";

// Help menu
pub(crate) const OPEN_DOCS: &str = "open_docs";

/// Every id, for the tests that hold them distinct and routed.
#[cfg(test)]
pub(crate) const ALL: [&str; 18] = [
    SETTINGS,
    GET_MODELS,
    ADD_FILE,
    NEW_CHAT,
    SHOW_LIBRARY,
    SHOW_ENDPOINT,
    SHOW_DEVICES,
    SHOW_CHAT,
    SHOW_DIAGNOSTICS,
    HIDE_INSPECTOR,
    LOAD,
    LOAD_WITH_OPTIONS,
    UNLOAD,
    REMOVE,
    ENDPOINT_SWITCH,
    COPY_ENDPOINT_URL,
    CONNECT_CLIENT,
    OPEN_DOCS,
];

#[cfg(test)]
mod tests {
    use super::*;

    /// Menu events are routed by id, so a duplicate would fire the wrong
    /// handler. The tray's ids are prefixed `tray_`, so none can collide with
    /// these either: both menus deliver to the same app.
    #[test]
    fn every_menu_id_is_distinct_and_none_is_the_trays() {
        let mut unique = ALL.to_vec();
        unique.sort_unstable();
        unique.dedup();

        assert_eq!(unique.len(), ALL.len(), "duplicate menu id");
        assert!(ALL.iter().all(|id| !id.starts_with("tray_")));
    }
}
