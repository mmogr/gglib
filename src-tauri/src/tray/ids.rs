//! Tray menu item IDs for event handling.

/// The status line. Disabled, so it never raises a menu event — it has an ID
/// only because `MenuItem` requires one and `tray::sync` looks it up.
pub(super) const STATUS: &str = "tray_status";

/// Show the tray panel. On the menu only where the tray has no left click
/// (Linux, see the module README); elsewhere the click is the way in, and the
/// id is still routed so the panel has one name everywhere.
pub(super) const SHOW_PANEL: &str = "tray_show_panel";

pub(super) const COPY_ENDPOINT_URL: &str = "tray_copy_endpoint_url";
/// Turn Endpoint On / Off: one item, its verb from the snapshot.
pub(super) const ENDPOINT_SWITCH: &str = "tray_endpoint_switch";

/// Show the main window.
pub(super) const OPEN_MAIN: &str = "tray_open_main";
pub(super) const SETTINGS: &str = "tray_settings";

/// Start gglib / Stop gglib…: one item, its verb from the snapshot. Stopping
/// ends the daemon: the endpoint, every loaded model, the lot.
pub(super) const SERVICE_SWITCH: &str = "tray_service_switch";

/// Quit the application.
pub(super) const QUIT: &str = "tray_quit";

/// Every id, for the tests that hold them distinct and routed.
#[cfg(test)]
pub(super) const ALL: [&str; 8] = [
    STATUS,
    SHOW_PANEL,
    COPY_ENDPOINT_URL,
    ENDPOINT_SWITCH,
    OPEN_MAIN,
    SETTINGS,
    SERVICE_SWITCH,
    QUIT,
];

#[cfg(test)]
mod tests {
    use super::*;

    /// Menu events are routed by ID, so a duplicate would silently fire the
    /// wrong handler — a copy-paste here could make Quit run on Turn On.
    #[test]
    fn every_menu_id_is_distinct() {
        let mut unique = ALL.to_vec();
        unique.sort_unstable();
        unique.dedup();

        assert_eq!(unique.len(), ALL.len(), "duplicate tray menu id");
    }
}
