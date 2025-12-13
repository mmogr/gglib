// Prevents additional console window on Windows in release, DO NOT REMOVE!!
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod app;
mod commands;
mod menu;

use app::events::{emit_or_log, names};
use app::AppState;
use dotenvy::dotenv;
use gglib_core::services::AppCore;
use gglib_gui::GuiBackend;
use gglib_mcp::McpService;
use gglib_runtime::process::get_log_manager;
use gglib_tauri::bootstrap::{TauriConfig, bootstrap};
use menu::state_sync::sync_menu_state_or_log;
use std::sync::Arc;
use tauri::Manager;
use tracing::{debug, error, info};

fn main() {
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

    // Embedded API port from env or default
    let embedded_api_port = std::env::var("GGLIB_GUI_API_PORT")
        .ok()
        .and_then(|value| value.parse::<u16>().ok())
        .unwrap_or(8888);

    tauri::Builder::default()
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
            let mcp = ctx.mcp.clone();
            let core = ctx.app.clone();

            // Create and manage app state
            let app_state = AppState::new(core.clone(), gui.clone(), mcp.clone(), embedded_api_port);
            app.manage(app_state);

            // Continue with rest of setup
            setup_app(app)?;

            // Start embedded API server
            let gui_for_server = gui.clone();
            let core_for_server = core.clone();
            let mcp_for_server = mcp.clone();
            tauri::async_runtime::spawn(async move {
                if let Err(e) = start_embedded_api_server(
                    gui_for_server,
                    core_for_server,
                    mcp_for_server,
                    embedded_api_port,
                )
                .await
                {
                    error!(error = %e, "Failed to start embedded API server");
                    // TODO: Show error dialog to user
                }
            });

            Ok(())
        })
        .on_window_event(|window, event| {
            if let tauri::WindowEvent::CloseRequested { .. } = event {
                info!("Window close requested - cleaning up download processes");
                let app_handle = window.app_handle();
                let state: tauri::State<AppState> = app_handle.state();
                let gui = state.gui.clone();
                
                // Best-effort cancellation from sync context
                tauri::async_runtime::block_on(async {
                    gui.cancel_all_downloads().await;
                });
                info!("Download cancellation completed on window close");
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
            commands::search_hf_models,
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
            commands::get_models_directory,
            commands::set_models_directory,
            commands::get_system_memory,
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
            // Chat
            commands::list_conversations,
            commands::create_conversation,
            commands::get_conversation,
            commands::update_conversation,
            commands::delete_conversation,
            commands::get_messages,
            commands::save_message,
            commands::update_message,
            commands::delete_message,
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
                let gui = state.gui.clone();
                
                // Best-effort cancellation from sync context
                tauri::async_runtime::block_on(async {
                    gui.cancel_all_downloads().await;
                });
                info!("Download cancellation completed on app exit");
            }
        });
}

/// Application setup hook.
fn setup_app(app: &mut tauri::App) -> Result<(), Box<dyn std::error::Error>> {
    let handle = app.handle().clone();

    // Open devtools for debugging (available when "devtools" feature is enabled)
    #[cfg(feature = "devtools")]
    {
        if let Some(window) = app.get_webview_window("main") {
            window.open_devtools();
            info!("DevTools opened for debugging");
        }
    }

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

/// Start embedded Axum API server for Tauri.
///
/// This provides HTTP endpoints at `http://localhost:{port}/api/*` for:
/// - Chat history CRUD (`/api/conversations`, `/api/messages`)
/// - Chat streaming (`/api/chat`) - required by useGglibRuntime
///
/// Other API endpoints (downloads, HF, servers, etc.) are handled via Tauri IPC.
async fn start_embedded_api_server(
    gui: Arc<GuiBackend>,
    core: Arc<AppCore>,
    _mcp: Arc<McpService>, // Kept for API compatibility, not used
    port: u16,
) -> Result<(), Box<dyn std::error::Error>> {
    use gglib_axum::chat_api::{ChatApiContext, chat_routes};
    use std::net::SocketAddr;
    use tower_http::cors::{Any, CorsLayer};

    // Create minimal ChatApiContext - no need for full AxumContext
    let chat_state = Arc::new(ChatApiContext { core, gui });

    // Build chat-only router with permissive CORS for localhost
    let cors = CorsLayer::new()
        .allow_origin(Any)
        .allow_methods(Any)
        .allow_headers(Any);

    let app = chat_routes(chat_state).layer(cors);

    let addr = SocketAddr::from(([127, 0, 0, 1], port));
    info!(port = port, "Starting embedded chat API server");

    match tokio::net::TcpListener::bind(addr).await {
        Ok(listener) => {
            info!(port = port, "✓ Embedded chat API listening at http://localhost:{}", port);
            axum::serve(listener, app).await?;
        }
        Err(e) => {
            error!(port = port, error = %e, "Failed to bind embedded chat API server");
            return Err(Box::new(e));
        }
    }

    Ok(())
}
