//! What is on the tray menu, and when each entry is available.
//!
//! Two backends render this list — `muda` on macOS and Windows, `ksni` on
//! Linux — and they share no menu model, so the list itself is the only thing
//! that can keep them in step. Order, labels and the enabled rule live here
//! once; each backend only knows how to draw an [`Item`].
//!
//! The distro tables in `gglib-core` are the cautionary tale: four copies of
//! the same knowledge drifted apart until three of them were wrong.

use crate::daemon::DaemonSnapshot;
use crate::tray::ids;

/// One entry on the tray menu.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Item {
    /// The status header. Always disabled — a label, not a command — and the
    /// only entry whose text changes, from `icon::derive`.
    Status,
    /// A clickable entry, routed by `id` through [`super::handlers::dispatch`].
    Action {
        id: &'static str,
        label: &'static str,
    },
    Separator,
}

/// The menu, in order.
///
/// Every action appears here, including the ones that also have a click
/// gesture: the gesture is a shortcut, never the only route to a feature.
pub(super) const ITEMS: &[Item] = &[
    Item::Status,
    Item::Separator,
    Item::Action {
        id: ids::OPEN_PANEL,
        label: "Proxy Panel",
    },
    Item::Separator,
    Item::Action {
        id: ids::START_PROXY,
        label: "Start Proxy",
    },
    Item::Action {
        id: ids::STOP_PROXY,
        label: "Stop Proxy",
    },
    Item::Action {
        id: ids::COPY_PROXY_URL,
        label: "Copy Endpoint URL",
    },
    Item::Separator,
    Item::Action {
        id: ids::START_SERVICE,
        label: "Start gglib Service",
    },
    Item::Action {
        id: ids::STOP_SERVICE,
        label: "Stop gglib Service",
    },
    Item::Separator,
    Item::Action {
        id: ids::OPEN_MAIN,
        label: "Open gglib",
    },
    Item::Action {
        id: ids::PREFERENCES,
        label: "Preferences…",
    },
    Item::Separator,
    Item::Action {
        id: ids::QUIT,
        label: "Quit gglib",
    },
];

/// Whether an action is available for the current daemon state.
///
/// The single rule both backends apply, so the two menus cannot disagree about
/// what is greyed out. Pure, so the table below is testable without a tray.
#[must_use]
pub(super) fn is_enabled(id: &str, snap: &DaemonSnapshot) -> bool {
    match id {
        // A label, not a command — never clickable on any backend.
        ids::STATUS => false,
        // Every proxy action needs a daemon to perform it.
        ids::START_PROXY => snap.reachable && !snap.proxy_running,
        // Copying an endpoint that is not serving hands out a dead URL.
        ids::STOP_PROXY | ids::COPY_PROXY_URL => snap.proxy_running,
        ids::START_SERVICE => !snap.reachable,
        ids::STOP_SERVICE => snap.reachable,
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

    fn idle() -> DaemonSnapshot {
        DaemonSnapshot::from_responses(&json!({"running": false}), &json!([]))
    }

    fn serving() -> DaemonSnapshot {
        DaemonSnapshot::from_responses(&json!({"running": true, "port": 8080}), &json!([]))
    }

    /// The header is a label; enabling it would make it look like an action.
    #[test]
    fn the_status_header_is_never_enabled() {
        for snap in [offline(), idle(), serving()] {
            assert!(!is_enabled(ids::STATUS, &snap));
        }
    }

    #[test]
    fn starting_the_proxy_is_offered_only_while_stopped() {
        assert!(is_enabled(ids::START_PROXY, &idle()));
        assert!(!is_enabled(ids::START_PROXY, &serving()));
    }

    /// With no daemon there is nothing to start a proxy on, so offering it
    /// would be a button that can only fail.
    #[test]
    fn starting_the_proxy_needs_a_daemon() {
        assert!(!is_enabled(ids::START_PROXY, &offline()));
    }

    /// Stopping and copying both need something to act on.
    #[test]
    fn stopping_and_copying_need_a_running_proxy() {
        for id in [ids::STOP_PROXY, ids::COPY_PROXY_URL] {
            assert!(is_enabled(id, &serving()), "{id} should be enabled");
            assert!(!is_enabled(id, &idle()), "{id} should be disabled");
            assert!(!is_enabled(id, &offline()), "{id} should be disabled");
        }
    }

    /// The service verbs are exclusive: exactly one of them is live at a time,
    /// so the menu never offers to start what is already running.
    #[test]
    fn the_service_verbs_mirror_each_other() {
        for snap in [offline(), idle(), serving()] {
            assert_ne!(
                is_enabled(ids::START_SERVICE, &snap),
                is_enabled(ids::STOP_SERVICE, &snap),
            );
        }
    }

    #[test]
    fn stopping_the_service_needs_a_daemon_to_stop() {
        assert!(is_enabled(ids::STOP_SERVICE, &idle()));
        assert!(!is_enabled(ids::STOP_SERVICE, &offline()));
    }

    /// The way back from menu-bar-only mode must never be greyed out — with the
    /// window hidden and no dock icon, these are the only routes left.
    #[test]
    fn the_escape_routes_are_always_available() {
        for snap in [offline(), idle(), serving()] {
            for id in [ids::OPEN_PANEL, ids::OPEN_MAIN, ids::PREFERENCES, ids::QUIT] {
                assert!(is_enabled(id, &snap), "{id} must never be disabled");
            }
        }
    }

    /// Every action must be routable, or it renders and then does nothing.
    #[test]
    fn every_action_has_a_known_id() {
        let known = [
            ids::OPEN_PANEL,
            ids::START_PROXY,
            ids::STOP_PROXY,
            ids::COPY_PROXY_URL,
            ids::START_SERVICE,
            ids::STOP_SERVICE,
            ids::OPEN_MAIN,
            ids::PREFERENCES,
            ids::QUIT,
        ];

        for item in ITEMS {
            if let Item::Action { id, .. } = item {
                assert!(known.contains(id), "{id} is not routed by dispatch");
            }
        }
    }

    /// Exactly one status header, or `sync` would update the wrong entry.
    #[test]
    fn there_is_a_single_status_header() {
        let headers = ITEMS.iter().filter(|i| **i == Item::Status).count();
        assert_eq!(headers, 1);
    }
}
