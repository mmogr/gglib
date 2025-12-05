//! HuggingFace browsing and search commands.

use crate::app::AppState;
use gglib::models::gui::{
    HfQuantizationsResponse, HfSearchRequest, HfSearchResponse, HfToolSupportResponse,
};

#[tauri::command]
pub async fn browse_hf_models(
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
pub async fn get_hf_quantizations(
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
pub async fn get_hf_tool_support(
    model_id: String,
    state: tauri::State<'_, AppState>,
) -> Result<HfToolSupportResponse, String> {
    state
        .backend
        .get_hf_tool_support(&model_id)
        .await
        .map_err(|e| format!("Failed to get tool support info: {}", e))
}

#[tauri::command]
pub async fn search_models(
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
