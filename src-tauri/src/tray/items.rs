//! What is on the tray menu, and when each entry is available.
//!
//! Two backends render this list — `muda` on macOS and Windows, `ksni` on
//! Linux — and they share no menu model, so the list itself is the only thing
//! that can keep them in step. Order, labels and the enabled rule live here
//! once; each backend only knows how to draw an [`Item`].
//!
//! The distro tables in `gglib-core` are the cautionary tale: four copies of
//! the same knowledge drifted apart until three of them were wrong.

use crate::tray::ids;

/// One entry on the tray menu.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Item {
    /// The endpoint status header. Always disabled — a label, not a command —
    /// and the only entry whose text changes, from `icon::derive`.
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
pub const ITEMS: &[Item] = &[
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

/// Whether an action is available for the current proxy state.
///
/// The single rule both backends apply, so the two menus cannot disagree about
/// what is greyed out. Pure, so the table below is testable without a tray.
#[must_use]
pub fn is_enabled(id: &str, proxy_running: bool) -> bool {
    match id {
        ids::START_PROXY => !proxy_running,
        // Copying an endpoint that is not serving hands out a dead URL.
        ids::STOP_PROXY | ids::COPY_PROXY_URL => proxy_running,
        _ => true,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn starting_is_offered_only_while_stopped() {
        assert!(is_enabled(ids::START_PROXY, false));
        assert!(!is_enabled(ids::START_PROXY, true));
    }

    /// Stopping and copying both need something to act on.
    #[test]
    fn stopping_and_copying_need_a_running_proxy() {
        for id in [ids::STOP_PROXY, ids::COPY_PROXY_URL] {
            assert!(is_enabled(id, true), "{id} should be enabled when running");
            assert!(
                !is_enabled(id, false),
                "{id} should be disabled when stopped"
            );
        }
    }

    /// The way back from menu-bar-only mode must never be greyed out — with the
    /// window hidden and no dock icon, these are the only routes left.
    #[test]
    fn the_escape_routes_are_always_available() {
        for running in [true, false] {
            for id in [ids::OPEN_PANEL, ids::OPEN_MAIN, ids::PREFERENCES, ids::QUIT] {
                assert!(is_enabled(id, running), "{id} must never be disabled");
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
