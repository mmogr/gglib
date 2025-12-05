//! Proxy management commands.

use crate::app::AppState;

#[tauri::command]
pub async fn start_proxy(
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
pub async fn stop_proxy(state: tauri::State<'_, AppState>) -> Result<String, String> {
    state
        .backend
        .stop_proxy()
        .await
        .map_err(|e| format!("Failed to stop proxy: {}", e))
}

#[tauri::command]
pub async fn get_proxy_status(
    state: tauri::State<'_, AppState>,
) -> Result<serde_json::Value, String> {
    state
        .backend
        .get_proxy_status()
        .await
        .map_err(|e| format!("Failed to get proxy status: {}", e))
}
