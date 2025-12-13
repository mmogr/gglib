//! HuggingFace browsing and search commands.

use crate::app::AppState;
use gglib_tauri::gui_backend::{
    HfQuantizationsResponse, HfSearchRequest, HfSearchResponse, HfToolSupportResponse,
};

#[tauri::command]
pub async fn search_hf_models(
    request: HfSearchRequest,
    state: tauri::State<'_, AppState>,
) -> Result<HfSearchResponse, String> {
    state
        .gui
        .browse_hf_models(request)
        .await
        .map_err(|e| format!("Failed to search HuggingFace models: {}", e))
}

#[tauri::command]
pub async fn get_hf_quantizations(
    model_id: String,
    state: tauri::State<'_, AppState>,
) -> Result<HfQuantizationsResponse, String> {
    state
        .gui
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
        .gui
        .get_hf_tool_support(&model_id)
        .await
        .map_err(|e| format!("Failed to get tool support info: {}", e))
}

#[tauri::command]
pub async fn search_models(
    _query: String,
    _limit: u32,
    _sort: Option<String>,
    _gguf_only: bool,
) -> Result<String, String> {
    // TEMPORARILY DISABLED during Phase 2 crate extraction.
    // Use gglib-cli for search functionality.
    // See issue #221 for migration status.
    Err("Search is temporarily disabled during Phase 2 migration. Use gglib-cli instead.".to_string())
}
