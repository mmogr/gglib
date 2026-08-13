//! Asking before taking a running service away.
//!
//! Two menu entries can end an endpoint other programs are pointed at — Quit
//! and Stop gglib Service — so the sentence describing what is about to be
//! lost is derived once, here, from the same snapshot the icon is drawn from.
//! Deriving it rather than hardcoding it is the point: the warning used to say
//! quitting stopped the proxy, which stopped being true when the daemon took
//! ownership of the runtime, and nothing made the two disagree loudly.

use tauri::AppHandle;
use tauri_plugin_dialog::{DialogExt, MessageDialogButtons, MessageDialogKind};

use crate::daemon::DaemonSnapshot;

/// What a teardown would take away, or `None` when nothing is at stake.
///
/// Pure, so every phrasing below is testable without a dialog. `None` is the
/// signal not to ask at all: interrupting someone to confirm the shutdown of a
/// daemon that is serving nothing and holding nothing is noise.
#[must_use]
pub fn at_stake(snap: &DaemonSnapshot) -> Option<String> {
    let proxy = snap
        .proxy_port
        .map(|port| format!("the proxy on :{port}"))
        .or_else(|| snap.proxy_running.then(|| "the proxy".to_owned()));

    let resident = match snap.resident.len() {
        0 => None,
        1 => Some("1 resident model".to_owned()),
        n => Some(format!("{n} resident models")),
    };

    match (proxy, resident) {
        (Some(proxy), Some(resident)) => Some(format!("{proxy} and {resident}")),
        (Some(proxy), None) => Some(proxy),
        (None, Some(resident)) => Some(resident),
        (None, None) => None,
    }
}

/// Ask before quitting, when quitting would end something.
///
/// Only asks when this app's exit actually takes the daemon with it. Against
/// an adopted daemon there is nothing to warn about: it keeps serving, which
/// is the whole reason it is left alone.
pub fn quit(app: &AppHandle, snap: &DaemonSnapshot, ends_with_the_app: bool) -> bool {
    if !ends_with_the_app {
        return true;
    }

    let Some(at_stake) = at_stake(snap) else {
        return true;
    };

    ask(
        app,
        "Quit gglib?",
        &format!("Quitting stops {at_stake}. Any client using the endpoint will lose it."),
        "Quit",
    )
}

/// Ask before stopping the daemon from the tray.
pub fn stop_service(app: &AppHandle, snap: &DaemonSnapshot) -> bool {
    let Some(at_stake) = at_stake(snap) else {
        return true;
    };

    ask(
        app,
        "Stop the gglib service?",
        &format!("This stops {at_stake}. Any client using the endpoint will lose it."),
        "Stop",
    )
}

/// Tell the user an action they asked for did not happen.
///
/// The tray has no toast host and, with close-to-tray on, often no window
/// either — so a failed menu item used to be an `error!` in a log nobody was
/// reading and a menu that appeared to do nothing at all. The daemon's own
/// messages are worth showing verbatim: "Port 8080 is already in use … Stop
/// it, or change the proxy port in Settings" is the whole answer.
pub fn report_failure(app: &AppHandle, title: &str, detail: &str) {
    app.dialog()
        .message(detail)
        .title(title)
        .kind(MessageDialogKind::Error)
        .blocking_show();
}

/// Put one warning on screen and wait for an answer.
fn ask(app: &AppHandle, title: &str, message: &str, confirm: &str) -> bool {
    app.dialog()
        .message(message)
        .title(title)
        .kind(MessageDialogKind::Warning)
        .buttons(MessageDialogButtons::OkCancelCustom(
            confirm.to_owned(),
            "Cancel".to_owned(),
        ))
        .blocking_show()
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn snapshot(proxy: serde_json::Value, servers: serde_json::Value) -> DaemonSnapshot {
        DaemonSnapshot::from_responses(&proxy, &servers)
    }

    /// Nothing serving and nothing held means nothing to warn about, and the
    /// caller reads `None` as "do not interrupt".
    #[test]
    fn an_idle_daemon_has_nothing_at_stake() {
        assert_eq!(
            at_stake(&snapshot(json!({"running": false}), json!([]))),
            None
        );
        assert_eq!(at_stake(&DaemonSnapshot::default()), None);
    }

    /// The port is the detail that tells someone whether the endpoint about to
    /// go away is the one their editor is pointed at.
    #[test]
    fn a_serving_proxy_names_its_port() {
        let snap = snapshot(json!({"running": true, "port": 8080}), json!([]));

        assert_eq!(at_stake(&snap).as_deref(), Some("the proxy on :8080"));
    }

    /// A model held in VRAM is worth naming even with nothing listening — it
    /// is the case the tray could not previously see at all.
    #[test]
    fn resident_models_count_on_their_own() {
        let snap = snapshot(json!({"running": false}), json!([{"model_id": 1}]));

        assert_eq!(at_stake(&snap).as_deref(), Some("1 resident model"));
    }

    #[test]
    fn a_proxy_and_models_are_named_together() {
        let snap = snapshot(
            json!({"running": true, "port": 9000}),
            json!([{"model_id": 1}, {"model_id": 2}]),
        );

        assert_eq!(
            at_stake(&snap).as_deref(),
            Some("the proxy on :9000 and 2 resident models")
        );
    }

    /// The proxy can be up before its port is known; say so rather than
    /// leaving it out of the warning entirely.
    #[test]
    fn a_proxy_without_a_known_port_is_still_at_stake() {
        let snap = snapshot(json!({"running": true}), json!([]));

        assert_eq!(at_stake(&snap).as_deref(), Some("the proxy"));
    }
}
