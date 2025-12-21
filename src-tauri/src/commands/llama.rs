//! llama.cpp installation and status commands.

use crate::app::events::{emit_or_log, names};
use gglib_download::ProgressThrottle;
use gglib_runtime::llama::{
    check_llama_installed, check_prebuilt_availability, download_prebuilt_binaries_with_boxed_callback,
    PrebuiltAvailability,
};
use std::sync::{Arc, Mutex};
use tauri::AppHandle;

/// Response for check_llama_status command.
#[derive(serde::Serialize)]
pub struct LlamaStatus {
    pub installed: bool,
    pub can_download: bool,
}

/// Progress event for llama installation.
#[derive(Clone, serde::Serialize)]
pub struct LlamaInstallEvent {
    pub status: String,
    pub downloaded: u64,
    pub total: u64,
    pub percentage: f64,
    pub message: String,
}

/// Check if llama.cpp is installed.
#[tauri::command]
pub fn check_llama_status() -> Result<LlamaStatus, String> {
    let installed = check_llama_installed();
    let can_download = matches!(
        check_prebuilt_availability(),
        PrebuiltAvailability::Available { .. }
    );

    Ok(LlamaStatus {
        installed,
        can_download,
    })
}

/// Install llama.cpp by downloading pre-built binaries.
#[tauri::command]
pub async fn install_llama(app: AppHandle) -> Result<String, String> {
    // Check if pre-built binaries are available
    match check_prebuilt_availability() {
        PrebuiltAvailability::Available { description, .. } => {
            // Emit started event
            emit_or_log(
                &app,
                names::LLAMA_INSTALL_PROGRESS,
                LlamaInstallEvent {
                    status: "started".to_string(),
                    downloaded: 0,
                    total: 0,
                    percentage: 0.0,
                    message: format!("Downloading llama.cpp for {}...", description),
                },
            );

            // Create progress callback (boxed, thread-safe)
            let start_time = std::time::Instant::now();
            let throttle = Arc::new(Mutex::new(ProgressThrottle::default()));
            let app_clone = app.clone();

            let progress_callback: Box<dyn Fn(u64, u64) + Send + Sync> =
                Box::new(move |downloaded: u64, total: u64| {
                    // Rate-limit progress updates
                    if let Ok(mut t) = throttle.lock()
                        && !t.should_emit() {
                            return;
                        }
                    let elapsed = start_time.elapsed().as_secs_f64();
                    let percentage = if total > 0 {
                        (downloaded as f64 / total as f64) * 100.0
                    } else {
                        0.0
                    };
                    let speed = if elapsed > 0.0 {
                        downloaded as f64 / elapsed
                    } else {
                        0.0
                    };
                    let eta = if speed > 0.0 && total > downloaded {
                        (total - downloaded) as f64 / speed
                    } else {
                        0.0
                    };

                    emit_or_log(
                        &app_clone,
                        names::LLAMA_INSTALL_PROGRESS,
                        LlamaInstallEvent {
                            status: "downloading".to_string(),
                            downloaded,
                            total,
                            percentage,
                            message: format!(
                                "Downloading... {:.1}% ({:.1} MB/s, {:.0}s remaining)",
                                percentage,
                                speed / 1_000_000.0,
                                eta
                            ),
                        },
                    );
                });

            // Download with progress
            match download_prebuilt_binaries_with_boxed_callback(progress_callback).await {
                Ok(()) => {
                    emit_or_log(
                        &app,
                        names::LLAMA_INSTALL_PROGRESS,
                        LlamaInstallEvent {
                            status: "completed".to_string(),
                            downloaded: 0,
                            total: 0,
                            percentage: 100.0,
                            message: "llama.cpp installed successfully!".to_string(),
                        },
                    );
                    Ok("llama.cpp installed successfully".to_string())
                }
                Err(e) => {
                    let error_msg = format!("Failed to install llama.cpp: {}", e);
                    emit_or_log(
                        &app,
                        names::LLAMA_INSTALL_PROGRESS,
                        LlamaInstallEvent {
                            status: "error".to_string(),
                            downloaded: 0,
                            total: 0,
                            percentage: 0.0,
                            message: error_msg.clone(),
                        },
                    );
                    Err(error_msg)
                }
            }
        }
        PrebuiltAvailability::NotAvailable { reason } => Err(format!(
            "Cannot auto-install llama.cpp: {}. Please build from source.",
            reason
        )),
    }
}
