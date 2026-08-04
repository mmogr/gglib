// Prevents additional console window on Windows in release, DO NOT REMOVE!!
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod app;
mod autostart;
mod commands;
mod dock;
mod lifecycle;
mod menu;
mod proxy_actions;
mod tray;

use app::AppState;
use app::events::{emit_or_log, names};
use dotenvy::dotenv;
use gglib_axum::embedded::{EmbeddedServerConfig, start_embedded_server};
use gglib_download::cli_exec::preflight_fast_helper;
use gglib_runtime::process::get_log_manager;
use gglib_tauri::bootstrap::{TauriConfig, bootstrap};
use menu::state_sync::sync_all_state_logged;
use std::sync::Arc;
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
            // Bootstrap inside setup() where we have AppHandle for real event emission
            let config = TauriConfig::with_defaults()
                .expect("Failed to create Tauri config");

            let app_handle = app.handle().clone();
            let ctx = tauri::async_runtime::block_on(async {
                bootstrap(config, app_handle).await
            }).expect("Failed to bootstrap application");

            // Shared with AppState so proxy changes driven from the tray
            // broadcast the same lifecycle events an HTTP call would.
            let sse = Arc::new(gglib_axum::sse::SseBroadcaster::with_defaults());

            // Build AxumContext for the embedded server using the 7 domain ops from ctx
            let axum_ctx = gglib_axum::AxumContext {
                models: ctx.models.clone(),
                servers: ctx.servers.clone(),
                downloads: ctx.downloads.clone(),
                settings: ctx.settings.clone(),
                mcp_ops: ctx.mcp_ops.clone(),
                proxy: ctx.proxy.clone(),
                setup: ctx.setup.clone(),
                core: ctx.app.clone(),
                mcp: ctx.mcp.clone(),
                hf_client: ctx.hf_client.clone(),
                sse: sse.clone(),
                http_client: reqwest::Client::new(),
                agent_semaphore: Arc::new(tokio::sync::Semaphore::new(4)),
                approval_registry: ctx.approval_registry.clone(),
                council_repo: ctx.council_repo.clone(),
                bench_repo: ctx.bench_repo.clone(),
                benchmark: ctx.benchmark.clone(),
                steering_note_queues: Arc::new(tokio::sync::Mutex::new(std::collections::HashMap::new())),
                runtime: ctx.runtime.clone(),
                catalog: ctx.catalog.clone(),
            };

            // Start embedded API server with auth and ephemeral port
            let config = EmbeddedServerConfig {
                cors_origins: gglib_axum::embedded::default_embedded_cors_origins(),
            };

            let (embedded_api, server_handle) = tauri::async_runtime::block_on(async {
                start_embedded_server(axum_ctx, config)
                    .await
                    .expect("Failed to start embedded API server")
            });

            // Create and manage app state
            let app_state = AppState::new(
                ctx.servers.clone(),
                ctx.downloads.clone(),
                ctx.proxy.clone(),
                ctx.app.clone(),
                embedded_api,
                sse,
            );

            // Store the embedded server handle for cleanup
            {
                let tasks = app_state.background_tasks.clone();
                tauri::async_runtime::block_on(async move {
                    tasks.write().await.embedded_server = Some(tauri::async_runtime::JoinHandle::Tokio(server_handle));
                });
            }

            app.manage(app_state);

            // Download system init: preflight the Python fast downloader helper.
            // This runs on startup so the frontend can render a clear error state
            // instead of waiting indefinitely if Python is broken/missing.
            {
                let app_handle_for_init = app.handle().clone();
                tauri::async_runtime::spawn(async move {
                    match preflight_fast_helper().await {
                        Ok(python_exe) => {
                            info!(python = %python_exe, "Fast download helper preflight OK");
                            emit_or_log(&app_handle_for_init, names::DOWNLOAD_SYSTEM_READY, true);
                        }
                        Err(e) => {
                            error!(error = %e, "Fast download helper preflight failed");
                            let msg = format!(
                                "Fast downloads are unavailable: {e}. Please install Python 3 (python3) or set {} to a working interpreter.",
                                "GGLIB_PYTHON"
                            );
                            emit_or_log(
                                &app_handle_for_init,
                                names::DOWNLOAD_SYSTEM_ERROR,
                                gglib_tauri::events::DownloadSystemErrorPayload { message: msg },
                            );
                        }
                    }
                });
            }

            // Perform startup orphan cleanup
            tauri::async_runtime::block_on(lifecycle::startup_cleanup());

            // Continue with rest of setup
            setup_app(app)?;

            // Apply the always-on proxy settings once the window exists, so a
            // slow or failing start delays nothing the user is waiting on.
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
                        // Nothing is torn down here on purpose: the embedded
                        // API server has to outlive the window, or the tray
                        // panel would be left talking to a dead port.
                        //
                        // The window is already hidden, so dropping out of the
                        // Dock now leaves gglib living entirely in the menu
                        // bar rather than looking like an app with no windows.
                        dock::hide(&app_handle);
                        info!("Window closed to tray - proxy left running");
                        return;
                    }

                    info!("Window close requested - performing graceful shutdown");
                    lifecycle::request_shutdown(&app_handle);
                });
            }
            // Dismiss the panel when it loses focus, the way a menu does.
            tauri::WindowEvent::Focused(false)
                if window.label() == tray::window::PANEL_LABEL =>
            {
                let _ = window.hide();
            }
            _ => {}
        })
        ;

    #[cfg(target_os = "macos")]
    let builder = builder.on_menu_event(menu::handlers::handle_menu_event);

    builder
        .invoke_handler(tauri::generate_handler![
            // API discovery
            commands::util::get_embedded_api_info,
            // TRANSPORT_EXCEPTION: Desktop log snapshot (web uses HTTP)
            commands::util::get_server_logs,
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
                    // through here. Preventing that one is what used to strand
                    // the app: alive, but with its embedded API server already
                    // aborted, so every window's HTTP call failed from then on.
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
/// takes effect on the very next close.
async fn close_to_tray_enabled(app: &tauri::AppHandle) -> bool {
    let state: tauri::State<AppState> = app.state();
    match state.core.settings().get().await {
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

    // The main window is declared hidden, so something has to show it. Done
    // here rather than in the `autostart::apply` task below because that one
    // also starts the proxy, and the window should not wait behind that.
    //
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

    // Spawn server log event emitter
    let app_handle = app.handle().clone();
    let state: tauri::State<AppState> = app.state();
    let tasks = state.background_tasks.clone();

    let log_task = tauri::async_runtime::spawn(async move {
        let log_manager = get_log_manager();
        let mut receiver = log_manager.subscribe();

        loop {
            match receiver.recv().await {
                Ok(entry) => {
                    emit_or_log(&app_handle, names::SERVER_LOG, &entry);
                }
                Err(tokio::sync::broadcast::error::RecvError::Lagged(n)) => {
                    debug!(skipped = %n, "Server log receiver lagged");
                }
                Err(tokio::sync::broadcast::error::RecvError::Closed) => {
                    debug!("Server log channel closed");
                    break;
                }
            }
        }
    });

    // Store the log task handle for cleanup
    tasks.blocking_write().log_emitter = Some(log_task);

    // NOTE: Download events are now wired via AppEventBridge in bootstrap()
    // The TauriEventEmitter broadcasts DownloadEvent to the frontend automatically

    // Emit server:snapshot on app init to seed frontend registry
    {
        let state: tauri::State<AppState> = app.state();
        let servers = state.servers.clone();

        tauri::async_runtime::spawn(async move {
            servers.emit_initial_snapshot().await;
        });
    }

    Ok(())
}
