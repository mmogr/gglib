#![doc = include_str!("README.md")]
#[cfg(target_os = "macos")]
mod build;

#[cfg(target_os = "macos")]
pub(crate) mod handlers;

#[cfg(target_os = "macos")]
pub(crate) mod ids;
pub(crate) mod state_sync;

#[cfg(target_os = "macos")]
pub(crate) use build::build_app_menu;

#[cfg(target_os = "macos")]
use tauri::{Wry, menu::MenuItem};

/// The Endpoint menu's switch while the endpoint is off, and while it is on.
#[cfg(target_os = "macos")]
pub(crate) const TURN_ON: &str = "Turn On";
#[cfg(target_os = "macos")]
const TURN_OFF: &str = "Turn Off";

/// Holds references to menu items that need dynamic state updates.
///
/// macOS only: the other platforms are given an empty menu at startup so no
/// menu bar appears, leaving nothing to hold references to.
#[cfg(target_os = "macos")]
pub(crate) struct AppMenu {
    /// The Model and Endpoint menus' items, each beside its id.
    pub stateful: Vec<(&'static str, MenuItem<Wry>)>,
}

/// What the menu's enabled items and labels are drawn from.
///
/// The `Default` is "nothing known": no daemon, no model. It is what the menu
/// is built with, before the first poll.
#[cfg(target_os = "macos")]
#[derive(Debug, Clone, Copy, Default)]
pub(crate) struct MenuState {
    pub endpoint: Endpoint,
    pub inspected: Inspected,
}

/// The endpoint, as the last poll found it.
#[cfg(target_os = "macos")]
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(crate) enum Endpoint {
    /// No daemon answered: there is nothing to turn on.
    #[default]
    NoDaemon,
    Off,
    On,
}

/// The model in the inspector, if it is one of this machine's.
#[cfg(target_os = "macos")]
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(crate) enum Inspected {
    #[default]
    Nothing,
    /// A model with no live server.
    Idle,
    /// A model with a live server.
    Served,
}

/// Whether an item is offered in this state. Items with no rule always are.
#[cfg(target_os = "macos")]
#[must_use]
pub(crate) fn is_enabled(id: &str, state: MenuState) -> bool {
    match id {
        ids::LOAD | ids::LOAD_WITH_OPTIONS => state.inspected == Inspected::Idle,
        ids::UNLOAD => state.inspected == Inspected::Served,
        ids::REMOVE => state.inspected != Inspected::Nothing,
        ids::ENDPOINT_SWITCH => state.endpoint != Endpoint::NoDaemon,
        // A dead URL is worse than no URL.
        ids::COPY_ENDPOINT_URL => state.endpoint == Endpoint::On,
        _ => true,
    }
}

/// The text an item takes in this state, for the one whose verb changes.
#[cfg(target_os = "macos")]
#[must_use]
pub(crate) fn label(id: &str, state: MenuState) -> Option<&'static str> {
    (id == ids::ENDPOINT_SWITCH).then_some(if state.endpoint == Endpoint::On {
        TURN_OFF
    } else {
        TURN_ON
    })
}

#[cfg(target_os = "macos")]
impl AppMenu {
    /// Update every stateful item from the current state.
    ///
    /// Called whenever something it reads changes: the selected model (from
    /// `set_selected_model`) and the daemon's snapshot (every poll that sees a
    /// change).
    pub(crate) fn sync_state(&self, state: MenuState) -> Result<(), tauri::Error> {
        for (id, item) in &self.stateful {
            item.set_enabled(is_enabled(id, state))?;
            if let Some(text) = label(id, state) {
                item.set_text(text)?;
            }
        }
        Ok(())
    }
}

#[cfg(all(test, target_os = "macos"))]
mod tests {
    use super::*;

    fn state(endpoint: Endpoint, inspected: Inspected) -> MenuState {
        MenuState {
            endpoint,
            inspected,
        }
    }

    /// With nothing in the inspector, the Model menu has nothing to act on.
    #[test]
    fn the_model_items_need_a_model() {
        let none = state(Endpoint::On, Inspected::Nothing);
        for id in [ids::LOAD, ids::LOAD_WITH_OPTIONS, ids::UNLOAD, ids::REMOVE] {
            assert!(!is_enabled(id, none), "{id} enabled with no model");
        }
    }

    /// Load and Unload mirror each other; Remove is offered either way.
    #[test]
    fn load_and_unload_follow_whether_the_model_is_served() {
        let idle = state(Endpoint::On, Inspected::Idle);
        let served = state(Endpoint::On, Inspected::Served);

        assert!(is_enabled(ids::LOAD, idle) && is_enabled(ids::LOAD_WITH_OPTIONS, idle));
        assert!(!is_enabled(ids::UNLOAD, idle));
        assert!(!is_enabled(ids::LOAD, served) && !is_enabled(ids::LOAD_WITH_OPTIONS, served));
        assert!(is_enabled(ids::UNLOAD, served));
        assert!(is_enabled(ids::REMOVE, idle) && is_enabled(ids::REMOVE, served));
    }

    /// Turn On / Off needs a daemon; its verb changes what is true now.
    #[test]
    fn the_endpoint_switch_says_the_verb_and_needs_a_daemon() {
        let gone = state(Endpoint::NoDaemon, Inspected::Nothing);
        let off = state(Endpoint::Off, Inspected::Nothing);
        let on = state(Endpoint::On, Inspected::Nothing);

        assert!(!is_enabled(ids::ENDPOINT_SWITCH, gone));
        assert!(is_enabled(ids::ENDPOINT_SWITCH, off));
        assert_eq!(label(ids::ENDPOINT_SWITCH, gone), Some("Turn On"));
        assert_eq!(label(ids::ENDPOINT_SWITCH, off), Some("Turn On"));
        assert_eq!(label(ids::ENDPOINT_SWITCH, on), Some("Turn Off"));
        assert_eq!(label(ids::COPY_ENDPOINT_URL, on), None);
    }

    #[test]
    fn copying_needs_an_endpoint_that_is_on() {
        assert!(is_enabled(
            ids::COPY_ENDPOINT_URL,
            state(Endpoint::On, Inspected::Nothing)
        ));
        assert!(!is_enabled(
            ids::COPY_ENDPOINT_URL,
            state(Endpoint::Off, Inspected::Nothing)
        ));
    }

    /// Before the first poll nothing that needs a daemon or a model is
    /// offered, and the places always are.
    #[test]
    fn the_menu_is_built_offering_only_what_needs_nothing() {
        let initial = MenuState::default();
        for id in ids::ALL {
            let needs_something = [
                ids::LOAD,
                ids::LOAD_WITH_OPTIONS,
                ids::UNLOAD,
                ids::REMOVE,
                ids::ENDPOINT_SWITCH,
                ids::COPY_ENDPOINT_URL,
            ]
            .contains(&id);
            assert_eq!(is_enabled(id, initial), !needs_something, "{id}");
        }
    }
}
