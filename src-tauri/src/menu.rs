//! Native application menu for GGLib GUI.
//!
//! Provides a cross-platform menu bar with stateful items that reflect
//! the current application state (llama.cpp installation, proxy status,
//! selected model, etc.).

use tauri::{
    menu::{AboutMetadataBuilder, CheckMenuItem, Menu, MenuItem, PredefinedMenuItem, Submenu},
    AppHandle, Wry,
};

/// Menu item IDs for event handling
pub mod ids {
    // File menu
    pub const ADD_MODEL_FILE: &str = "add_model_file";
    pub const DOWNLOAD_MODEL: &str = "download_model";
    pub const REFRESH_MODELS: &str = "refresh_models";

    // Model menu
    pub const START_SERVER: &str = "start_server";
    pub const STOP_SERVER: &str = "stop_server";
    pub const REMOVE_MODEL: &str = "remove_model";

    // Proxy menu
    pub const PROXY_TOGGLE: &str = "proxy_toggle";
    pub const COPY_PROXY_URL: &str = "copy_proxy_url";

    // View menu
    pub const SHOW_DOWNLOADS: &str = "show_downloads";
    pub const SHOW_CHAT: &str = "show_chat";
    pub const TOGGLE_SIDEBAR: &str = "toggle_sidebar";

    // Help menu
    pub const INSTALL_LLAMA: &str = "install_llama";
    pub const CHECK_LLAMA_STATUS: &str = "check_llama_status";
    pub const OPEN_DOCS: &str = "open_docs";

    // App menu
    pub const PREFERENCES: &str = "preferences";
}

/// Holds references to menu items that need dynamic state updates.
///
/// This struct is stored in the application state and used to
/// enable/disable or check/uncheck menu items based on app state.
pub struct AppMenu {
    // Model menu items (enabled based on selection/server state)
    pub start_server: MenuItem<Wry>,
    pub stop_server: MenuItem<Wry>,
    pub remove_model: MenuItem<Wry>,

    // Proxy menu items
    pub proxy_toggle: CheckMenuItem<Wry>,
    pub copy_proxy_url: MenuItem<Wry>,

    // Help menu items
    pub install_llama: MenuItem<Wry>,
}

/// State used to synchronize menu item enabled/checked status
#[derive(Debug, Clone, Default)]
pub struct MenuState {
    pub llama_installed: bool,
    pub proxy_running: bool,
    pub model_selected: bool,
    pub selected_model_server_running: bool,
}

impl AppMenu {
    /// Update all stateful menu items based on current application state.
    ///
    /// This should be called whenever relevant state changes:
    /// - Model selection changes
    /// - Server starts/stops
    /// - Proxy starts/stops
    /// - llama.cpp is installed
    pub fn sync_state(&self, state: &MenuState) -> Result<(), tauri::Error> {
        // Model menu items
        // Start Server: enabled if model selected AND not already running
        self.start_server
            .set_enabled(state.model_selected && !state.selected_model_server_running)?;

        // Stop Server: enabled if model selected AND currently running
        self.stop_server
            .set_enabled(state.model_selected && state.selected_model_server_running)?;

        // Remove Model: enabled if model selected
        self.remove_model.set_enabled(state.model_selected)?;

        // Proxy menu items
        // Toggle checked state based on whether proxy is running
        self.proxy_toggle.set_checked(state.proxy_running)?;

        // Copy URL: only enabled if proxy is running
        self.copy_proxy_url.set_enabled(state.proxy_running)?;

        // Help menu items
        // Install llama.cpp: disabled if already installed
        self.install_llama.set_enabled(!state.llama_installed)?;

        Ok(())
    }
}

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
        .authors(Some(vec!["mmogr".to_string()]))
        .license(Some("MIT"))
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
