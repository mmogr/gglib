//! The gglib menu: About, Settings… and the items macOS puts in every app's
//! first menu.

use tauri::{
    AppHandle, Wry,
    menu::{AboutMetadataBuilder, PredefinedMenuItem, Submenu},
};

use super::{Entry, item};
use crate::menu::ids;

pub(super) const ENTRIES: &[Entry] = &[Entry {
    id: ids::SETTINGS,
    label: "Settings…",
    accelerator: Some("CmdOrCtrl+,"),
}];

pub(super) fn submenu(app: &AppHandle) -> tauri::Result<Submenu<Wry>> {
    let about = AboutMetadataBuilder::new()
        .name(Some("gglib"))
        .version(Some(gglib_build_info::SEMVER))
        .short_version(gglib_build_info::HAS_GIT_SHA.then_some(gglib_build_info::GIT_SHA))
        .authors(Some(vec!["mmogr".to_string()]))
        .license(Some("AGPL-3.0-only"))
        .website(Some("https://github.com/mmogr/gglib"))
        .website_label(Some("GitHub"))
        .build();

    let [settings] = ENTRIES else { unreachable!() };

    Submenu::with_items(
        app,
        "gglib",
        true,
        &[
            &PredefinedMenuItem::about(app, Some("About gglib"), Some(about))?,
            &PredefinedMenuItem::separator(app)?,
            &item(app, settings)?,
            &PredefinedMenuItem::separator(app)?,
            &PredefinedMenuItem::services(app, None)?,
            &PredefinedMenuItem::separator(app)?,
            &PredefinedMenuItem::hide(app, Some("Hide gglib"))?,
            &PredefinedMenuItem::hide_others(app, Some("Hide Others"))?,
            &PredefinedMenuItem::show_all(app, Some("Show All"))?,
            &PredefinedMenuItem::separator(app)?,
            &PredefinedMenuItem::quit(app, Some("Quit gglib"))?,
        ],
    )
}
