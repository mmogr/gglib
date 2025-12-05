//! Tag management commands.

use crate::app::AppState;
use gglib::services::database::ModelFilterOptions;

#[tauri::command]
pub async fn list_tags(state: tauri::State<'_, AppState>) -> Result<Vec<String>, String> {
    state
        .backend
        .list_tags()
        .await
        .map_err(|e| format!("Failed to list tags: {}", e))
}

#[tauri::command]
pub async fn get_model_filter_options(
    state: tauri::State<'_, AppState>,
) -> Result<ModelFilterOptions, String> {
    state
        .backend
        .get_model_filter_options()
        .await
        .map_err(|e| format!("Failed to get filter options: {}", e))
}

#[tauri::command]
pub async fn add_model_tag(
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
pub async fn remove_model_tag(
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
pub async fn get_model_tags(
    model_id: u32,
    state: tauri::State<'_, AppState>,
) -> Result<Vec<String>, String> {
    state
        .backend
        .get_model_tags(model_id)
        .await
        .map_err(|e| format!("Failed to get model tags: {}", e))
}
