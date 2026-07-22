//! llama.cpp installation and status commands.

use crate::app::events::{emit_or_log, names};
use gglib_core::download::{RateEstimator, format_duration, format_rate};
use gglib_core::paths::{llama_cpp_dir, llama_server_path};
use gglib_download::ProgressThrottle;
use gglib_runtime::llama::{
    BuildEvent, PrebuiltAvailability, check_llama_installed, check_prebuilt_availability,
    detect_optimal_acceleration, download_prebuilt_binaries_with_boxed_callback,
    run_llama_source_build,
};
use std::sync::{Arc, Mutex};
use std::time::Instant;
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

/// Trigger a llama.cpp source build and stream progress as Tauri events.
///
/// Emits `llama-build-progress` events to the frontend throughout the build.
/// Each event payload is a serialised [`BuildEvent`]:
///
/// | Variant          | JSON shape                                              |
/// |------------------|---------------------------------------------------------|
/// | `PhaseStarted`   | `{ "type": "phase_started", "phase": "..." }`          |
/// | `Log`            | `{ "type": "log", "message": "..." }`                  |
/// | `Progress`       | `{ "type": "progress", "current": N, "total": N }`     |
/// | `PhaseCompleted` | `{ "type": "phase_completed", "phase": "..." }`        |
/// | `Completed`      | `{ "type": "completed", "version": "...", "acceleration": "..." }` |
/// | `Failed`         | `{ "type": "failed", "message": "..." }`               |
///
/// Returns an error string if path resolution, acceleration detection, or the
/// build itself fails.
#[tauri::command]
pub async fn build_llama_from_source(app: AppHandle) -> Result<(), String> {
    let (tx, mut rx) = tokio::sync::mpsc::channel::<BuildEvent>(64);

    let llama_dir = llama_cpp_dir().map_err(|e| e.to_string())?;
    let server_path = llama_server_path().map_err(|e| e.to_string())?;
    let acceleration = detect_optimal_acceleration().map_err(|e| e.to_string())?;

    let build_handle = tokio::spawn(async move {
        run_llama_source_build(acceleration, llama_dir, server_path, tx).await
    });

    while let Some(event) = rx.recv().await {
        emit_or_log(&app, names::LLAMA_BUILD_PROGRESS, event);
    }

    build_handle
        .await
        .map_err(|e| format!("Build task panicked: {e}"))?
        .map_err(|e| e.to_string())
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

            // Create progress callback (boxed, thread-safe).
            //
            // Speed and ETA come from the same `RateEstimator` the model
            // download path uses, and render through the same formatters, so
            // this bar reads identically to the ones in the CLI and the model
            // download UI. The previous inline arithmetic here divided total
            // bytes by total elapsed time — a cumulative average that keeps
            // drifting toward the run's mean and never reflects current
            // throughput — and used 0 to mean "no ETA yet".
            let estimator = Arc::new(Mutex::new(RateEstimator::new(Instant::now())));
            let throttle = Arc::new(Mutex::new(ProgressThrottle::default()));
            let app_clone = app.clone();

            let progress_callback: Box<dyn Fn(u64, u64) + Send + Sync> =
                Box::new(move |downloaded: u64, total: u64| {
                    // Feed every callback; the estimator wants all the samples.
                    let (speed_bps, eta_seconds) = match estimator.lock() {
                        Ok(mut est) => {
                            est.record(downloaded, total, Instant::now());
                            (est.rate_bps(), est.eta_seconds())
                        }
                        Err(_) => (None, None),
                    };

                    // Rate-limit only the *emission*, not the measurement.
                    if let Ok(mut t) = throttle.lock()
                        && !t.should_emit()
                    {
                        return;
                    }

                    let percentage = if total > 0 {
                        #[allow(clippy::cast_precision_loss)]
                        let pct = (downloaded as f64 / total as f64) * 100.0;
                        pct.clamp(0.0, 100.0)
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
                                "Downloading... {percentage:.1}% ({}, {} remaining)",
                                format_rate(speed_bps),
                                format_duration(eta_seconds),
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
