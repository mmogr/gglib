// Prevents additional console window on Windows in release, DO NOT REMOVE!!
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod app;
mod commands;
mod menu;

use app::events::{emit_or_log, names};
use app::AppState;
use dotenvy::dotenv;
use gglib::download::DownloadEvent;
use gglib::services::gui_backend::GuiBackend;
use gglib::utils::process::log_streamer::get_log_manager;
use gglib::utils::process::{ServerEvent, ServerStateInfo, ServerStatus};
use menu::state_sync::sync_menu_state_or_log;
use std::sync::Arc;
use tauri::Manager;
use tracing::{debug, error, info, warn};

#[tokio::main]
async fn main() {
    let _ = dotenv();

    // Initialize tracing/logging for the Tauri GUI
    // Priority: RUST_LOG env var > default (warn)
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("warn")),
        )
        .try_init()
        .ok(); // Ignore error if already initialized

    // Initialize the shared backend (same as Web GUI!)
    let backend = Arc::new(
        GuiBackend::new(9000, 5)
            .await
            .expect("Failed to initialize backend"),
    );

    let embedded_api_port = std::env::var("GGLIB_GUI_API_PORT")
        .ok()
        .and_then(|value| value.parse::<u16>().ok())
        .unwrap_or(8888);

    let app_state = AppState::new(backend.clone(), embedded_api_port);

    // Start embedded API server for chat functionality
    let backend_for_server = backend.clone();
    tokio::spawn(async move {
        if let Err(e) = start_embedded_api_server(backend_for_server, embedded_api_port).await {
            error!(error = %e, "Failed to start embedded API server");
        }
    });

    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .manage(app_state)
        .setup(|app| {
            setup_app(app)?;
            Ok(())
        })
        .on_window_event(|window, event| {
            if let tauri::WindowEvent::CloseRequested { .. } = event {
                info!("Window close requested - cleaning up download processes");
                let app_handle = window.app_handle();
                let state: tauri::State<AppState> = app_handle.state();
                let killed = state.backend.core().downloads().kill_all_processes_sync();
                if killed > 0 {
                    info!(count = killed, "Killed download processes on window close");
                }
            }
        })
        .on_menu_event(menu::handlers::handle_menu_event)
        .invoke_handler(tauri::generate_handler![
            // Models
            commands::list_models,
            commands::add_model,
            commands::remove_model,
            commands::update_model,
            // Servers
            commands::serve_model,
            commands::stop_server,
            commands::list_servers,
            commands::get_server_logs,
            commands::clear_server_logs,
            // Downloads
            commands::download_model,
            commands::cancel_download,
            commands::queue_download,
            commands::get_download_queue,
            commands::remove_from_download_queue,
            commands::reorder_download_queue,
            commands::cancel_shard_group,
            commands::clear_failed_downloads,
            // HuggingFace
            commands::browse_hf_models,
            commands::get_hf_quantizations,
            commands::get_hf_tool_support,
            commands::search_models,
            // Tags
            commands::list_tags,
            commands::get_model_filter_options,
            commands::add_model_tag,
            commands::remove_model_tag,
            commands::get_model_tags,
            // Proxy
            commands::start_proxy,
            commands::stop_proxy,
            commands::get_proxy_status,
            // Settings
            commands::get_settings,
            commands::update_settings,
            // Llama
            commands::check_llama_status,
            commands::install_llama,
            // MCP
            commands::add_mcp_server,
            commands::list_mcp_servers,
            commands::update_mcp_server,
            commands::remove_mcp_server,
            commands::start_mcp_server,
            commands::stop_mcp_server,
            commands::list_mcp_tools,
            commands::call_mcp_tool,
            // Utility
            commands::open_url,
            commands::get_gui_api_port,
            commands::set_selected_model,
            commands::sync_menu_state,
        ])
        .build(tauri::generate_context!())
        .expect("error while building tauri application")
        .run(|app_handle, event| {
            if let tauri::RunEvent::Exit = event {
                info!("App exiting - final cleanup of download processes");
                let state: tauri::State<AppState> = app_handle.state();
                let killed = state.backend.core().downloads().kill_all_processes_sync();
                if killed > 0 {
                    info!(count = killed, "Killed download processes on app exit");
                }
            }
        });
}

/// Application setup hook.
fn setup_app(app: &mut tauri::App) -> Result<(), Box<dyn std::error::Error>> {
    let handle = app.handle().clone();

    // Build and attach the application menu
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

    // Spawn server log event emitter
    let app_handle = app.handle().clone();
    tauri::async_runtime::spawn(async move {
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

    // Wire download events to Tauri events + completion handler
    {
        let state: tauri::State<AppState> = app.state();
        let backend = state.backend.clone();
        let backend_for_callback = backend.clone();
        let app_handle = app.handle().clone();

        backend
            .core()
            .downloads()
            .set_event_callback(Arc::new(move |event: DownloadEvent| {
                // Handle download completion: register model in database
                if let DownloadEvent::DownloadCompleted { id, .. } = &event {
                    backend_for_callback.core().handle_download_completed(id);
                }

                // Emit canonical DownloadEvent directly to frontend
                emit_or_log(&app_handle, names::DOWNLOAD_PROGRESS, &event);
            }));
    }

    // Emit server:snapshot on app init to seed frontend registry
    {
        let state: tauri::State<AppState> = app.state();
        let backend = state.backend.clone();
        let app_handle = app.handle().clone();

        tauri::async_runtime::spawn(async move {
            tokio::time::sleep(tokio::time::Duration::from_millis(200)).await;

            match backend.list_servers().await {
                Ok(servers) => {
                    let server_states: Vec<ServerStateInfo> = servers
                        .iter()
                        .map(|s| ServerStateInfo::new(s.model_id, ServerStatus::Running, Some(s.port)))
                        .collect();

                    let snapshot = ServerEvent::snapshot(server_states);
                    emit_or_log(&app_handle, names::SERVER_SNAPSHOT, &snapshot);
                    debug!(count = servers.len(), "Emitted server:snapshot event");
                }
                Err(e) => {
                    warn!(error = %e, "Failed to list servers for snapshot");
                }
            }
        });
    }

    Ok(())
}

/// Start a minimal embedded API server for chat functionality.
async fn start_embedded_api_server(
    backend: Arc<GuiBackend>,
    port: u16,
) -> Result<(), Box<dyn std::error::Error>> {
    use gglib::commands::gui_web::{routes, state::AppState as WebAppState};
    use std::net::SocketAddr;

    let state = Arc::new(WebAppState::new(backend));
    let app = routes::api_routes(state);

    let addr = SocketAddr::from(([127, 0, 0, 1], port));
    println!("🌐 Embedded API server listening on http://localhost:{}", port);

    let listener = tokio::net::TcpListener::bind(addr).await?;
    axum::serve(listener, app).await?;

    Ok(())
}
