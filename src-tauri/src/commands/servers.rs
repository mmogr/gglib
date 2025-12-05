//! Server management commands.

use crate::app::events::{emit_or_log, names};
use crate::app::AppState;
use gglib::models::gui::{StartServerRequest, StartServerResponse};
use gglib::utils::process::log_streamer::{get_log_manager, ServerLogEntry};
use gglib::utils::process::ServerEvent;
use gglib::utils::ServerInfo;
use tauri::AppHandle;
use tracing::{debug, error, info};

#[tauri::command]
pub async fn serve_model(
    id: u32,
    ctx_size: Option<String>,
    context_length: Option<u64>,
    mlock: bool,
    port: Option<u16>,
    jinja: Option<bool>,
    app: AppHandle,
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
            emit_or_log(&app, names::SERVER_RUNNING, &event);

            resp
        })
        .map_err(|e| {
            error!(error = %e, "Failed to start server");
            format!("Failed to start server: {}", e)
        })
}

#[tauri::command]
pub async fn stop_server(
    model_id: u32,
    app: AppHandle,
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
    emit_or_log(&app, names::SERVER_STOPPING, &stopping_event);

    // Actually stop the server
    let result = state
        .backend
        .stop_model(model_id)
        .await
        .map_err(|e| format!("Failed to stop server: {}", e));

    // Emit server:stopped event after successful stop
    if result.is_ok() {
        let stopped_event = ServerEvent::stopped(model_id, port);
        emit_or_log(&app, names::SERVER_STOPPED, &stopped_event);
    }

    result
}

#[tauri::command]
pub async fn list_servers(state: tauri::State<'_, AppState>) -> Result<Vec<ServerInfo>, String> {
    state
        .backend
        .list_servers()
        .await
        .map_err(|e| format!("Failed to list servers: {}", e))
}

#[tauri::command]
pub async fn get_server_logs(port: u16) -> Result<Vec<ServerLogEntry>, String> {
    let log_manager = get_log_manager();
    Ok(log_manager.get_logs(port))
}

#[tauri::command]
pub async fn clear_server_logs(port: u16) -> Result<(), String> {
    let log_manager = get_log_manager();
    log_manager.clear_logs(port);
    Ok(())
}
