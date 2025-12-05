//! Model management commands.

use crate::app::AppState;
use gglib::models::gui::{AddModelRequest, GuiModel, RemoveModelRequest, UpdateModelRequest};

#[tauri::command]
pub async fn list_models(state: tauri::State<'_, AppState>) -> Result<Vec<GuiModel>, String> {
    state
        .backend
        .list_models()
        .await
        .map_err(|e| format!("Failed to list models: {}", e))
}

#[tauri::command]
pub async fn add_model(
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
pub async fn remove_model(
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
pub async fn update_model(
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
