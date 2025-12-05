//! Settings management commands.

use crate::app::AppState;
use gglib::models::gui::{AppSettings, UpdateSettingsRequest};

#[tauri::command]
pub async fn get_settings(state: tauri::State<'_, AppState>) -> Result<AppSettings, String> {
    state
        .backend
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
        .backend
        .update_settings(updates)
        .await
        .map_err(|e| format!("Failed to update settings: {}", e))
}
