// Prevents additional console window on Windows in release, DO NOT REMOVE!!
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use dotenvy::dotenv;
use gglib::{
    models::gui::{
        AddModelRequest, AppSettings, GuiModel, RemoveModelRequest, StartServerRequest,
        UpdateModelRequest, UpdateSettingsRequest,
    },
    services::gui_backend::{DownloadTaskError, GuiBackend},
};
use std::sync::Arc;
use tauri::Emitter;

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
    eprintln!("🚀 Serve model command called: id={}, ctx_size={:?}, context_length={:?}, port={:?}", id, ctx_size, context_length, port);
    
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
            eprintln!("✓ Server started successfully on port {}: {}", resp.port, resp.message);
            resp.message
        })
        .map_err(|e| {
            eprintln!("✗ Failed to start server: {}", e);
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
    eprintln!("Attempting to emit 'download-progress' (starting) for model: {}", model_id);
    if let Err(err) = app.emit(
        "download-progress",
        DownloadProgressEvent::starting(&model_id),
    ) {
        eprintln!("Failed to emit start event: {}", err);
    } else {
        eprintln!("Successfully emitted start event");
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
        // eprintln!("Emitting progress: {}/{}", downloaded, total); // Commented out to avoid spam
        if let Err(err) = app_clone.emit("download-progress", event) {
            eprintln!("Failed to emit progress event: {}", err);
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
                eprintln!("Failed to emit completion event: {}", err);
            }
            Ok(message)
        }
        Err(e) => {
            let is_cancelled = e.downcast_ref::<DownloadTaskError>().is_some();
            let error_msg = if is_cancelled {
                format!("Download '{}' was cancelled", model_id_clone2)
            } else {
                format!("Failed to download model: {}", e)
            };
            if let Err(err) = app.emit(
                "download-progress",
                DownloadProgressEvent::errored(&model_id_clone2, &error_msg),
            ) {
                eprintln!("Failed to emit error event: {}", err);
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

#[tauri::command]
fn get_gui_api_port(state: tauri::State<'_, AppState>) -> u16 {
    state.api_port
}

#[tokio::main]
async fn main() {
    let _ = dotenv();
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
            eprintln!("Failed to start embedded API server: {}", e);
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
            get_gui_api_port
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
