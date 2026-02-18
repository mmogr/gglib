// Prevents additional console window on Windows in release, DO NOT REMOVE!!
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod app;
mod commands;
mod lifecycle;
mod menu;

use app::AppState;
use app::events::{emit_or_log, names};
use dotenvy::dotenv;
use gglib_axum::embedded::{EmbeddedServerConfig, start_embedded_server};
use gglib_download::cli_exec::preflight_fast_helper;
use gglib_runtime::process::get_log_manager;
use gglib_tauri::bootstrap::{TauriConfig, bootstrap};
#[cfg(target_os = "macos")]
use menu::state_sync::sync_menu_state_or_log;
use std::sync::Arc;
use tauri::Manager;
#[cfg(not(target_os = "macos"))]
use tauri::Wry;
#[cfg(not(target_os = "macos"))]
use tauri::menu::Menu;
use tracing::{debug, error, info};

/// Initialize tracing with file appender for persistent logs.
///
/// Logs are written to:
/// - stdout (for console viewing)
/// - {data_dir}/logs/gglib-{date}.log (daily rotation via tracing-appender)
///
/// Log level is controlled by RUST_LOG environment variable (default: warn).
fn init_tracing() {
    use tracing_subscriber::layer::SubscriberExt;
    use tracing_subscriber::util::SubscriberInitExt;

    // Get data directory for logs
    let log_dir = match gglib_core::paths::data_root() {
        Ok(root) => root.join("logs"),
        Err(e) => {
            eprintln!("Failed to get data root for logs: {}", e);
            // Fallback to current directory
            std::path::PathBuf::from(".")
        }
    };

    // Create log directory if it doesn't exist
    if let Err(e) = std::fs::create_dir_all(&log_dir) {
        eprintln!("Failed to create log directory: {}", e);
    }

    // Configure daily rotating file appender
    let file_appender = tracing_appender::rolling::daily(&log_dir, "gglib");
    let (non_blocking, _guard) = tracing_appender::non_blocking(file_appender);

    // Configure environment filter
    let env_filter = tracing_subscriber::EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("warn"));

    // Build layered subscriber with both stdout and file output
    tracing_subscriber::registry()
        .with(env_filter)
        .with(
            tracing_subscriber::fmt::layer()
                .with_writer(std::io::stdout)
                .compact(),
        )
        .with(
            tracing_subscriber::fmt::layer()
                .with_writer(non_blocking)
                .with_ansi(false) // No ANSI colors in files
                .compact(),
        )
        .try_init()
        .ok(); // Ignore error if already initialized

    // Store guard in static to prevent early drop
    // Note: This leaks memory but ensures logs persist for app lifetime
    std::mem::forget(_guard);
}

fn main() {
    let _ = dotenv();

    // Initialize tracing/logging for the Tauri GUI with file appender
    // Priority: RUST_LOG env var > default (warn)
    init_tracing();

    info!("Tauri application starting");

    let builder = tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .setup(move |app| {
            // Bootstrap inside setup() where we have AppHandle for real event emission
            let config = TauriConfig::with_defaults()
                .expect("Failed to create Tauri config");

            let app_handle = app.handle().clone();
            let ctx = tauri::async_runtime::block_on(async {
                bootstrap(config, app_handle).await
            }).expect("Failed to bootstrap application");

            // Build GUI backend from context
            let gui = Arc::new(ctx.build_gui_backend());

            // Build AxumContext for the embedded server
            // This mirrors TauriContext but is used by Axum handlers
            let axum_ctx = gglib_axum::AxumContext {
                gui: gui.clone(),
                core: ctx.app.clone(),
                mcp: ctx.mcp.clone(),
                downloads: ctx.downloads.clone(),
                hf_client: ctx.hf_client.clone(),
                runner: ctx.runner.clone(),
                sse: Arc::new(gglib_axum::sse::SseBroadcaster::with_defaults()),
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
            let app_state = AppState::new(gui.clone(), embedded_api);

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

            Ok(())
        })
        .on_window_event(|window, event| {
            if let tauri::WindowEvent::CloseRequested { api, .. } = event {
                info!("Window close requested - performing graceful shutdown");
                api.prevent_close();

                // Hide window immediately so user sees instant feedback
                let _ = window.hide();

                let app_handle = window.app_handle().clone();

                tauri::async_runtime::spawn(async move {
                    let state: tauri::State<AppState> = app_handle.state();
                    lifecycle::perform_shutdown(&state).await;
                    app_handle.exit(0);
                });
            }
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
            // Research logs: file persistence for debugging
            commands::research_logs::init_research_logs,
            commands::research_logs::append_research_log,
            commands::research_logs::get_research_log_path,
            commands::research_logs::list_research_logs,
            // Frontend logging: bridge to Rust tracing
            commands::app_logs::log_from_frontend,
            // Voice mode: OS-specific audio pipeline
            commands::voice::voice_start,
            commands::voice::voice_stop,
            commands::voice::voice_unload,
            commands::voice::voice_status,
            commands::voice::voice_ptt_start,
            commands::voice::voice_ptt_stop,
            commands::voice::voice_speak,
            commands::voice::voice_stop_speaking,
            commands::voice::voice_list_models,
            commands::voice::voice_download_stt_model,
            commands::voice::voice_download_tts_model,
            commands::voice::voice_load_stt,
            commands::voice::voice_load_tts,
            commands::voice::voice_set_mode,
            commands::voice::voice_set_voice,
            commands::voice::voice_set_speed,
            commands::voice::voice_set_auto_speak,
            commands::voice::voice_list_devices,
            commands::voice::voice_download_vad_model,
        ])
        .build(tauri::generate_context!())
        .expect("error while building tauri application")
        .run(|app_handle, event| {
            match event {
                tauri::RunEvent::ExitRequested { api, .. } => {
                    info!("App exit requested (Cmd+Q) - performing graceful shutdown");
                    api.prevent_exit();

                    // Hide all windows immediately
                    if let Some(window) = app_handle.get_webview_window("main") {
                        let _ = window.hide();
                    }

                    let handle_for_exit = app_handle.clone();

                    tauri::async_runtime::spawn(async move {
                        let state: tauri::State<AppState> = handle_for_exit.state();
                        lifecycle::perform_shutdown(&state).await;
                        handle_for_exit.exit(0);
                    });
                }
                tauri::RunEvent::Exit => {
                    // This is called after ExitRequested completes, or if the process exits unexpectedly
                    info!("App exiting");
                }
                _ => {}
            }
        });
}

/// Application setup hook.
fn setup_app(app: &mut tauri::App) -> Result<(), Box<dyn std::error::Error>> {
    let handle = app.handle().clone();

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

                // Perform initial menu state sync
                let handle_clone = handle.clone();
                tauri::async_runtime::spawn(async move {
                    tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;
                    let state: tauri::State<AppState> = handle_clone.state();
                    sync_menu_state_or_log(&handle_clone, &state).await;
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
        let gui = state.gui.clone();

        tauri::async_runtime::spawn(async move {
            gui.emit_initial_snapshot().await;
        });
    }

    Ok(())
}
