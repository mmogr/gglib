//! Menu construction.

use super::{ids, AppMenu};
use tauri::{
    menu::{AboutMetadataBuilder, CheckMenuItem, Menu, MenuItem, PredefinedMenuItem, Submenu},
    AppHandle, Wry,
};

/// Build the complete application menu.
///
/// Returns both the Menu to attach to the app and the AppMenu struct
/// containing references to stateful items for later updates.
pub fn build_app_menu(app: &AppHandle) -> Result<(Menu<Wry>, AppMenu), tauri::Error> {
    // =========================================================================
    // App Menu (GGLib) - First submenu becomes app menu on macOS
    // =========================================================================
    let about_metadata = AboutMetadataBuilder::new()
        .name(Some("GGLib"))
        .version(Some(env!("CARGO_PKG_VERSION")))
        .short_version(Some(env!("GIT_COMMIT_HASH")))
        .authors(Some(vec!["mmogr".to_string()]))
        .license(Some("GPLv3"))
        .website(Some("https://github.com/mmogr/gglib"))
        .website_label(Some("GitHub"))
        .build();

    let preferences_item = MenuItem::with_id(
        app,
        ids::PREFERENCES,
        "Settings...",
        true,
        Some("CmdOrCtrl+,"),
    )?;

    let app_submenu = Submenu::with_items(
        app,
        "GGLib",
        true,
        &[
            &PredefinedMenuItem::about(app, Some("About GGLib"), Some(about_metadata))?,
            &PredefinedMenuItem::separator(app)?,
            &preferences_item,
            &PredefinedMenuItem::separator(app)?,
            &PredefinedMenuItem::services(app, None)?,
            &PredefinedMenuItem::separator(app)?,
            &PredefinedMenuItem::hide(app, Some("Hide GGLib"))?,
            &PredefinedMenuItem::hide_others(app, Some("Hide Others"))?,
            &PredefinedMenuItem::show_all(app, Some("Show All"))?,
            &PredefinedMenuItem::separator(app)?,
            &PredefinedMenuItem::quit(app, Some("Quit GGLib"))?,
        ],
    )?;

    // =========================================================================
    // File Menu
    // =========================================================================
    let add_model_item = MenuItem::with_id(
        app,
        ids::ADD_MODEL_FILE,
        "Add Model from File...",
        true,
        Some("CmdOrCtrl+O"),
    )?;

    let download_item = MenuItem::with_id(
        app,
        ids::DOWNLOAD_MODEL,
        "Download from Hugging Face...",
        true,
        Some("CmdOrCtrl+D"),
    )?;

    let refresh_item = MenuItem::with_id(
        app,
        ids::REFRESH_MODELS,
        "Refresh Model List",
        true,
        Some("CmdOrCtrl+R"),
    )?;

    let file_submenu = Submenu::with_items(
        app,
        "File",
        true,
        &[
            &add_model_item,
            &download_item,
            &PredefinedMenuItem::separator(app)?,
            &refresh_item,
        ],
    )?;

    // =========================================================================
    // Edit Menu - Standard text editing shortcuts (Copy, Cut, Paste, etc.)
    // =========================================================================
    // Note: Undo/Redo are only supported on macOS via PredefinedMenuItem.
    // On Windows/Linux, the WebView handles Ctrl+Z/Ctrl+Y natively.
    let edit_submenu = Submenu::with_items(
        app,
        "Edit",
        true,
        &[
            #[cfg(target_os = "macos")]
            &PredefinedMenuItem::undo(app, None)?,
            #[cfg(target_os = "macos")]
            &PredefinedMenuItem::redo(app, None)?,
            #[cfg(target_os = "macos")]
            &PredefinedMenuItem::separator(app)?,
            &PredefinedMenuItem::cut(app, None)?,
            &PredefinedMenuItem::copy(app, None)?,
            &PredefinedMenuItem::paste(app, None)?,
            &PredefinedMenuItem::separator(app)?,
            &PredefinedMenuItem::select_all(app, None)?,
        ],
    )?;

    // =========================================================================
    // Model Menu
    // =========================================================================
    // These start disabled until a model is selected
    let start_server_item = MenuItem::with_id(
        app,
        ids::START_SERVER,
        "Start Server",
        false, // Initially disabled
        Some("CmdOrCtrl+Return"),
    )?;

    let stop_server_item = MenuItem::with_id(
        app,
        ids::STOP_SERVER,
        "Stop Server",
        false, // Initially disabled
        Some("CmdOrCtrl+."),
    )?;

    let remove_model_item = MenuItem::with_id(
        app,
        ids::REMOVE_MODEL,
        "Remove from Library...",
        false, // Initially disabled
        Some("CmdOrCtrl+Backspace"),
    )?;

    let model_submenu = Submenu::with_items(
        app,
        "Model",
        true,
        &[
            &start_server_item,
            &stop_server_item,
            &PredefinedMenuItem::separator(app)?,
            &remove_model_item,
        ],
    )?;

    // =========================================================================
    // Proxy Menu
    // =========================================================================
    let proxy_toggle_item = CheckMenuItem::with_id(
        app,
        ids::PROXY_TOGGLE,
        "Enable Proxy",
        true,
        false, // Initially unchecked
        Some("CmdOrCtrl+P"),
    )?;

    let copy_proxy_url_item = MenuItem::with_id(
        app,
        ids::COPY_PROXY_URL,
        "Copy Proxy URL",
        false, // Initially disabled (proxy not running)
        Some("CmdOrCtrl+Shift+C"),
    )?;

    let proxy_submenu = Submenu::with_items(
        app,
        "Proxy",
        true,
        &[
            &proxy_toggle_item,
            &PredefinedMenuItem::separator(app)?,
            &copy_proxy_url_item,
        ],
    )?;

    // =========================================================================
    // View Menu
    // =========================================================================
    let show_downloads_item = MenuItem::with_id(
        app,
        ids::SHOW_DOWNLOADS,
        "Show Downloads Panel",
        true,
        Some("CmdOrCtrl+1"),
    )?;

    let show_chat_item =
        MenuItem::with_id(app, ids::SHOW_CHAT, "Show Chat", true, Some("CmdOrCtrl+2"))?;

    let toggle_sidebar_item = MenuItem::with_id(
        app,
        ids::TOGGLE_SIDEBAR,
        "Toggle Sidebar",
        true,
        Some("CmdOrCtrl+\\"),
    )?;

    let view_submenu = Submenu::with_items(
        app,
        "View",
        true,
        &[
            &show_downloads_item,
            &show_chat_item,
            &PredefinedMenuItem::separator(app)?,
            &toggle_sidebar_item,
            &PredefinedMenuItem::separator(app)?,
            &PredefinedMenuItem::fullscreen(app, Some("Enter Full Screen"))?,
        ],
    )?;

    // =========================================================================
    // Window Menu (standard macOS)
    // =========================================================================
    let window_submenu = Submenu::with_items(
        app,
        "Window",
        true,
        &[
            &PredefinedMenuItem::minimize(app, Some("Minimize"))?,
            &PredefinedMenuItem::maximize(app, Some("Zoom"))?,
            &PredefinedMenuItem::separator(app)?,
            &PredefinedMenuItem::close_window(app, Some("Close Window"))?,
        ],
    )?;

    // =========================================================================
    // Help Menu
    // =========================================================================
    let install_llama_item = MenuItem::with_id(
        app,
        ids::INSTALL_LLAMA,
        "Install llama.cpp...",
        true, // Will be disabled if already installed
        None::<&str>,
    )?;

    let check_status_item = MenuItem::with_id(
        app,
        ids::CHECK_LLAMA_STATUS,
        "Check llama.cpp Status",
        true,
        None::<&str>,
    )?;

    let open_docs_item = MenuItem::with_id(
        app,
        ids::OPEN_DOCS,
        "GGLib Documentation",
        true,
        None::<&str>,
    )?;

    let help_submenu = Submenu::with_items(
        app,
        "Help",
        true,
        &[
            &install_llama_item,
            &check_status_item,
            &PredefinedMenuItem::separator(app)?,
            &open_docs_item,
        ],
    )?;

    // =========================================================================
    // Build Complete Menu
    // =========================================================================
    let menu = Menu::with_items(
        app,
        &[
            &app_submenu,
            &file_submenu,
            &edit_submenu,
            &model_submenu,
            &proxy_submenu,
            &view_submenu,
            &window_submenu,
            &help_submenu,
        ],
    )?;

    // Create AppMenu with references to stateful items
    let app_menu = AppMenu {
        start_server: start_server_item,
        stop_server: stop_server_item,
        remove_model: remove_model_item,
        proxy_toggle: proxy_toggle_item,
        copy_proxy_url: copy_proxy_url_item,
        install_llama: install_llama_item,
    };

    Ok((menu, app_menu))
}
