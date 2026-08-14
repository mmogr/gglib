//! Deriving the tray's appearance from the daemon's state.
//!
//! Pure functions with no `AppHandle` and no I/O, so the mapping from "what
//! gglib is doing" to "what the icon looks like" is directly testable without
//! a running Tauri application.

use tauri::image::Image;

use crate::daemon::DaemonSnapshot;

/// Identifier the tray registers under.
pub(super) const TRAY_ID: &str = "gglib";

/// How much of the idle icon's opacity the offline icon keeps.
const OFFLINE_OPACITY_PERCENT: u16 = 35;

/// What the tray icon is saying.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum TrayState {
    /// No daemon answering: gglib is not running on this machine.
    Offline,
    /// A daemon is up but consuming nothing — no proxy, no resident models.
    Idle,
    /// Something is being served or held in memory.
    Active,
}

/// Decoded idle icon: a daemon that is up and doing nothing.
pub(super) fn idle_icon() -> tauri::Result<Image<'static>> {
    Image::from_bytes(include_bytes!("../../icons/tray-idle.png"))
}

/// Decoded active icon: something is being served or resident.
pub(super) fn active_icon() -> tauri::Result<Image<'static>> {
    Image::from_bytes(include_bytes!("../../icons/tray-active.png"))
}

/// The idle ring, faded, for when there is no daemon at all.
///
/// Derived from the idle icon rather than shipped as a third asset. It is the
/// same glyph, so a separate file could only drift from it, and fading needs
/// no image dependency — the same reasoning as the ARGB byte swap in
/// [`super::linux`]. Both icons carry their glyph entirely in the alpha
/// channel, which is what macOS template mode requires, so scaling alpha is
/// exactly "draw the same thing fainter" on every backend.
pub(super) fn offline_icon() -> tauri::Result<Image<'static>> {
    let idle = idle_icon()?;
    let (width, height) = (idle.width(), idle.height());

    let faded = idle
        .rgba()
        .chunks_exact(4)
        .flat_map(|px| [px[0], px[1], px[2], fade(px[3])])
        .collect();

    Ok(Image::new_owned(faded, width, height))
}

/// One alpha byte, dimmed.
const fn fade(alpha: u8) -> u8 {
    // Cannot overflow: the largest product is 255 * 35, and dividing by 100
    // lands well inside u8.
    #[allow(
        clippy::cast_possible_truncation,
        reason = "255 * OFFLINE_OPACITY_PERCENT / 100 is at most 89"
    )]
    {
        (alpha as u16 * OFFLINE_OPACITY_PERCENT / 100) as u8
    }
}

/// The icon for a state, already decoded.
pub(super) fn for_state(state: TrayState) -> tauri::Result<Image<'static>> {
    match state {
        TrayState::Offline => offline_icon(),
        TrayState::Idle => idle_icon(),
        TrayState::Active => active_icon(),
    }
}

/// How the tray should look for a given daemon state.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct TrayVisual {
    /// Which icon to show.
    pub state: TrayState,
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
/// The icon tracks **consumption, not existence**. Not the app — an icon that
/// lit up merely because a window was open would answer a question nobody
/// asked — but not a live daemon either: nearly every CLI command leaves one
/// running, so an icon lit by that would be lit permanently and mean nothing.
/// What it reports is whether gglib is doing something to this machine right
/// now, which is true of a resident model whether or not the proxy is up.
#[must_use]
pub(super) fn derive(snap: &DaemonSnapshot) -> TrayVisual {
    TrayVisual {
        state: state_of(snap),
        status: status_of(snap),
    }
}

/// Which icon a snapshot calls for.
fn state_of(snap: &DaemonSnapshot) -> TrayState {
    match (snap.reachable, snap.is_active()) {
        (false, _) => TrayState::Offline,
        (true, false) => TrayState::Idle,
        (true, true) => TrayState::Active,
    }
}

/// The one sentence the tooltip and the menu header share.
fn status_of(snap: &DaemonSnapshot) -> String {
    if !snap.reachable {
        return "gglib — not running".to_owned();
    }

    match (proxy_phrase(snap), residency_phrase(snap)) {
        (Some(proxy), Some(residency)) => format!("gglib — {proxy} · {residency}"),
        (Some(proxy), None) => format!("gglib — {proxy}"),
        (None, Some(residency)) => format!("gglib — {residency}, proxy off"),
        (None, None) => "gglib — idle".to_owned(),
    }
}

/// Where the endpoint is, if there is one.
///
/// The port is the thing a user came to the tray to find out, but the proxy
/// can be up before its bound port has been recorded — say it is running
/// rather than inventing a number.
fn proxy_phrase(snap: &DaemonSnapshot) -> Option<String> {
    if !snap.proxy_running {
        return None;
    }

    Some(snap.proxy_port.map_or_else(
        || "proxy running".to_owned(),
        |port| format!("proxy on :{port}"),
    ))
}

/// How much is being held in memory, if anything.
fn residency_phrase(snap: &DaemonSnapshot) -> Option<String> {
    match snap.resident.len() {
        0 => None,
        1 => Some("1 model resident".to_owned()),
        n => Some(format!("{n} models resident")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    /// Build a snapshot the way the watcher does, so these tests exercise the
    /// same construction the app runs.
    fn snapshot(proxy: serde_json::Value, servers: serde_json::Value) -> DaemonSnapshot {
        DaemonSnapshot::from_responses(&proxy, &servers)
    }

    #[test]
    fn no_daemon_reads_as_not_running() {
        let visual = derive(&DaemonSnapshot::default());

        assert_eq!(visual.state, TrayState::Offline);
        assert_eq!(visual.status, "gglib — not running");
    }

    /// A daemon that is up but consuming nothing is a real state, and a
    /// distinct one: "idle" is not "not running".
    #[test]
    fn a_daemon_doing_nothing_is_idle() {
        let visual = derive(&snapshot(json!({"running": false}), json!([])));

        assert_eq!(visual.state, TrayState::Idle);
        assert_eq!(visual.status, "gglib — idle");
    }

    #[test]
    fn a_running_proxy_reports_its_port() {
        let visual = derive(&snapshot(json!({"running": true, "port": 8080}), json!([])));

        assert_eq!(visual.state, TrayState::Active);
        assert_eq!(visual.status, "gglib — proxy on :8080");
    }

    /// The case the proxy-only tray was blind to: VRAM held with nothing
    /// listening. The icon has to be lit, because the machine is in use.
    #[test]
    fn a_resident_model_lights_the_icon_with_the_proxy_off() {
        let visual = derive(&snapshot(
            json!({"running": false}),
            json!([{"model_id": 1}]),
        ));

        assert_eq!(visual.state, TrayState::Active);
        assert_eq!(visual.status, "gglib — 1 model resident, proxy off");
    }

    #[test]
    fn serving_and_resident_are_reported_together() {
        let visual = derive(&snapshot(
            json!({"running": true, "port": 8080}),
            json!([{"model_id": 1}, {"model_id": 2}]),
        ));

        assert_eq!(visual.status, "gglib — proxy on :8080 · 2 models resident");
    }

    /// The proxy can be up before its bound port has been recorded; say it is
    /// running rather than inventing a port number.
    #[test]
    fn running_without_a_known_port_still_reads_as_running() {
        let visual = derive(&snapshot(json!({"running": true}), json!([])));

        assert_eq!(visual.state, TrayState::Active);
        assert_eq!(visual.status, "gglib — proxy running");
    }

    /// An unreachable daemon says so and nothing else — no leftover endpoint
    /// from before it went away.
    #[test]
    fn an_unreachable_daemon_reports_no_endpoint() {
        let stale = DaemonSnapshot {
            reachable: false,
            proxy_running: true,
            proxy_port: Some(8080),
            resident: vec![1],
        };

        assert_eq!(derive(&stale).state, TrayState::Offline);
        assert!(!derive(&stale).status.contains("8080"));
    }

    /// The offline icon is derived at runtime, so it has to actually come out
    /// the same shape as the one it is derived from.
    #[test]
    fn the_offline_icon_matches_the_idle_icon_dimmed() {
        let idle = idle_icon().expect("idle icon decodes");
        let offline = offline_icon().expect("offline icon derives");

        assert_eq!(offline.width(), idle.width());
        assert_eq!(offline.height(), idle.height());

        let brightest_idle = idle.rgba().chunks_exact(4).map(|px| px[3]).max();
        let brightest_offline = offline.rgba().chunks_exact(4).map(|px| px[3]).max();
        assert!(
            brightest_offline < brightest_idle,
            "the offline icon must be fainter than the idle one"
        );
        assert!(
            brightest_offline > Some(0),
            "but still visible, or there is no icon to click"
        );
    }
}
