// Prevents additional console window on Windows in release, DO NOT REMOVE!!
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use dotenvy::dotenv;
use gglib::{
    models::gui::{
        AddModelRequest, AppSettings, GuiModel, RemoveModelRequest, StartServerRequest,
        UpdateModelRequest, UpdateSettingsRequest,
    },
    services::core::{DownloadError, DownloadQueueStatus},
    services::gui_backend::GuiBackend,
};
use std::sync::Arc;
use tauri::Emitter;
use tracing::{debug, error, info};

// Application state with shared backend
struct AppState {
    backend: Arc<GuiBackend>,
    api_port: u16,
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
async fn add_model(
    file_path: String,
    state: tauri::State<'_, AppState>,
) -> Result<String, String> {
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
        name: updates.get("name").and_then(|v| v.as_str().map(str::to_string)),
        quantization: updates.get("quantization").and_then(|v| v.as_str().map(str::to_string)),
        file_path: updates.get("file_path").and_then(|v| v.as_str().map(str::to_string)),
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
    state: tauri::State<'_, AppState>,
) -> Result<String, String> {
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
    };

    let result = state
        .backend
        .start_server(id, request)
        .await
        .map(|resp| {
            info!(port = %resp.port, "Server started successfully");
            resp.message
        })
        .map_err(|e| {
            error!(error = %e, "Failed to start server");
            format!("Failed to start server: {}", e)
        });
    
    result
}

#[tauri::command]
async fn stop_server(model_id: u32, state: tauri::State<'_, AppState>) -> Result<String, String> {
    state
        .backend
        .stop_model(model_id)
        .await
        .map_err(|e| format!("Failed to stop server: {}", e))
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

#[tauri::command]
async fn download_model(
    app: tauri::AppHandle,
    model_id: String,
    quantization: Option<String>,
    state: tauri::State<'_, AppState>,
) -> Result<String, String> {
    use gglib::commands::download::{DownloadProgressEvent, ProgressThrottle};

    // Emit download started event
    debug!(model_id = %model_id, "Attempting to emit download-progress (starting)");
    if let Err(err) = app.emit(
        "download-progress",
        DownloadProgressEvent::starting(&model_id),
    ) {
        error!(error = %err, "Failed to emit start event");
    } else {
        debug!("Successfully emitted start event");
    }
    
    // Clone for emission in closure
    let model_id_clone = model_id.clone();
    let app_clone = app.clone();
    let model_id_clone2 = model_id.clone();

    // Create progress callback
    let start_time = std::time::Instant::now();
    
    let throttle = ProgressThrottle::responsive_ui();
    let callback_throttle = throttle.clone();

    let progress_callback: gglib::commands::download::ProgressCallback = Box::new(move |downloaded, total| {
        if !callback_throttle.should_emit(downloaded, total) {
            return;
        }
        let event = DownloadProgressEvent::progress(&model_id_clone, downloaded, total, start_time);
        // debug!(downloaded, total, "Emitting progress"); // Commented out to avoid spam
        if let Err(err) = app_clone.emit("download-progress", event) {
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
                DownloadProgressEvent::completed(&model_id_clone2, Some(&message)),
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
                DownloadProgressEvent::errored(&model_id_clone2, &error_msg),
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
async fn get_proxy_status(
    state: tauri::State<'_, AppState>,
) -> Result<serde_json::Value, String> {
    state
        .backend
        .get_proxy_status()
        .await
        .map_err(|e| format!("Failed to get proxy status: {}", e))
}

// Tag Management Commands

#[tauri::command]
async fn list_tags(
    state: tauri::State<'_, AppState>,
) -> Result<Vec<String>, String> {
    state
        .backend
        .list_tags()
        .await
        .map_err(|e| format!("Failed to list tags: {}", e))
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

#[tauri::command]
async fn queue_download(
    app: tauri::AppHandle,
    model_id: String,
    quantization: Option<String>,
    state: tauri::State<'_, AppState>,
) -> Result<usize, String> {
    // Add to queue
    let position = state
        .backend
        .queue_download(model_id.clone(), quantization)
        .await
        .map_err(|e| format!("Failed to queue download: {}", e))?;

    // Start the queue processor in a background task (if not already running)
    let backend = state.backend.clone();
    let app_clone = app.clone();
    
    tokio::spawn(async move {
        use gglib::commands::download::DownloadProgressEvent;
        
        let progress_callback = move |event: DownloadProgressEvent| {
            if let Err(err) = app_clone.emit("download-progress", &event) {
                tracing::error!(error = %err, "Failed to emit download progress event");
            }
        };
        
        backend.core().downloads().process_queue(progress_callback).await;
    });

    Ok(position)
}

#[tauri::command]
async fn get_download_queue(
    state: tauri::State<'_, AppState>,
) -> Result<DownloadQueueStatus, String> {
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
async fn clear_failed_downloads(
    state: tauri::State<'_, AppState>,
) -> Result<String, String> {
    state.backend.clear_failed_downloads().await;
    Ok("Cleared failed downloads".to_string())
}

/// Check if llama.cpp is installed
#[tauri::command]
fn check_llama_status() -> Result<LlamaStatus, String> {
    use gglib::commands::llama::{check_llama_installed, check_prebuilt_availability, PrebuiltAvailability};
    
    let installed = check_llama_installed();
    let can_download = matches!(check_prebuilt_availability(), PrebuiltAvailability::Available { .. });
    
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
    use gglib::commands::llama::{check_prebuilt_availability, download_prebuilt_binaries_with_boxed_callback, PrebuiltAvailability};
    use gglib::commands::download::ProgressThrottle;
    use std::sync::Arc;
    
    // Check if pre-built binaries are available
    match check_prebuilt_availability() {
        PrebuiltAvailability::Available { description, .. } => {
            // Emit started event
            let _ = app.emit("llama-install-progress", LlamaInstallEvent {
                status: "started".to_string(),
                downloaded: 0,
                total: 0,
                percentage: 0.0,
                message: format!("Downloading llama.cpp for {}...", description),
            });
            
            // Create progress callback (boxed, thread-safe)
            let start_time = std::time::Instant::now();
            let throttle = Arc::new(ProgressThrottle::responsive_ui());
            let app_clone = app.clone();
            
            let progress_callback: Box<dyn Fn(u64, u64) + Send + Sync> = Box::new(move |downloaded: u64, total: u64| {
                if !throttle.should_emit(downloaded, total) {
                    return;
                }
                let elapsed = start_time.elapsed().as_secs_f64();
                let percentage = if total > 0 { (downloaded as f64 / total as f64) * 100.0 } else { 0.0 };
                let speed = if elapsed > 0.0 { downloaded as f64 / elapsed } else { 0.0 };
                let eta = if speed > 0.0 && total > downloaded { (total - downloaded) as f64 / speed } else { 0.0 };
                
                let _ = app_clone.emit("llama-install-progress", LlamaInstallEvent {
                    status: "downloading".to_string(),
                    downloaded,
                    total,
                    percentage,
                    message: format!("Downloading... {:.1}% ({:.1} MB/s, {:.0}s remaining)", 
                        percentage, speed / 1_000_000.0, eta),
                });
            });
            
            // Download with progress
            match download_prebuilt_binaries_with_boxed_callback(progress_callback).await {
                Ok(()) => {
                    let _ = app.emit("llama-install-progress", LlamaInstallEvent {
                        status: "completed".to_string(),
                        downloaded: 0,
                        total: 0,
                        percentage: 100.0,
                        message: "llama.cpp installed successfully!".to_string(),
                    });
                    Ok("llama.cpp installed successfully".to_string())
                }
                Err(e) => {
                    let error_msg = format!("Failed to install llama.cpp: {}", e);
                    let _ = app.emit("llama-install-progress", LlamaInstallEvent {
                        status: "error".to_string(),
                        downloaded: 0,
                        total: 0,
                        percentage: 0.0,
                        message: error_msg.clone(),
                    });
                    Err(error_msg)
                }
            }
        }
        PrebuiltAvailability::NotAvailable { reason } => {
            Err(format!("Cannot auto-install llama.cpp: {}. Please build from source.", reason))
        }
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

    let app_state = AppState {
        backend: backend.clone(),
        api_port: embedded_api_port,
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
        .invoke_handler(tauri::generate_handler![
            list_models,
            add_model,
            remove_model,
            update_model,
            serve_model,
            stop_server,
            list_servers,
            download_model,
            cancel_download,
            search_models,
            start_proxy,
            stop_proxy,
            get_proxy_status,
            list_tags,
            add_model_tag,
            remove_model_tag,
            get_model_tags,
            get_settings,
            update_settings,
            queue_download,
            get_download_queue,
            remove_from_download_queue,
            cancel_shard_group,
            clear_failed_downloads,
            get_gui_api_port,
            check_llama_status,
            install_llama
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
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
    println!("🌐 Embedded API server listening on http://localhost:{}", port);

    let listener = tokio::net::TcpListener::bind(addr).await?;
    axum::serve(listener, app).await?;

    Ok(())
}
