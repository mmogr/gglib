//! Tray menu item IDs for event handling.

/// Endpoint status header. Disabled, so it never raises a menu event — it has
/// an ID only because `MenuItem` requires one and `tray::sync` looks it up.
pub(super) const STATUS: &str = "tray_status";

/// Show the live proxy panel. Also the left-click action where the platform
/// reports one — see the module README on Linux's AppIndicator.
pub(super) const OPEN_PANEL: &str = "tray_open_panel";

pub(super) const START_PROXY: &str = "tray_start_proxy";
pub(super) const STOP_PROXY: &str = "tray_stop_proxy";
pub(super) const COPY_PROXY_URL: &str = "tray_copy_proxy_url";

/// Start the gglib daemon, when nothing is running.
pub(super) const START_SERVICE: &str = "tray_start_service";
/// Stop the gglib daemon: the proxy, every llama-server, the lot.
pub(super) const STOP_SERVICE: &str = "tray_stop_service";

/// Show the main window.
pub(super) const OPEN_MAIN: &str = "tray_open_main";
pub(super) const PREFERENCES: &str = "tray_preferences";

/// Quit the application.
pub(super) const QUIT: &str = "tray_quit";

#[cfg(test)]
mod tests {
    use super::*;

    /// Menu events are routed by ID, so a duplicate would silently fire the
    /// wrong handler — a copy-paste here could make Quit run on Start Proxy.
    #[test]
    fn every_menu_id_is_distinct() {
        let ids = [
            STATUS,
            OPEN_PANEL,
            START_PROXY,
            STOP_PROXY,
            COPY_PROXY_URL,
            START_SERVICE,
            STOP_SERVICE,
            OPEN_MAIN,
            PREFERENCES,
            QUIT,
        ];

        let mut unique = ids.to_vec();
        unique.sort_unstable();
        unique.dedup();

        assert_eq!(unique.len(), ids.len(), "duplicate tray menu id");
    }
}
