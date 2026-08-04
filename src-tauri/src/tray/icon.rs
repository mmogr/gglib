//! Deriving the tray's appearance from application state.
//!
//! Pure functions with no `AppHandle` and no I/O, so the mapping from "what
//! the app is doing" to "what the icon looks like" is directly testable
//! without a running Tauri application.

use tauri::image::Image;

/// Identifier the tray registers under.
pub const TRAY_ID: &str = "gglib";

/// Decoded idle icon (proxy stopped).
pub fn idle_icon() -> tauri::Result<Image<'static>> {
    Image::from_bytes(include_bytes!("../../icons/tray-idle.png"))
}

/// Decoded active icon (proxy serving).
pub fn active_icon() -> tauri::Result<Image<'static>> {
    Image::from_bytes(include_bytes!("../../icons/tray-active.png"))
}

/// The icon for a state, already decoded.
pub fn for_state(active: bool) -> tauri::Result<Image<'static>> {
    if active { active_icon() } else { idle_icon() }
}

/// How the tray should look for a given application state.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TrayVisual {
    /// Whether to use the active (proxy serving) icon rather than the idle one.
    pub active: bool,
    /// Hover text, and the label of the menu's status item.
    ///
    /// One string for both on purpose. It is the same sentence, and the menu
    /// is the only place Linux can show it: `set_tooltip` is a documented
    /// no-op there, so on a machine with no hover text the status item is
    /// what tells you where the endpoint is.
    pub status: String,
}

/// Derive the tray's appearance.
///
/// The icon tracks the **proxy**, not the app: the app being open says nothing
/// about whether anything is being served, and the proxy is the whole reason
/// the tray exists. An icon that lit up merely because gglib was running would
/// be answering a question nobody asked.
///
/// The status reports where the endpoint is, not what it is doing. Live
/// request counts belong to the panel, which subscribes to the proxy's
/// dashboard stream; the count is not available on this side without
/// threading the connection registry out of the running proxy.
#[must_use]
pub fn derive(proxy_running: bool, port: Option<u16>) -> TrayVisual {
    let status = if proxy_running {
        port.map_or_else(
            || "gglib — proxy running".to_owned(),
            |port| format!("gglib — proxy on :{port}"),
        )
    } else {
        "gglib — proxy stopped".to_owned()
    };

    TrayVisual {
        active: proxy_running,
        status,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stopped_proxy_uses_the_idle_icon() {
        let visual = derive(false, None);

        assert!(!visual.active);
        assert_eq!(visual.status, "gglib — proxy stopped");
    }

    /// The port is the thing a user came to the tray to find out, so it
    /// belongs in the status rather than only in the popover.
    #[test]
    fn running_proxy_reports_its_port() {
        let visual = derive(true, Some(8080));

        assert!(visual.active);
        assert_eq!(visual.status, "gglib — proxy on :8080");
    }

    /// The proxy can be up before its bound port has been recorded; say it is
    /// running rather than inventing a port number.
    #[test]
    fn running_without_a_known_port_still_reads_as_running() {
        let visual = derive(true, None);

        assert!(visual.active);
        assert_eq!(visual.status, "gglib — proxy running");
    }

    /// A port left over from a previous run must not make a stopped proxy
    /// look reachable.
    #[test]
    fn stopped_proxy_ignores_a_stale_port() {
        let visual = derive(false, Some(8080));

        assert!(!visual.active);
        assert_eq!(visual.status, "gglib — proxy stopped");
    }
}
