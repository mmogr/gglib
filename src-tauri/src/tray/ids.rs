//! Tray menu item IDs for event handling.

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
