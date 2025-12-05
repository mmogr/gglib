//! Download and queue management commands.

use crate::app::events::{emit_or_log, names};
use crate::app::AppState;
use gglib::commands::download::ProgressThrottle;
use gglib::download::{DownloadEvent, QueueSnapshot};
use gglib::services::core::DownloadError;
use tauri::{AppHandle, Emitter};
use tracing::{debug, error};

/// Response for queue_download command.
#[derive(serde::Serialize)]
pub struct QueueDownloadResponse {
    pub position: usize,
    pub shard_count: usize,
}

// =============================================================================
// Direct Download Commands
// =============================================================================

#[tauri::command]
pub async fn download_model(
    app: AppHandle,
    model_id: String,
    quantization: Option<String>,
    state: tauri::State<'_, AppState>,
) -> Result<String, String> {
    // Emit download started event
    debug!(model_id = %model_id, "Attempting to emit download-progress (starting)");
    emit_or_log(&app, names::DOWNLOAD_PROGRESS, DownloadEvent::started(&model_id));

    // Clone for emission in closure
    let model_id_clone = model_id.clone();
    let app_clone = app.clone();
    let model_id_clone2 = model_id.clone();

    // Create progress callback with EWA speed calculation
    let throttle = ProgressThrottle::responsive_ui();
    let callback_throttle = throttle.clone();

    let progress_callback: gglib::commands::download::ProgressCallback =
        Box::new(move |downloaded, total| {
            let Some(speed) = callback_throttle.should_emit_with_speed(downloaded, total) else {
                return;
            };
            let event = DownloadEvent::progress(&model_id_clone, downloaded, total, speed);
            if let Err(err) = app_clone.emit(names::DOWNLOAD_PROGRESS, &event) {
                error!(error = %err, "Failed to emit progress event");
            }
        });

    // Use the shared backend
    let result = state
        .backend
        .download_model(model_id.clone(), quantization, Some(&progress_callback))
        .await;

    match result {
        Ok(message) => {
            emit_or_log(
                &app,
                names::DOWNLOAD_PROGRESS,
                DownloadEvent::completed(&model_id_clone2, Some(&message)),
            );
            Ok(message)
        }
        Err(e) => {
            let is_cancelled = e.downcast_ref::<DownloadError>().is_some();
            let error_msg = if is_cancelled {
                format!("Download '{}' was cancelled", model_id_clone2)
            } else {
                format!("Failed to download model: {}", e)
            };
            emit_or_log(
                &app,
                names::DOWNLOAD_PROGRESS,
                DownloadEvent::failed(&model_id_clone2, &error_msg),
            );
            Err(error_msg)
        }
    }
}

#[tauri::command]
pub async fn cancel_download(
    model_id: String,
    state: tauri::State<'_, AppState>,
) -> Result<String, String> {
    state
        .backend
        .cancel_download(&model_id)
        .await
        .map(|_| format!("Download '{}' cancelled", model_id))
        .map_err(|e| format!("Failed to cancel download: {}", e))
}

// =============================================================================
// Queue Management Commands
// =============================================================================

#[tauri::command]
pub async fn queue_download(
    _app: AppHandle,
    model_id: String,
    quantization: Option<String>,
    state: tauri::State<'_, AppState>,
) -> Result<QueueDownloadResponse, String> {
    // Add to queue (auto-detects and handles sharded models)
    let (position, shard_count) = state
        .backend
        .queue_download(model_id.clone(), quantization)
        .await
        .map_err(|e| format!("Failed to queue download: {}", e))?;

    // Start the queue processor in a background task (if not already running)
    if start_queue_if_idle(&state) {
        let backend = state.backend.clone();
        tokio::spawn(async move {
            // process_queue runs until queue is empty, handles progress internally via on_event
            let _ = backend.core().downloads().process_queue().await;
            // Mark idle when done so future queues can start
            backend.core().mark_queue_idle();
        });
    }

    Ok(QueueDownloadResponse {
        position,
        shard_count,
    })
}

#[tauri::command]
pub async fn get_download_queue(state: tauri::State<'_, AppState>) -> Result<QueueSnapshot, String> {
    Ok(state.backend.get_download_queue().await)
}

#[tauri::command]
pub async fn remove_from_download_queue(
    model_id: String,
    state: tauri::State<'_, AppState>,
) -> Result<String, String> {
    state
        .backend
        .remove_from_download_queue(&model_id)
        .await
        .map(|_| format!("Removed '{}' from download queue", model_id))
        .map_err(|e| format!("Failed to remove from queue: {}", e))
}

#[tauri::command]
pub async fn reorder_download_queue(
    model_id: String,
    new_position: usize,
    state: tauri::State<'_, AppState>,
) -> Result<usize, String> {
    state
        .backend
        .reorder_download_queue(&model_id, new_position)
        .await
        .map_err(|e| format!("Failed to reorder queue: {}", e))
}

#[tauri::command]
pub async fn cancel_shard_group(
    group_id: String,
    state: tauri::State<'_, AppState>,
) -> Result<String, String> {
    state
        .backend
        .cancel_shard_group(&group_id)
        .await
        .map(|_| format!("Cancelled shard group '{}'", group_id))
        .map_err(|e| format!("Failed to cancel shard group: {}", e))
}

#[tauri::command]
pub async fn clear_failed_downloads(state: tauri::State<'_, AppState>) -> Result<String, String> {
    state.backend.clear_failed_downloads().await;
    Ok("Cleared failed downloads".to_string())
}

// =============================================================================
// Helpers
// =============================================================================

/// Check if queue processor should be started.
fn start_queue_if_idle(state: &tauri::State<'_, AppState>) -> bool {
    state.backend.core().start_queue_if_idle()
}
