//! Download and queue management commands.
//!
//! All downloads now go through the queue with events handled by AppEventBridge.
//! The legacy "blocking download with progress callback" pattern has been replaced
//! with queue-based downloads where the UI receives events via the event bridge.

use crate::app::AppState;
use gglib_tauri::gui_backend::QueueSnapshot;

/// Response for queue_download command.
#[derive(serde::Serialize)]
pub struct QueueDownloadResponse {
    pub position: usize,
    pub shard_count: usize,
}

// =============================================================================
// Download Commands
// =============================================================================

/// Queue a model download.
///
/// This is the primary download command. Downloads are processed asynchronously
/// and progress events are emitted via the AppEventBridge to the frontend.
#[tauri::command]
pub async fn download_model(
    model_id: String,
    quantization: Option<String>,
    state: tauri::State<'_, AppState>,
) -> Result<QueueDownloadResponse, String> {
    // All downloads go through the queue now
    // The GuiBackend.queue_download() handles worker spawning internally
    let (position, shard_count) = state
        .gui
        .queue_download(model_id, quantization)
        .await
        .map_err(|e| format!("Failed to queue download: {}", e))?;

    Ok(QueueDownloadResponse {
        position,
        shard_count,
    })
}

/// Queue a model download (explicit queue command, same as download_model).
#[tauri::command]
pub async fn queue_download(
    model_id: String,
    quantization: Option<String>,
    state: tauri::State<'_, AppState>,
) -> Result<QueueDownloadResponse, String> {
    let (position, shard_count) = state
        .gui
        .queue_download(model_id, quantization)
        .await
        .map_err(|e| format!("Failed to queue download: {}", e))?;

    Ok(QueueDownloadResponse {
        position,
        shard_count,
    })
}

/// Cancel an active or queued download.
#[tauri::command]
pub async fn cancel_download(
    model_id: String,
    state: tauri::State<'_, AppState>,
) -> Result<String, String> {
    state
        .gui
        .cancel_download(&model_id)
        .await
        .map(|_| format!("Download '{}' cancelled", model_id))
        .map_err(|e| format!("Failed to cancel download: {}", e))
}

// =============================================================================
// Queue Management Commands
// =============================================================================

/// Get the current download queue state.
#[tauri::command]
pub async fn get_download_queue(state: tauri::State<'_, AppState>) -> Result<QueueSnapshot, String> {
    Ok(state.gui.get_download_queue().await)
}

/// Remove an item from the download queue.
#[tauri::command]
pub async fn remove_from_download_queue(
    model_id: String,
    state: tauri::State<'_, AppState>,
) -> Result<String, String> {
    state
        .gui
        .remove_from_download_queue(&model_id)
        .await
        .map(|_| format!("Removed '{}' from download queue", model_id))
        .map_err(|e| format!("Failed to remove from queue: {}", e))
}

/// Reorder the download queue.
#[tauri::command]
pub async fn reorder_download_queue(
    ids: Vec<String>,
    state: tauri::State<'_, AppState>,
) -> Result<(), String> {
    state
        .gui
        .reorder_download_queue_full(&ids)
        .await
        .map_err(|e| format!("Failed to reorder queue: {}", e))
}

/// Cancel all shards in a shard group.
#[tauri::command]
pub async fn cancel_shard_group(
    group_id: String,
    state: tauri::State<'_, AppState>,
) -> Result<String, String> {
    state
        .gui
        .cancel_shard_group(&group_id)
        .await
        .map(|_| format!("Cancelled shard group '{}'", group_id))
        .map_err(|e| format!("Failed to cancel shard group: {}", e))
}

/// Clear all failed downloads from the list.
#[tauri::command]
pub async fn clear_failed_downloads(state: tauri::State<'_, AppState>) -> Result<String, String> {
    state.gui.clear_failed_downloads().await;
    Ok("Cleared failed downloads".to_string())
}
