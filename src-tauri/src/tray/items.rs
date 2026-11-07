//! What is on the tray menu, and when each entry is available.
//!
//! Two backends render this list — `muda` on macOS and Windows, `ksni` on
//! Linux — and they share no menu model, so the list itself is the only thing
//! that can keep them in step. Order, labels and the enabled rule live here
//! once; each backend only knows how to draw an [`Item`].

use crate::daemon::DaemonSnapshot;
use crate::tray::ids;

/// One entry on the tray menu.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Item {
    /// The status line. Always disabled — a label, not a command — and its
    /// text is `icon::derive`'s, the tooltip's sentence.
    Status,
    /// A clickable entry, routed by `id` through [`super::handlers::dispatch`].
    Action {
        id: &'static str,
        label: &'static str,
    },
    /// One clickable entry whose verb follows the state: `on` is its label
    /// while the thing it switches is on (so it says how to turn it off),
    /// `off` while it is off. See [`label`].
    Switch {
        id: &'static str,
        on: &'static str,
        off: &'static str,
    },
    Separator,
}

/// The menu, in order: what the endpoint is doing and the two things done to
/// it; the window; the service and the app.
///
/// Every action appears here, including the ones that also have a click
/// gesture: the gesture is a shortcut, never the only route to a feature.
/// The panel is the exception that proves it: where the icon has a left click
/// the click opens it, and only Linux, whose tray hosts may never send one,
/// lists it.
pub(super) const ITEMS: &[Item] = &[
    Item::Status,
    #[cfg(target_os = "linux")]
    Item::Action {
        id: ids::SHOW_PANEL,
        label: "Show panel",
    },
    Item::Action {
        id: ids::COPY_ENDPOINT_URL,
        label: "Copy Endpoint URL",
    },
    Item::Switch {
        id: ids::ENDPOINT_SWITCH,
        on: "Turn Endpoint Off",
        off: "Turn Endpoint On",
    },
    Item::Separator,
    Item::Action {
        id: ids::OPEN_MAIN,
        label: "Open gglib",
    },
    Item::Action {
        id: ids::SETTINGS,
        label: "Settings…",
    },
    Item::Separator,
    Item::Switch {
        id: ids::SERVICE_SWITCH,
        on: "Stop gglib…",
        off: "Start gglib",
    },
    Item::Action {
        id: ids::QUIT,
        label: "Quit gglib",
    },
];

/// Whether the thing a switch turns is on, by the snapshot the menu is drawn
/// from: the endpoint listening, or gglib answering.
#[must_use]
pub(super) fn is_on(id: &str, snap: &DaemonSnapshot) -> bool {
    match id {
        ids::ENDPOINT_SWITCH => snap.reachable && snap.proxy_running,
        ids::SERVICE_SWITCH => snap.reachable,
        _ => false,
    }
}

/// Whether a click on the endpoint switch turns the endpoint off (else on).
///
/// The rule its label is drawn by, so the click does the verb it showed:
/// "Turn Endpoint Off" stops, "Turn Endpoint On" starts. The macOS menu's
/// Turn On / Off takes the same path (`proxy_actions::spawn_switch`).
#[must_use]
pub(crate) fn endpoint_switch_turns_off(snap: &DaemonSnapshot) -> bool {
    is_on(ids::ENDPOINT_SWITCH, snap)
}

/// The text an entry shows for this snapshot; `None` for a separator. The
/// status line's text is `icon::derive`'s, and is passed in.
#[must_use]
pub(super) fn label(item: &Item, status: &str, snap: &DaemonSnapshot) -> Option<String> {
    match *item {
        Item::Status => Some(status.to_owned()),
        Item::Action { label, .. } => Some(label.to_owned()),
        Item::Switch { id, on, off } => Some(if is_on(id, snap) { on } else { off }.to_owned()),
        Item::Separator => None,
    }
}

/// Whether an action is available for the current daemon state.
///
/// The single rule both backends apply, so the two menus cannot disagree about
/// what is greyed out. Pure, so the table below is testable without a tray.
#[must_use]
pub(super) fn is_enabled(id: &str, snap: &DaemonSnapshot) -> bool {
    match id {
        // A label, not a command — never clickable on any backend.
        ids::STATUS => false,
        // Copying an endpoint that is not serving hands out a dead URL.
        ids::COPY_ENDPOINT_URL => snap.reachable && snap.proxy_running,
        // Turning it on or off needs a daemon to do it.
        ids::ENDPOINT_SWITCH => snap.reachable,
        _ => true,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn offline() -> DaemonSnapshot {
        DaemonSnapshot::default()
    }

    fn off() -> DaemonSnapshot {
        DaemonSnapshot::from_responses(&json!({"running": false}), &json!([]))
    }

    fn on() -> DaemonSnapshot {
        DaemonSnapshot::from_responses(&json!({"running": true, "port": 8080}), &json!([]))
    }

    /// The labels as drawn for `snap`, separators as "—".
    fn drawn(snap: &DaemonSnapshot) -> Vec<String> {
        ITEMS
            .iter()
            .map(|item| label(item, "status", snap).unwrap_or_else(|| "—".to_owned()))
            .collect()
    }

    /// The order and the words, as the spec draws the menu.
    #[test]
    fn the_menu_reads_as_the_spec_draws_it() {
        let mut expected = vec!["status"];
        if cfg!(target_os = "linux") {
            expected.push("Show panel");
        }
        expected.extend([
            "Copy Endpoint URL",
            "Turn Endpoint Off",
            "—",
            "Open gglib",
            "Settings…",
            "—",
            "Stop gglib…",
            "Quit gglib",
        ]);

        assert_eq!(drawn(&on()), expected);
    }

    /// Each switch says the verb that changes what is true now.
    #[test]
    fn the_switches_say_the_verb_that_changes_the_state() {
        assert!(drawn(&off()).contains(&"Turn Endpoint On".to_owned()));
        assert!(drawn(&off()).contains(&"Stop gglib…".to_owned()));
        assert!(drawn(&offline()).contains(&"Start gglib".to_owned()));
        assert!(drawn(&offline()).contains(&"Turn Endpoint On".to_owned()));
    }

    /// A click turns off what the switch says is on, and on what it says is
    /// off — never the reverse of its label.
    #[test]
    fn the_endpoint_switch_turns_off_only_what_is_on() {
        assert!(endpoint_switch_turns_off(&on()));
        assert!(!endpoint_switch_turns_off(&off()));
        assert!(!endpoint_switch_turns_off(&offline()));
        let switch = ITEMS
            .iter()
            .find(|i| matches!(i, Item::Switch { id, .. } if *id == ids::ENDPOINT_SWITCH))
            .expect("the endpoint switch is on the menu");
        assert_eq!(
            label(switch, "", &on()).as_deref(),
            Some("Turn Endpoint Off")
        );
        assert_eq!(
            label(switch, "", &off()).as_deref(),
            Some("Turn Endpoint On")
        );
    }

    /// The panel is listed only where the icon may have no left click.
    #[test]
    fn show_panel_is_listed_only_on_linux() {
        let listed = drawn(&on()).contains(&"Show panel".to_owned());
        assert_eq!(listed, cfg!(target_os = "linux"));
    }

    /// The status line is a label; enabling it would make it look like an action.
    #[test]
    fn the_status_line_is_never_enabled() {
        for snap in [offline(), off(), on()] {
            assert!(!is_enabled(ids::STATUS, &snap));
        }
    }

    /// With no daemon there is no endpoint to turn on, so offering it would be
    /// a button that can only fail.
    #[test]
    fn turning_the_endpoint_on_or_off_needs_a_daemon() {
        assert!(is_enabled(ids::ENDPOINT_SWITCH, &off()));
        assert!(is_enabled(ids::ENDPOINT_SWITCH, &on()));
        assert!(!is_enabled(ids::ENDPOINT_SWITCH, &offline()));
    }

    #[test]
    fn copying_needs_an_endpoint_that_is_on() {
        assert!(is_enabled(ids::COPY_ENDPOINT_URL, &on()));
        assert!(!is_enabled(ids::COPY_ENDPOINT_URL, &off()));
        assert!(!is_enabled(ids::COPY_ENDPOINT_URL, &offline()));
    }

    /// The way back from menu-bar-only mode must never be greyed out — with the
    /// window hidden and no dock icon, these are the only routes left. Start
    /// gglib is one of them: with no daemon it is the only way to get one.
    #[test]
    fn the_escape_routes_are_always_available() {
        for snap in [offline(), off(), on()] {
            for id in [
                ids::SHOW_PANEL,
                ids::OPEN_MAIN,
                ids::SETTINGS,
                ids::SERVICE_SWITCH,
                ids::QUIT,
            ] {
                assert!(is_enabled(id, &snap), "{id} must never be disabled");
            }
        }
    }

    /// Every id drawn is an id the module declares, once.
    #[test]
    fn every_entry_is_a_declared_id_drawn_once() {
        let drawn: Vec<&str> = ITEMS
            .iter()
            .filter_map(|item| match *item {
                Item::Status => Some(ids::STATUS),
                Item::Action { id, .. } | Item::Switch { id, .. } => Some(id),
                Item::Separator => None,
            })
            .collect();

        for id in &drawn {
            assert!(ids::ALL.contains(id), "{id} is not declared in ids");
            assert_eq!(
                drawn.iter().filter(|d| *d == id).count(),
                1,
                "{id} drawn twice"
            );
        }
    }
}
