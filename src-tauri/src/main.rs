// Prevents additional console window on Windows in release, DO NOT REMOVE!!
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod app;
mod autostart;
mod commands;
mod daemon;
mod dock;
mod lifecycle;
mod menu;
mod proxy_actions;
mod tray;

use std::sync::Arc;

use app::AppState;
use app::events::{emit_or_log, names};
use daemon::Daemon;
use dotenvy::dotenv;
use menu::state_sync::sync_all_state_logged;
use tauri::Manager;
#[cfg(not(target_os = "macos"))]
use tauri::Wry;
#[cfg(not(target_os = "macos"))]
use tauri::menu::Menu;
use tracing::{debug, error, info};

fn main() {
    let _ = dotenv();

    // Initialize shared tracing (idempotent; safe to call from multiple entry points)
    let _ = gglib_core::telemetry::init_tracing(false);

    info!("Tauri application starting");

    let builder = tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_autostart::init(
            tauri_plugin_autostart::MacosLauncher::LaunchAgent,
            // Marks launches the OS started, so those can go straight to the
            // menu bar. `apply_login_item` re-registers on every run and the
            // plugin rewrites the entry unconditionally, so existing login
            // items pick this up on their next launch with nothing to migrate.
            Some(vec![autostart::LOGIN_ITEM_FLAG]),
        ))
        .setup(move |app| {
            // The daemon owns the backend: connect to a running one, launch
            // `gglib daemon run` detached, or — bundle-only fallback — host
            // the daemon composition in this process behind the same lock.
            let daemon = tauri::async_runtime::block_on(Daemon::connect_or_launch())
                .expect("Failed to reach or start the gglib daemon");
            let hosted = daemon.hosted_in_process;
            let app_state = AppState::new(Arc::new(daemon));
            app.manage(app_state);
            info!(hosted_in_process = hosted, "daemon connection established");

            // Downloads run on the daemon and need nothing provisioned in
            // this process, so the subsystem is ready by construction.
            emit_or_log(app.handle(), names::DOWNLOAD_SYSTEM_READY, true);

            // Continue with rest of setup
            setup_app(app)?;

            // Register/unregister the login item to match settings once the
            // window exists. (Proxy autostart is the daemon's job now.)
            {
                let handle_for_autostart = app.handle().clone();
                tauri::async_runtime::spawn(async move {
                    autostart::apply(&handle_for_autostart).await;
                });
            }

            Ok(())
        })
        .on_window_event(|window, event| match event {
            tauri::WindowEvent::CloseRequested { api, .. } => {
                api.prevent_close();

                // The panel is a popover, not a window anyone closes on
                // purpose; hide it and leave the app alone.
                if window.label() == tray::window::PANEL_LABEL {
                    let _ = window.hide();
                    return;
                }

                // Hide immediately either way, so the click feels instant
                // whether it ends in a shutdown or not.
                let _ = window.hide();

                let app_handle = window.app_handle().clone();
                tauri::async_runtime::spawn(async move {
                    if close_to_tray_enabled(&app_handle).await {
                        // Nothing is torn down here on purpose: the daemon
                        // outlives the window, so the tray panel keeps a
                        // live API to talk to.
                        dock::hide(&app_handle);
                        info!("Window closed to tray - daemon left running");
                        return;
                    }

                    info!("Window close requested - performing graceful shutdown");
                    lifecycle::request_shutdown(&app_handle);
                });
            }
            // Dismiss the panel when it loses focus, the way a menu does.
            tauri::WindowEvent::Focused(false) if window.label() == tray::window::PANEL_LABEL => {
                let _ = window.hide();
            }
            _ => {}
        });

    #[cfg(target_os = "macos")]
    let builder = builder.on_menu_event(menu::handlers::handle_menu_event);

    builder
        .invoke_handler(tauri::generate_handler![
            // API discovery
            commands::util::get_embedded_api_info,
            // OS integration: shell
            commands::util::open_url,
            // OS integration: menu sync
            commands::util::set_selected_model,
            commands::util::sync_menu_state,
            commands::util::set_proxy_state,
            // OS integration: llama.cpp binary management
            commands::llama::check_llama_status,
            commands::llama::install_llama,
            commands::llama::build_llama_from_source,
            // Frontend logging: bridge to Rust tracing
            commands::app_logs::log_from_frontend,
        ])
        .build(tauri::generate_context!())
        .expect("error while building tauri application")
        .run(|app_handle, event| {
            match event {
                tauri::RunEvent::ExitRequested { api, .. } => {
                    // The exit `request_shutdown` itself asks for comes back
                    // through here; preventing that one is what used to
                    // strand the app.
                    if !lifecycle::should_prevent_exit(lifecycle::is_shutting_down()) {
                        info!("Shutdown complete - letting the app exit");
                        return;
                    }

                    info!("App exit requested (Cmd+Q) - performing graceful shutdown");
                    api.prevent_exit();

                    // Hide all windows immediately
                    if let Some(window) = app_handle.get_webview_window("main") {
                        let _ = window.hide();
                    }

                    lifecycle::request_shutdown(app_handle);
                }
                // Dock icon clicked with no window on screen: bring it back
                // rather than leaving the click doing nothing.
                #[cfg(target_os = "macos")]
                tauri::RunEvent::Reopen { .. } => {
                    if let Err(e) = tray::window::show_main(app_handle) {
                        error!(error = %e, "Failed to reopen main window");
                    }
                }
                tauri::RunEvent::Exit => {
                    // This is called after ExitRequested completes, or if the process exits unexpectedly
                    info!("App exiting");
                }
                _ => {}
            }
        });
}

/// Whether closing the window should hide to the tray instead of quitting.
///
/// Read at close time rather than cached at startup, so toggling the setting
/// takes effect on the very next close. Read through the daemon: settings
/// belong to the backend, and the backend is the daemon now.
async fn close_to_tray_enabled(app: &tauri::AppHandle) -> bool {
    let state: tauri::State<AppState> = app.state();
    match state.daemon.settings().await {
        Ok(settings) => settings.close_to_tray == Some(true),
        Err(e) => {
            // Falling back to quitting keeps the historical behaviour; a
            // hidden window with no way back would be worse.
            error!(error = %e, "Could not read close_to_tray; quitting instead");
            false
        }
    }
}

/// Application setup hook.
fn setup_app(app: &mut tauri::App) -> Result<(), Box<dyn std::error::Error>> {
    let handle = app.handle().clone();

    // Build the tray before the menu: on Linux and Windows it is the only
    // persistent UI once the window is hidden.
    let tray_available = match tray::Tray::build(&handle) {
        Ok(tray) => {
            info!("System tray initialized");
            let state: tauri::State<AppState> = app.state();
            let slot = state.tray.clone();
            tauri::async_runtime::spawn(async move {
                *slot.write().await = Some(tray);
            });
            true
        }
        Err(e) => {
            // A missing tray is a degraded app, not a broken one: the window
            // still works, so log and carry on rather than refusing to start.
            error!(error = %e, "Failed to build system tray");
            false
        }
    };

    // Before the panel is ever shown, and on the main thread: layer-shell can
    // only claim a GTK window that has not been realized yet, and the panel is
    // declared hidden precisely so it still qualifies here.
    if let Some(panel) = handle.get_webview_window(tray::window::PANEL_LABEL)
        && !tray::placement::prepare(&panel)
    {
        debug!("Tray panel placement left to the compositor");
    }

    // The main window is declared hidden, so something has to show it.
    // After the tray build, never before: whether there is a tray icon decides
    // whether staying hidden is recoverable.
    tauri::async_runtime::block_on(autostart::apply_initial_visibility(&handle, tray_available));

    // Initial paint for every surface. Runs on all platforms: even where
    // there is no application menu, the tray still needs its first sync.
    {
        let handle_for_sync = handle.clone();
        tauri::async_runtime::spawn(async move {
            tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;
            let state: tauri::State<AppState> = handle_for_sync.state();
            sync_all_state_logged(&handle_for_sync, &state).await;
        });
    }

    // Open devtools for debugging (Tauri 2.x always includes devtools in debug builds)
    #[cfg(debug_assertions)]
    {
        if let Some(window) = app.get_webview_window("main") {
            window.open_devtools();
            info!("DevTools opened for debugging");
        }
    }

    #[cfg(target_os = "macos")]
    {
        // Build and attach the application menu (macOS only)
        match menu::build_app_menu(&handle) {
            Ok((menu_obj, app_menu)) => {
                if let Err(e) = app.set_menu(menu_obj) {
                    error!(error = %e, "Failed to set app menu");
                } else {
                    info!("Application menu initialized");
                }

                // Store menu references for state updates
                let state: tauri::State<AppState> = app.state();
                let menu_arc = state.menu.clone();

                tauri::async_runtime::spawn(async move {
                    *menu_arc.write().await = Some(app_menu);
                });
            }
            Err(e) => {
                error!(error = %e, "Failed to build app menu");
            }
        }
    }

    #[cfg(not(target_os = "macos"))]
    {
        // Explicitly attach an empty menu on non-macOS to prevent any default
        // File/Edit/Window-style menu from being shown by the platform.
        match Menu::<Wry>::with_items(&handle, &[]) {
            Ok(empty_menu) => {
                if let Err(e) = app.set_menu(empty_menu) {
                    error!(error = %e, "Failed to set empty app menu");
                } else {
                    info!("Empty application menu attached (non-macOS)");
                }
            }
            Err(e) => {
                error!(error = %e, "Failed to build empty app menu");
            }
        }
    }

    Ok(())
}
