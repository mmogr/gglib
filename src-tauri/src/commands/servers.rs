//! Server management commands.

use crate::app::AppState;
use gglib_gui::types::{StartServerRequest, StartServerResponse};
use gglib_runtime::process::{get_log_manager, ServerLogEntry};
use gglib_tauri::gui_backend::ServerInfo;
use tracing::debug;

#[tauri::command]
pub async fn serve_model(
    id: i64,
    request: StartServerRequest,
    state: tauri::State<'_, AppState>,
) -> Result<StartServerResponse, String> {
    debug!(
        model_id = %id,
        "Serve model command called"
    );

    state
        .gui
        .start_server(id, request)
        .await
        .map_err(|e| format!("Failed to start server: {}", e))
}

#[tauri::command]
pub async fn stop_server(
    model_id: i64,
    state: tauri::State<'_, AppState>,
) -> Result<String, String> {
    state
        .gui
        .stop_server(model_id)
        .await
        .map_err(|e| format!("Failed to stop server: {}", e))
}

#[tauri::command]
pub async fn list_servers(state: tauri::State<'_, AppState>) -> Result<Vec<ServerInfo>, String> {
    Ok(state.gui.list_servers().await)
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
