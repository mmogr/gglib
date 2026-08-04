//! Tray menu item IDs for event handling.

/// Endpoint status header. Disabled, so it never raises a menu event — it has
/// an ID only because `MenuItem` requires one and `tray::sync` looks it up.
pub const STATUS: &str = "tray_status";

/// Show the live proxy panel. Also the left-click action where the platform
/// reports one — see the module README on Linux's AppIndicator.
pub const OPEN_PANEL: &str = "tray_open_panel";

pub const START_PROXY: &str = "tray_start_proxy";
pub const STOP_PROXY: &str = "tray_stop_proxy";
pub const COPY_PROXY_URL: &str = "tray_copy_proxy_url";

/// Show the main window.
pub const OPEN_MAIN: &str = "tray_open_main";
pub const PREFERENCES: &str = "tray_preferences";

/// Quit the application, stopping the proxy with it.
pub const QUIT: &str = "tray_quit";

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
