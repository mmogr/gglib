//! Menu event handling.
//!
//! A place is one `menu:navigate` event carrying the place's id; an action
//! on the inspected model is an event of its own, which the frontend runs on
//! the model its address names. The endpoint's Turn On / Off and Copy are done
//! here, natively, the same way the tray does them.

use crate::app::events::{emit_or_log, names, places};
use crate::menu::ids;
use tauri::AppHandle;
use tracing::debug;

/// What a menu item does.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Action {
    /// `menu:navigate` with this place.
    Navigate(&'static str),
    /// This event, for the frontend to act on.
    Emit(&'static str),
    SwitchEndpoint,
    CopyEndpointUrl,
    OpenDocs,
}

/// The action an id names, or `None` for an id that is not ours.
///
/// Pure, so a test can hold every id on the menu to an action of its own.
fn action_of(id: &str) -> Option<Action> {
    use Action::{CopyEndpointUrl, Emit, Navigate, OpenDocs, SwitchEndpoint};

    Some(match id {
        ids::SETTINGS => Navigate(places::SETTINGS),
        ids::GET_MODELS => Navigate(places::GET_MODELS),
        ids::ADD_FILE => Navigate(places::ADD_FILE),
        ids::NEW_CHAT => Navigate(places::NEW_CHAT),
        ids::SHOW_LIBRARY => Navigate(places::LIBRARY),
        ids::SHOW_ENDPOINT => Navigate(places::ENDPOINT),
        ids::SHOW_DEVICES => Navigate(places::DEVICES),
        ids::SHOW_CHAT => Navigate(places::CHAT),
        ids::SHOW_DIAGNOSTICS => Navigate(places::DIAGNOSTICS),
        ids::CONNECT_CLIENT => Navigate(places::CONNECT_CLIENT),
        ids::HIDE_INSPECTOR => Emit(names::MENU_HIDE_INSPECTOR),
        ids::LOAD => Emit(names::MENU_LOAD_MODEL),
        ids::LOAD_WITH_OPTIONS => Emit(names::MENU_LOAD_WITH_OPTIONS),
        ids::UNLOAD => Emit(names::MENU_UNLOAD_MODEL),
        ids::REMOVE => Emit(names::MENU_REMOVE_MODEL),
        ids::ENDPOINT_SWITCH => SwitchEndpoint,
        ids::COPY_ENDPOINT_URL => CopyEndpointUrl,
        ids::OPEN_DOCS => OpenDocs,
        _ => return None,
    })
}

/// Handle menu item click events.
#[allow(
    clippy::needless_pass_by_value,
    reason = "Tauri hands a menu event to its handler by value"
)]
pub(crate) fn handle_menu_event(app: &AppHandle, event: tauri::menu::MenuEvent) {
    let id = event.id().as_ref();

    debug!(menu_id = %id, "Menu event received");

    match action_of(id) {
        Some(Action::Navigate(place)) => emit_or_log(app, names::MENU_NAVIGATE, place),
        Some(Action::Emit(name)) => emit_or_log(app, name, ()),
        Some(Action::SwitchEndpoint) => crate::proxy_actions::spawn_switch(app),
        Some(Action::CopyEndpointUrl) => {
            let app = app.clone();
            tauri::async_runtime::spawn(async move {
                crate::proxy_actions::copy_endpoint_url(&app).await;
            });
        }
        Some(Action::OpenDocs) => {
            let _ = open::that("https://github.com/mmogr/gglib");
        }
        // Not one of ours: the OS's predefined items, and any tray item
        // (`tray_…`, never one of these ids) its own handler routes.
        None => debug!(menu_id = %id, "Unhandled menu event"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Every id on the menu does something, and no two do the same thing.
    #[test]
    fn every_id_is_routed_to_its_own_action() {
        let mut actions: Vec<Action> = Vec::new();
        for id in ids::ALL {
            let action = action_of(id).unwrap_or_else(|| panic!("{id} is not routed"));
            assert!(!actions.contains(&action), "{id} shares an action");
            actions.push(action);
        }
    }

    /// Turn On / Off and Copy are done here, not sent to a webview that may
    /// not be listening: the same path as the tray's.
    #[test]
    fn the_endpoint_is_switched_and_copied_natively() {
        assert_eq!(
            action_of(ids::ENDPOINT_SWITCH),
            Some(Action::SwitchEndpoint)
        );
        assert_eq!(
            action_of(ids::COPY_ENDPOINT_URL),
            Some(Action::CopyEndpointUrl)
        );
    }

    /// The places the menu names, each once: the frontend's table is held to
    /// the same list by `menuEvents.test.ts`.
    #[test]
    fn the_places_are_the_settings_and_the_view_and_file_items() {
        let mut navigated: Vec<&str> = ids::ALL
            .iter()
            .filter_map(|id| match action_of(id) {
                Some(Action::Navigate(place)) => Some(place),
                _ => None,
            })
            .collect();
        navigated.sort_unstable();

        assert_eq!(
            navigated,
            vec![
                "add-file",
                "chat",
                "connect-client",
                "devices",
                "diagnostics",
                "endpoint",
                "get-models",
                "library",
                "new-chat",
                "settings",
            ]
        );
    }
}
