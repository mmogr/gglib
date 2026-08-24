//! llama.cpp installation and status commands.

use crate::app::events::{emit_or_log, names};
use gglib_runtime::llama::{
    LlamaProgressEvent, PrebuiltAvailability, check_llama_installed, check_prebuilt_availability,
    download_prebuilt_binaries,
};
use tauri::AppHandle;
use tokio::sync::mpsc;

/// Response for check_llama_status command.
#[derive(serde::Serialize)]
pub(crate) struct LlamaStatus {
    pub installed: bool,
    pub can_download: bool,
}

/// Check if llama.cpp is installed.
#[tauri::command]
pub(crate) fn check_llama_status() -> Result<LlamaStatus, String> {
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
///
/// The WebView receives [`LlamaProgressEvent`] verbatim on
/// `llama-install-progress` — byte for byte the payload the SSE route streams,
/// so both transports render from one type.
///
/// This command used to run its own `RateEstimator` and `ProgressThrottle`
/// behind an `Arc<Mutex<_>>`, derive a percentage, and invent a four-value
/// status vocabulary, all because the callback it was handed carried nothing
/// but two byte counts. The pipeline now says which phase it is in and how
/// fast it is going, and this is a forwarder.
#[tauri::command]
pub(crate) async fn install_llama(app: AppHandle) -> Result<String, String> {
    if let PrebuiltAvailability::NotAvailable { reason } = check_prebuilt_availability() {
        return Err(format!(
            "Cannot auto-install llama.cpp: {reason}. Please build from source."
        ));
    }

    let (tx, mut rx) = mpsc::channel::<LlamaProgressEvent>(64);
    let install = tokio::spawn(download_prebuilt_binaries(tx));

    while let Some(event) = rx.recv().await {
        emit_or_log(&app, names::LLAMA_INSTALL_PROGRESS, event);
    }

    let outcome = match install.await {
        Ok(result) => result.map_err(|e| format!("Failed to install llama.cpp: {e}")),
        Err(e) => Err(format!("Install task failed: {e}")),
    };

    match outcome {
        Ok(()) => Ok("llama.cpp installed successfully".to_string()),
        Err(message) => {
            // The pipeline reports failure by returning rather than emitting,
            // leaving the wording to whoever owns the channel. The WebView is
            // listening there, so it hears it there.
            emit_or_log(
                &app,
                names::LLAMA_INSTALL_PROGRESS,
                LlamaProgressEvent::Failed {
                    message: message.clone(),
                },
            );
            Err(message)
        }
    }
}
