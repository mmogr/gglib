//! Settings management commands.

use crate::app::AppState;
use gglib_tauri::gui_backend::{AppSettings, ModelsDirectoryInfo, SystemMemoryInfo, UpdateSettingsRequest};

#[tauri::command]
pub async fn get_settings(state: tauri::State<'_, AppState>) -> Result<AppSettings, String> {
    state
        .gui
        .get_settings()
        .await
        .map_err(|e| format!("Failed to get settings: {}", e))
}

#[tauri::command]
pub async fn update_settings(
    updates: UpdateSettingsRequest,
    state: tauri::State<'_, AppState>,
) -> Result<AppSettings, String> {
    state
        .gui
        .update_settings(updates)
        .await
        .map_err(|e| format!("Failed to update settings: {}", e))
}

#[tauri::command]
pub fn get_models_directory(state: tauri::State<'_, AppState>) -> Result<ModelsDirectoryInfo, String> {
    state
        .gui
        .get_models_directory_info()
        .map_err(|e| format!("Failed to get models directory: {}", e))
}

#[tauri::command]
pub fn set_models_directory(
    path: String,
    state: tauri::State<'_, AppState>,
) -> Result<ModelsDirectoryInfo, String> {
    state
        .gui
        .update_models_directory(path)
        .map_err(|e| format!("Failed to set models directory: {}", e))
}

#[tauri::command]
pub fn get_system_memory(state: tauri::State<'_, AppState>) -> Result<SystemMemoryInfo, String> {
    state
        .gui
        .get_system_memory()
        .map_err(|e| format!("Failed to get system memory: {}", e))
}
