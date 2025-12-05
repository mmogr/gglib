// Prevents additional console window on Windows in release, DO NOT REMOVE!!
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod menu;

use dotenvy::dotenv;
use gglib::{
    commands::llama::check_llama_installed,
    download::QueueSnapshot,
    models::gui::{
        AddModelRequest, AppSettings, GuiModel, HfQuantizationsResponse, HfSearchRequest,
        HfSearchResponse, HfToolSupportResponse, RemoveModelRequest, StartServerRequest,
        StartServerResponse, UpdateModelRequest, UpdateSettingsRequest,
    },
    services::core::DownloadError,
    services::gui_backend::GuiBackend,
    services::mcp::{McpServerConfig, McpServerInfo, McpTool, McpToolResult},
    utils::process::{ServerEvent, ServerStateInfo, ServerStatus},
};
use menu::{AppMenu, MenuState};
use std::sync::Arc;
use tauri::{Emitter, Manager};
use tokio::sync::RwLock;
use tracing::{debug, error, info, warn};

// Application state with shared backend
struct AppState {
    backend: Arc<GuiBackend>,
    api_port: u16,
    /// Menu state for dynamic updates
    menu: Arc<RwLock<Option<AppMenu>>>,
    /// Currently selected model ID (for menu state sync)
    selected_model_id: Arc<RwLock<Option<u32>>>,
}

// Tauri commands that use the shared GuiBackend (same as Web GUI!)

#[tauri::command]
async fn list_models(state: tauri::State<'_, AppState>) -> Result<Vec<GuiModel>, String> {
    state
        .backend
        .list_models()
        .await
        .map_err(|e| format!("Failed to list models: {}", e))
}

#[tauri::command]
async fn add_model(file_path: String, state: tauri::State<'_, AppState>) -> Result<String, String> {
    let request = AddModelRequest { file_path };

    state
        .backend
        .add_model(request)
        .await
        .map(|model| format!("Model added successfully: {}", model.name))
        .map_err(|e| format!("Failed to add model: {}", e))
}

#[tauri::command]
async fn remove_model(
    identifier: String,
    force: bool,
    state: tauri::State<'_, AppState>,
) -> Result<String, String> {
    // Parse identifier as u32
    let id: u32 = identifier
        .parse()
        .map_err(|_| format!("Invalid model ID: {}", identifier))?;

    let request = RemoveModelRequest { force };

    state
        .backend
        .remove_model(id, request)
        .await
        .map_err(|e| format!("Failed to remove model: {}", e))
}

#[tauri::command]
async fn update_model(
    id: u32,
    updates: serde_json::Value,
    state: tauri::State<'_, AppState>,
) -> Result<GuiModel, String> {
    let request = UpdateModelRequest {
        name: updates
            .get("name")
            .and_then(|v| v.as_str().map(str::to_string)),
        quantization: updates
            .get("quantization")
            .and_then(|v| v.as_str().map(str::to_string)),
        file_path: updates
            .get("file_path")
            .and_then(|v| v.as_str().map(str::to_string)),
    };

    state
        .backend
        .update_model(id, request)
        .await
        .map_err(|e| format!("Failed to update model: {}", e))
}

#[tauri::command]
async fn serve_model(
    id: u32,
    ctx_size: Option<String>,
    context_length: Option<u64>,
    mlock: bool,
    port: Option<u16>,
    jinja: Option<bool>,
    app: tauri::AppHandle,
    state: tauri::State<'_, AppState>,
) -> Result<StartServerResponse, String> {
    debug!(
        model_id = %id,
        ctx_size = ?ctx_size,
        context_length = ?context_length,
        port = ?port,
        "Serve model command called"
    );

    // Use context_length if provided, otherwise parse ctx_size
    let context_length = if let Some(len) = context_length {
        Some(len)
    } else if let Some(ctx) = ctx_size {
        if ctx == "max" {
            None // Will use model's default
        } else {
            Some(
                ctx.parse::<u64>()
                    .map_err(|_| format!("Invalid context size: {}", ctx))?,
            )
        }
    } else {
        None
    };

    let request = StartServerRequest {
        context_length,
        port,
        mlock,
        jinja,
        reasoning_format: None, // Auto-detect from model tags
    };

    state
        .backend
        .start_server(id, request)
        .await
        .map(|resp| {
            info!(port = %resp.port, "Server started successfully");
            
            // Emit server:running event
            let event = ServerEvent::running(id, resp.port);
            if let Err(e) = app.emit("server:running", &event) {
                error!(error = %e, "Failed to emit server:running event");
            }
            
            resp
        })
        .map_err(|e| {
            error!(error = %e, "Failed to start server");
            format!("Failed to start server: {}", e)
        })
}

#[tauri::command]
async fn stop_server(
    model_id: u32,
    app: tauri::AppHandle,
    state: tauri::State<'_, AppState>,
) -> Result<String, String> {
    // Get the server port before stopping (for event payload)
    let port = state
        .backend
        .list_servers()
        .await
        .ok()
        .and_then(|servers| {
            servers
                .iter()
                .find(|s| s.model_id == model_id)
                .map(|s| s.port)
        });

    // Emit server:stopping event immediately
    let stopping_event = ServerEvent::stopping(model_id, port);
    if let Err(e) = app.emit("server:stopping", &stopping_event) {
        error!(error = %e, "Failed to emit server:stopping event");
    }

    // Actually stop the server
    let result = state
        .backend
        .stop_model(model_id)
        .await
        .map_err(|e| format!("Failed to stop server: {}", e));

    // Emit server:stopped event after successful stop
    if result.is_ok() {
        let stopped_event = ServerEvent::stopped(model_id, port);
        if let Err(e) = app.emit("server:stopped", &stopped_event) {
            error!(error = %e, "Failed to emit server:stopped event");
        }
    }

    result
}

#[tauri::command]
async fn list_servers(
    state: tauri::State<'_, AppState>,
) -> Result<Vec<gglib::utils::ServerInfo>, String> {
    state
        .backend
        .list_servers()
        .await
        .map_err(|e| format!("Failed to list servers: {}", e))
}

// Server log commands
use gglib::utils::process::log_streamer::{ServerLogEntry, get_log_manager};

#[tauri::command]
async fn get_server_logs(port: u16) -> Result<Vec<ServerLogEntry>, String> {
    let log_manager = get_log_manager();
    Ok(log_manager.get_logs(port))
}

#[tauri::command]
async fn clear_server_logs(port: u16) -> Result<(), String> {
    let log_manager = get_log_manager();
    log_manager.clear_logs(port);
    Ok(())
}

#[tauri::command]
async fn download_model(
    app: tauri::AppHandle,
    model_id: String,
    quantization: Option<String>,
    state: tauri::State<'_, AppState>,
) -> Result<String, String> {
    use gglib::commands::download::ProgressThrottle;
    use gglib::download::DownloadEvent;

    // Emit download started event
    debug!(model_id = %model_id, "Attempting to emit download-progress (starting)");
    if let Err(err) = app.emit(
        "download-progress",
        DownloadEvent::started(&model_id),
    ) {
        error!(error = %err, "Failed to emit start event");
    } else {
        debug!("Successfully emitted start event");
    }

    // Clone for emission in closure
    let model_id_clone = model_id.clone();
    let app_clone = app.clone();
    let model_id_clone2 = model_id.clone();

    // Create progress callback with EWA speed calculation
    let throttle = ProgressThrottle::responsive_ui();
    let callback_throttle = throttle.clone();

    let progress_callback: gglib::commands::download::ProgressCallback =
        Box::new(move |downloaded, total| {
            let Some(speed) = callback_throttle.should_emit_with_speed(downloaded, total) else {
                return;
            };
            let event = DownloadEvent::progress(&model_id_clone, downloaded, total, speed);
            // debug!(downloaded, total, "Emitting progress"); // Commented out to avoid spam
            if let Err(err) = app_clone.emit("download-progress", &event) {
                tracing::error!(error = %err, "Failed to emit progress event");
            }
        });

    // Use the shared backend (DRY!)
    let result = state
        .backend
        .download_model(model_id.clone(), quantization, Some(&progress_callback))
        .await;

    match result {
        Ok(message) => {
            if let Err(err) = app.emit(
                "download-progress",
                DownloadEvent::completed(&model_id_clone2, Some(&message)),
            ) {
                error!(error = %err, "Failed to emit completion event");
            }
            Ok(message)
        }
        Err(e) => {
            let is_cancelled = e.downcast_ref::<DownloadError>().is_some();
            let error_msg = if is_cancelled {
                format!("Download '{}' was cancelled", model_id_clone2)
            } else {
                format!("Failed to download model: {}", e)
            };
            if let Err(err) = app.emit(
                "download-progress",
                DownloadEvent::failed(&model_id_clone2, &error_msg),
            ) {
                error!(error = %err, "Failed to emit error event");
            }
            Err(error_msg)
        }
    }
}

#[tauri::command]
async fn cancel_download(
    model_id: String,
    state: tauri::State<'_, AppState>,
) -> Result<String, String> {
    state
        .backend
        .cancel_download(&model_id)
        .await
        .map(|_| format!("Download '{}' cancelled", model_id))
        .map_err(|e| format!("Failed to cancel download: {}", e))
}

#[tauri::command]
async fn search_models(
    query: String,
    limit: u32,
    sort: Option<String>,
    gguf_only: bool,
) -> Result<String, String> {
    use gglib::commands;
    commands::download::handle_search(
        query,
        limit,
        sort.unwrap_or_else(|| "downloads".to_string()),
        gguf_only,
    )
    .await
    .map(|_| "Search completed".to_string())
    .map_err(|e| format!("Search failed: {}", e))
}

#[tauri::command]
async fn start_proxy(
    host: String,
    port: u16,
    start_port: u16,
    default_context: u64,
    state: tauri::State<'_, AppState>,
) -> Result<String, String> {
    state
        .backend
        .start_proxy(host, port, start_port, default_context)
        .await
        .map_err(|e| format!("Failed to start proxy: {}", e))
}

#[tauri::command]
async fn stop_proxy(state: tauri::State<'_, AppState>) -> Result<String, String> {
    state
        .backend
        .stop_proxy()
        .await
        .map_err(|e| format!("Failed to stop proxy: {}", e))
}

#[tauri::command]
async fn get_proxy_status(state: tauri::State<'_, AppState>) -> Result<serde_json::Value, String> {
    state
        .backend
        .get_proxy_status()
        .await
        .map_err(|e| format!("Failed to get proxy status: {}", e))
}

// Tag Management Commands

#[tauri::command]
async fn list_tags(state: tauri::State<'_, AppState>) -> Result<Vec<String>, String> {
    state
        .backend
        .list_tags()
        .await
        .map_err(|e| format!("Failed to list tags: {}", e))
}

#[tauri::command]
async fn get_model_filter_options(
    state: tauri::State<'_, AppState>,
) -> Result<gglib::services::database::ModelFilterOptions, String> {
    state
        .backend
        .get_model_filter_options()
        .await
        .map_err(|e| format!("Failed to get filter options: {}", e))
}

#[tauri::command]
async fn add_model_tag(
    model_id: u32,
    tag: String,
    state: tauri::State<'_, AppState>,
) -> Result<String, String> {
    state
        .backend
        .add_model_tag(model_id, tag)
        .await
        .map(|_| "Tag added to model successfully".to_string())
        .map_err(|e| format!("Failed to add tag to model: {}", e))
}

#[tauri::command]
async fn remove_model_tag(
    model_id: u32,
    tag: String,
    state: tauri::State<'_, AppState>,
) -> Result<String, String> {
    state
        .backend
        .remove_model_tag(model_id, tag)
        .await
        .map(|_| "Tag removed from model successfully".to_string())
        .map_err(|e| format!("Failed to remove tag from model: {}", e))
}

#[tauri::command]
async fn get_model_tags(
    model_id: u32,
    state: tauri::State<'_, AppState>,
) -> Result<Vec<String>, String> {
    state
        .backend
        .get_model_tags(model_id)
        .await
        .map_err(|e| format!("Failed to get model tags: {}", e))
}

#[tauri::command]
async fn get_settings(state: tauri::State<'_, AppState>) -> Result<AppSettings, String> {
    state
        .backend
        .get_settings()
        .await
        .map_err(|e| format!("Failed to get settings: {}", e))
}

#[tauri::command]
async fn update_settings(
    updates: UpdateSettingsRequest,
    state: tauri::State<'_, AppState>,
) -> Result<AppSettings, String> {
    state
        .backend
        .update_settings(updates)
        .await
        .map_err(|e| format!("Failed to update settings: {}", e))
}

// Download Queue Commands

/// Response for queue_download command
#[derive(serde::Serialize)]
struct QueueDownloadResponse {
    position: usize,
    shard_count: usize,
}

#[tauri::command]
async fn queue_download(
    _app: tauri::AppHandle,
    model_id: String,
    quantization: Option<String>,
    state: tauri::State<'_, AppState>,
) -> Result<QueueDownloadResponse, String> {
    // Add to queue (auto-detects and handles sharded models)
    let (position, shard_count) = state
        .backend
        .queue_download(model_id.clone(), quantization)
        .await
        .map_err(|e| format!("Failed to queue download: {}", e))?;

    // Start the queue processor in a background task (if not already running)
    if state.backend.core().start_queue_if_idle() {
        let backend = state.backend.clone();
        tokio::spawn(async move {
            // process_queue runs until queue is empty, handles progress internally via on_event
            let _ = backend.core().downloads().process_queue().await;
            // Mark idle when done so future queues can start
            backend.core().mark_queue_idle();
        });
    }

    Ok(QueueDownloadResponse {
        position,
        shard_count,
    })
}

#[tauri::command]
async fn get_download_queue(
    state: tauri::State<'_, AppState>,
) -> Result<QueueSnapshot, String> {
    Ok(state.backend.get_download_queue().await)
}

#[tauri::command]
async fn remove_from_download_queue(
    model_id: String,
    state: tauri::State<'_, AppState>,
) -> Result<String, String> {
    state
        .backend
        .remove_from_download_queue(&model_id)
        .await
        .map(|_| format!("Removed '{}' from download queue", model_id))
        .map_err(|e| format!("Failed to remove from queue: {}", e))
}

#[tauri::command]
async fn reorder_download_queue(
    model_id: String,
    new_position: usize,
    state: tauri::State<'_, AppState>,
) -> Result<usize, String> {
    state
        .backend
        .reorder_download_queue(&model_id, new_position)
        .await
        .map_err(|e| format!("Failed to reorder queue: {}", e))
}

#[tauri::command]
async fn cancel_shard_group(
    group_id: String,
    state: tauri::State<'_, AppState>,
) -> Result<String, String> {
    state
        .backend
        .cancel_shard_group(&group_id)
        .await
        .map(|_| format!("Cancelled shard group '{}'", group_id))
        .map_err(|e| format!("Failed to cancel shard group: {}", e))
}

#[tauri::command]
async fn clear_failed_downloads(state: tauri::State<'_, AppState>) -> Result<String, String> {
    state.backend.clear_failed_downloads().await;
    Ok("Cleared failed downloads".to_string())
}

// HuggingFace Browser Commands

#[tauri::command]
async fn browse_hf_models(
    request: HfSearchRequest,
    state: tauri::State<'_, AppState>,
) -> Result<HfSearchResponse, String> {
    state
        .backend
        .browse_hf_models(request)
        .await
        .map_err(|e| format!("Failed to browse HuggingFace models: {}", e))
}

#[tauri::command]
async fn get_hf_quantizations(
    model_id: String,
    state: tauri::State<'_, AppState>,
) -> Result<HfQuantizationsResponse, String> {
    state
        .backend
        .get_model_quantizations(&model_id)
        .await
        .map_err(|e| format!("Failed to get quantizations: {}", e))
}

#[tauri::command]
async fn get_hf_tool_support(
    model_id: String,
    state: tauri::State<'_, AppState>,
) -> Result<HfToolSupportResponse, String> {
    state
        .backend
        .get_hf_tool_support(&model_id)
        .await
        .map_err(|e| format!("Failed to get tool support info: {}", e))
}

/// Check if llama.cpp is installed
#[tauri::command]
fn check_llama_status() -> Result<LlamaStatus, String> {
    use gglib::commands::llama::{
        check_llama_installed, check_prebuilt_availability, PrebuiltAvailability,
    };

    let installed = check_llama_installed();
    let can_download = matches!(
        check_prebuilt_availability(),
        PrebuiltAvailability::Available { .. }
    );

    Ok(LlamaStatus {
        installed,
        can_download,
    })
}

/// Response for check_llama_status
#[derive(serde::Serialize)]
struct LlamaStatus {
    installed: bool,
    can_download: bool,
}

/// Install llama.cpp by downloading pre-built binaries
#[tauri::command]
async fn install_llama(app: tauri::AppHandle) -> Result<String, String> {
    use gglib::commands::download::ProgressThrottle;
    use gglib::commands::llama::{
        check_prebuilt_availability, download_prebuilt_binaries_with_boxed_callback,
        PrebuiltAvailability,
    };
    use std::sync::Arc;

    // Check if pre-built binaries are available
    match check_prebuilt_availability() {
        PrebuiltAvailability::Available { description, .. } => {
            // Emit started event
            let _ = app.emit(
                "llama-install-progress",
                LlamaInstallEvent {
                    status: "started".to_string(),
                    downloaded: 0,
                    total: 0,
                    percentage: 0.0,
                    message: format!("Downloading llama.cpp for {}...", description),
                },
            );

            // Create progress callback (boxed, thread-safe)
            let start_time = std::time::Instant::now();
            let throttle = Arc::new(ProgressThrottle::responsive_ui());
            let app_clone = app.clone();

            let progress_callback: Box<dyn Fn(u64, u64) + Send + Sync> =
                Box::new(move |downloaded: u64, total: u64| {
                    if !throttle.should_emit(downloaded, total) {
                        return;
                    }
                    let elapsed = start_time.elapsed().as_secs_f64();
                    let percentage = if total > 0 {
                        (downloaded as f64 / total as f64) * 100.0
                    } else {
                        0.0
                    };
                    let speed = if elapsed > 0.0 {
                        downloaded as f64 / elapsed
                    } else {
                        0.0
                    };
                    let eta = if speed > 0.0 && total > downloaded {
                        (total - downloaded) as f64 / speed
                    } else {
                        0.0
                    };

                    let _ = app_clone.emit(
                        "llama-install-progress",
                        LlamaInstallEvent {
                            status: "downloading".to_string(),
                            downloaded,
                            total,
                            percentage,
                            message: format!(
                                "Downloading... {:.1}% ({:.1} MB/s, {:.0}s remaining)",
                                percentage,
                                speed / 1_000_000.0,
                                eta
                            ),
                        },
                    );
                });

            // Download with progress
            match download_prebuilt_binaries_with_boxed_callback(progress_callback).await {
                Ok(()) => {
                    let _ = app.emit(
                        "llama-install-progress",
                        LlamaInstallEvent {
                            status: "completed".to_string(),
                            downloaded: 0,
                            total: 0,
                            percentage: 100.0,
                            message: "llama.cpp installed successfully!".to_string(),
                        },
                    );
                    Ok("llama.cpp installed successfully".to_string())
                }
                Err(e) => {
                    let error_msg = format!("Failed to install llama.cpp: {}", e);
                    let _ = app.emit(
                        "llama-install-progress",
                        LlamaInstallEvent {
                            status: "error".to_string(),
                            downloaded: 0,
                            total: 0,
                            percentage: 0.0,
                            message: error_msg.clone(),
                        },
                    );
                    Err(error_msg)
                }
            }
        }
        PrebuiltAvailability::NotAvailable { reason } => Err(format!(
            "Cannot auto-install llama.cpp: {}. Please build from source.",
            reason
        )),
    }
}

/// Progress event for llama installation
#[derive(Clone, serde::Serialize)]
struct LlamaInstallEvent {
    status: String,
    downloaded: u64,
    total: u64,
    percentage: f64,
    message: String,
}

#[tauri::command]
fn get_gui_api_port(state: tauri::State<'_, AppState>) -> u16 {
    state.api_port
}

// =============================================================================
// Utility Commands
// =============================================================================

/// Open a URL in the system's default browser.
/// Used by the frontend to open external links (e.g., HuggingFace model pages).
#[tauri::command]
async fn open_url(url: String) -> Result<(), String> {
    open::that(&url).map_err(|e| format!("Failed to open URL: {}", e))
}

// =============================================================================
// Menu State Synchronization Commands
// =============================================================================

/// Set the currently selected model ID and sync menu state
#[tauri::command]
async fn set_selected_model(
    model_id: Option<u32>,
    app: tauri::AppHandle,
    state: tauri::State<'_, AppState>,
) -> Result<(), String> {
    // Update selected model ID
    *state.selected_model_id.write().await = model_id;

    // Sync menu state
    sync_menu_state_internal(&app, &state).await
}

// =============================================================================
// MCP Server Management Commands
// =============================================================================

/// Add a new MCP server configuration.
#[tauri::command]
async fn add_mcp_server(
    config: McpServerConfig,
    state: tauri::State<'_, AppState>,
) -> Result<McpServerConfig, String> {
    state
        .backend
        .core()
        .mcp()
        .add_server(config)
        .await
        .map_err(|e| format!("Failed to add MCP server: {}", e))
}

/// List all MCP server configurations with their current status.
#[tauri::command]
async fn list_mcp_servers(
    state: tauri::State<'_, AppState>,
) -> Result<Vec<McpServerInfo>, String> {
    state
        .backend
        .core()
        .mcp()
        .list_servers_with_status()
        .await
        .map_err(|e| format!("Failed to list MCP servers: {}", e))
}

/// Update an MCP server configuration.
#[tauri::command]
async fn update_mcp_server(
    id: String,
    config: McpServerConfig,
    state: tauri::State<'_, AppState>,
) -> Result<McpServerConfig, String> {
    state
        .backend
        .core()
        .mcp()
        .update_server(&id, config)
        .await
        .map_err(|e| format!("Failed to update MCP server: {}", e))
}

/// Remove an MCP server configuration.
#[tauri::command]
async fn remove_mcp_server(
    id: String,
    state: tauri::State<'_, AppState>,
) -> Result<(), String> {
    state
        .backend
        .core()
        .mcp()
        .remove_server(&id)
        .await
        .map_err(|e| format!("Failed to remove MCP server: {}", e))
}

/// Start an MCP server.
#[tauri::command]
async fn start_mcp_server(
    id: String,
    state: tauri::State<'_, AppState>,
) -> Result<Vec<McpTool>, String> {
    state
        .backend
        .core()
        .mcp()
        .start_server(&id)
        .await
        .map_err(|e| format!("Failed to start MCP server: {}", e))
}

/// Stop an MCP server.
#[tauri::command]
async fn stop_mcp_server(
    id: String,
    state: tauri::State<'_, AppState>,
) -> Result<(), String> {
    state
        .backend
        .core()
        .mcp()
        .stop_server(&id)
        .await
        .map_err(|e| format!("Failed to stop MCP server: {}", e))
}

/// Get all tools from all running MCP servers.
/// Returns a list of (server_id, tools) pairs.
#[tauri::command]
async fn list_mcp_tools(
    state: tauri::State<'_, AppState>,
) -> Result<Vec<(String, Vec<McpTool>)>, String> {
    Ok(state
        .backend
        .core()
        .mcp()
        .list_all_tools()
        .await)
}

/// Call an MCP tool.
#[tauri::command]
async fn call_mcp_tool(
    server_id: String,
    tool_name: String,
    arguments: serde_json::Value,
    state: tauri::State<'_, AppState>,
) -> Result<McpToolResult, String> {
    // Convert Value to HashMap
    let args_map: std::collections::HashMap<String, serde_json::Value> = 
        if let serde_json::Value::Object(map) = arguments {
            map.into_iter().collect()
        } else {
            std::collections::HashMap::new()
        };
    
    state
        .backend
        .core()
        .mcp()
        .call_tool(&server_id, &tool_name, args_map)
        .await
        .map_err(|e| format!("Failed to call MCP tool: {}", e))
}

/// Sync menu state based on current application state
#[tauri::command]
async fn sync_menu_state(
    app: tauri::AppHandle,
    state: tauri::State<'_, AppState>,
) -> Result<(), String> {
    sync_menu_state_internal(&app, &state).await
}

/// Internal helper to sync menu state (used by commands and event handlers)
async fn sync_menu_state_internal(
    _app: &tauri::AppHandle,
    state: &tauri::State<'_, AppState>,
) -> Result<(), String> {
    let menu_guard = state.menu.read().await;
    let Some(menu) = menu_guard.as_ref() else {
        // Menu not yet initialized, skip
        return Ok(());
    };

    // Gather current state
    let llama_installed = check_llama_installed();

    let proxy_status = state
        .backend
        .get_proxy_status()
        .await
        .map_err(|e| format!("Failed to get proxy status: {}", e))?;
    let proxy_running = proxy_status
        .get("running")
        .and_then(|v| v.as_bool())
        .unwrap_or(false);

    let selected_id = *state.selected_model_id.read().await;
    let model_selected = selected_id.is_some();

    // Check if selected model has a running server
    let selected_model_server_running = if let Some(id) = selected_id {
        let servers = state.backend.list_servers().await.unwrap_or_default();
        servers.iter().any(|s| s.model_id == id)
    } else {
        false
    };

    let menu_state = MenuState {
        llama_installed,
        proxy_running,
        model_selected,
        selected_model_server_running,
    };

    // Update menu items
    menu.sync_state(&menu_state)
        .map_err(|e| format!("Failed to sync menu state: {}", e))?;

    Ok(())
}

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

    // Note: Download event callback is wired in setup() where we have AppHandle

    let embedded_api_port = std::env::var("GGLIB_GUI_API_PORT")
        .ok()
        .and_then(|value| value.parse::<u16>().ok())
        .unwrap_or(8888);

    let app_state = AppState {
        backend: backend.clone(),
        api_port: embedded_api_port,
        menu: Arc::new(RwLock::new(None)),
        selected_model_id: Arc::new(RwLock::new(None)),
    };

    // Start embedded API server for chat functionality
    // This allows the frontend to use the same /api/chat endpoint as the web UI
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
            // Build and attach the application menu
            let handle = app.handle().clone();

            match menu::build_app_menu(&handle) {
                Ok((menu, app_menu)) => {
                    // Attach menu to the app
                    if let Err(e) = app.set_menu(menu) {
                        error!(error = %e, "Failed to set app menu");
                    } else {
                        info!("Application menu initialized");
                    }

                    // Store menu references for state updates
                    let state: tauri::State<AppState> = app.state();
                    let menu_arc = state.menu.clone();

                    // We need to spawn this since we're in a sync context
                    tauri::async_runtime::spawn(async move {
                        *menu_arc.write().await = Some(app_menu);
                    });

                    // Perform initial menu state sync
                    let handle_clone = handle.clone();
                    tauri::async_runtime::spawn(async move {
                        // Small delay to ensure state is initialized
                        tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;

                        let state: tauri::State<AppState> = handle_clone.state();
                        if let Err(e) = sync_menu_state_internal(&handle_clone, &state).await {
                            warn!(error = %e, "Failed to perform initial menu sync");
                        }
                    });
                }
                Err(e) => {
                    error!(error = %e, "Failed to build app menu");
                }
            }

            // Spawn a task to emit server log events
            let app_handle = app.handle().clone();
            tauri::async_runtime::spawn(async move {
                let log_manager = get_log_manager();
                let mut receiver = log_manager.subscribe();
                
                loop {
                    match receiver.recv().await {
                        Ok(entry) => {
                            if let Err(e) = app_handle.emit("server-log", &entry) {
                                debug!(error = %e, "Failed to emit server-log event");
                            }
                        }
                        Err(tokio::sync::broadcast::error::RecvError::Lagged(n)) => {
                            debug!(skipped = %n, "Server log receiver lagged, skipping old messages");
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
                use gglib::download::DownloadEvent;
                
                let state: tauri::State<AppState> = app.state();
                let backend = state.backend.clone();
                let backend_for_callback = backend.clone();
                let app_handle = app.handle().clone();
                
                backend.core().downloads().set_event_callback(Arc::new(move |event: DownloadEvent| {
                    // Handle download completion: register model in database
                    if let DownloadEvent::DownloadCompleted { id, .. } = &event {
                        backend_for_callback.core().handle_download_completed(id);
                    }
                    
                    // Emit canonical DownloadEvent directly to frontend
                    if let Err(e) = app_handle.emit("download-progress", &event) {
                        error!(error = %e, "Failed to emit download-progress event");
                    }
                }));
            }

            // Emit server:snapshot on app init to seed frontend registry
            {
                let state: tauri::State<AppState> = app.state();
                let backend = state.backend.clone();
                let app_handle = app.handle().clone();
                
                tauri::async_runtime::spawn(async move {
                    // Small delay to ensure frontend is ready
                    tokio::time::sleep(tokio::time::Duration::from_millis(200)).await;
                    
                    match backend.list_servers().await {
                        Ok(servers) => {
                            let server_states: Vec<ServerStateInfo> = servers
                                .iter()
                                .map(|s| ServerStateInfo::new(s.model_id, ServerStatus::Running, Some(s.port)))
                                .collect();
                            
                            let snapshot = ServerEvent::snapshot(server_states);
                            if let Err(e) = app_handle.emit("server:snapshot", &snapshot) {
                                error!(error = %e, "Failed to emit server:snapshot event");
                            } else {
                                debug!(count = servers.len(), "Emitted server:snapshot event");
                            }
                        }
                        Err(e) => {
                            warn!(error = %e, "Failed to list servers for snapshot");
                        }
                    }
                });
            }

            Ok(())
        })
        // Handle window close events - this is more reliable than RunEvent::WindowEvent
        .on_window_event(|window, event| {
            if let tauri::WindowEvent::CloseRequested { .. } = event {
                info!(
                    "Window close requested via on_window_event - cleaning up download processes"
                );
                let app_handle = window.app_handle();
                let state: tauri::State<AppState> = app_handle.state();
                let downloads = state.backend.core().downloads();

                // Synchronously kill all child processes (Python downloaders)
                let killed = downloads.kill_all_processes_sync();
                if killed > 0 {
                    info!(count = killed, "Killed download processes on window close");
                }
            }
        })
        .on_menu_event(handle_menu_event)
        .invoke_handler(tauri::generate_handler![
            list_models,
            add_model,
            remove_model,
            update_model,
            serve_model,
            stop_server,
            list_servers,
            get_server_logs,
            clear_server_logs,
            download_model,
            cancel_download,
            search_models,
            start_proxy,
            stop_proxy,
            get_proxy_status,
            list_tags,
            get_model_filter_options,
            add_model_tag,
            remove_model_tag,
            get_model_tags,
            get_settings,
            update_settings,
            queue_download,
            get_download_queue,
            remove_from_download_queue,
            reorder_download_queue,
            cancel_shard_group,
            clear_failed_downloads,
            browse_hf_models,
            get_hf_quantizations,
            get_hf_tool_support,
            get_gui_api_port,
            check_llama_status,
            install_llama,
            open_url,
            set_selected_model,
            sync_menu_state,
            // MCP server management
            add_mcp_server,
            list_mcp_servers,
            update_mcp_server,
            remove_mcp_server,
            start_mcp_server,
            stop_mcp_server,
            list_mcp_tools,
            call_mcp_tool
        ])
        .build(tauri::generate_context!())
        .expect("error while building tauri application")
        .run(|app_handle, event| {
            // Handle app exit as a final fallback (Cmd+Q, dock quit, etc.)
            // Note: on_window_event handles CloseRequested more reliably
            if let tauri::RunEvent::Exit = event {
                info!("App exiting (RunEvent::Exit) - final cleanup of download processes");
                let state: tauri::State<AppState> = app_handle.state();
                let downloads = state.backend.core().downloads();

                // Synchronously kill all child processes (Python downloaders)
                let killed = downloads.kill_all_processes_sync();
                if killed > 0 {
                    info!(count = killed, "Killed download processes on app exit");
                }
            }
        });
}

// =============================================================================
// Menu Event Handler
// =============================================================================

/// Handle menu item click events
fn handle_menu_event(app: &tauri::AppHandle, event: tauri::menu::MenuEvent) {
    let id = event.id().as_ref();

    debug!(menu_id = %id, "Menu event received");

    match id {
        // File menu
        menu::ids::ADD_MODEL_FILE => {
            // Emit event to frontend to open file dialog
            if let Err(e) = app.emit("menu:add-model-file", ()) {
                error!(error = %e, "Failed to emit add-model-file event");
            }
        }
        menu::ids::DOWNLOAD_MODEL => {
            if let Err(e) = app.emit("menu:show-downloads", ()) {
                error!(error = %e, "Failed to emit show-downloads event");
            }
        }
        menu::ids::REFRESH_MODELS => {
            if let Err(e) = app.emit("menu:refresh-models", ()) {
                error!(error = %e, "Failed to emit refresh-models event");
            }
        }

        // Model menu
        menu::ids::START_SERVER => {
            if let Err(e) = app.emit("menu:start-server", ()) {
                error!(error = %e, "Failed to emit start-server event");
            }
        }
        menu::ids::STOP_SERVER => {
            if let Err(e) = app.emit("menu:stop-server", ()) {
                error!(error = %e, "Failed to emit stop-server event");
            }
        }
        menu::ids::REMOVE_MODEL => {
            if let Err(e) = app.emit("menu:remove-model", ()) {
                error!(error = %e, "Failed to emit remove-model event");
            }
        }

        // Proxy menu
        menu::ids::PROXY_TOGGLE => {
            // Toggle proxy based on current state
            let app_clone = app.clone();
            tauri::async_runtime::spawn(async move {
                let state: tauri::State<AppState> = app_clone.state();

                // Check current proxy status
                let proxy_running = match state.backend.get_proxy_status().await {
                    Ok(status) => status
                        .get("running")
                        .and_then(|v| v.as_bool())
                        .unwrap_or(false),
                    Err(_) => false,
                };

                if proxy_running {
                    // Stop proxy
                    if let Err(e) = state.backend.stop_proxy().await {
                        error!(error = %e, "Failed to stop proxy from menu");
                        // Emit error to frontend
                        let _ = app_clone
                            .emit("menu:proxy-error", format!("Failed to stop proxy: {}", e));
                    } else {
                        info!("Proxy stopped from menu");
                        let _ = app_clone.emit("menu:proxy-stopped", ());
                    }
                } else {
                    // Start proxy with default settings
                    // Frontend should handle this to use proper settings
                    let _ = app_clone.emit("menu:start-proxy", ());
                }

                // Sync menu state after proxy toggle
                let state_ref: tauri::State<AppState> = app_clone.state();
                if let Err(e) = sync_menu_state_internal(&app_clone, &state_ref).await {
                    warn!(error = %e, "Failed to sync menu after proxy toggle");
                }
            });
        }
        menu::ids::COPY_PROXY_URL => {
            // Copy proxy URL to clipboard
            let app_clone = app.clone();
            tauri::async_runtime::spawn(async move {
                let state: tauri::State<AppState> = app_clone.state();

                if let Ok(status) = state.backend.get_proxy_status().await {
                    let host = status
                        .get("host")
                        .and_then(|v| v.as_str())
                        .unwrap_or("127.0.0.1");
                    let port = status.get("port").and_then(|v| v.as_u64()).unwrap_or(8080);

                    let url = format!("http://{}:{}/v1", host, port);
                    let _ = app_clone.emit("menu:copy-to-clipboard", url);
                }
            });
        }

        // View menu
        menu::ids::SHOW_DOWNLOADS => {
            if let Err(e) = app.emit("menu:show-downloads", ()) {
                error!(error = %e, "Failed to emit show-downloads event");
            }
        }
        menu::ids::SHOW_CHAT => {
            if let Err(e) = app.emit("menu:show-chat", ()) {
                error!(error = %e, "Failed to emit show-chat event");
            }
        }
        menu::ids::TOGGLE_SIDEBAR => {
            if let Err(e) = app.emit("menu:toggle-sidebar", ()) {
                error!(error = %e, "Failed to emit toggle-sidebar event");
            }
        }

        // Help menu
        menu::ids::INSTALL_LLAMA => {
            if let Err(e) = app.emit("menu:install-llama", ()) {
                error!(error = %e, "Failed to emit install-llama event");
            }
        }
        menu::ids::CHECK_LLAMA_STATUS => {
            if let Err(e) = app.emit("menu:check-llama-status", ()) {
                error!(error = %e, "Failed to emit check-llama-status event");
            }
        }
        menu::ids::OPEN_DOCS => {
            // Open documentation URL
            let _ = open::that("https://github.com/mmogr/gglib");
        }

        // App menu
        menu::ids::PREFERENCES => {
            if let Err(e) = app.emit("menu:open-settings", ()) {
                error!(error = %e, "Failed to emit open-settings event");
            }
        }

        _ => {
            debug!(menu_id = %id, "Unhandled menu event");
        }
    }
}

// Start a minimal embedded API server for chat functionality
async fn start_embedded_api_server(
    backend: Arc<GuiBackend>,
    port: u16,
) -> Result<(), Box<dyn std::error::Error>> {
    use gglib::commands::gui_web::{routes, state::AppState as WebAppState};
    use std::net::SocketAddr;

    // Wrap GuiBackend in WebAppState for compatibility with handlers
    let state = Arc::new(WebAppState::new(backend));

    // Reuse the exact API router from the web UI so both platforms stay in sync
    let app = routes::api_routes(state);

    let addr = SocketAddr::from(([127, 0, 0, 1], port));
    println!(
        "🌐 Embedded API server listening on http://localhost:{}",
        port
    );

    let listener = tokio::net::TcpListener::bind(addr).await?;
    axum::serve(listener, app).await?;

    Ok(())
}
