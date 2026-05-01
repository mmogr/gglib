//! Python-based fast download orchestrator.
//!
//! This module coordinates the fast download process using:
//! - `python_env`: Environment setup (venv, requirements, script)
//! - `python_protocol`: JSON message parsing
//! - `progress`: Terminal progress display
//!
//! The orchestrator spawns a Python subprocess, streams its output,
//! and dispatches progress events to callbacks or CLI display.

use std::path::Path;
use std::sync::Arc;

use gglib_core::utils::process::async_cmd;
use thiserror::Error;
use tokio::io::{AsyncBufReadExt, AsyncReadExt, BufReader};
use tokio::signal;
use tokio_util::sync::CancellationToken;

use super::progress::CliProgressPrinter;
use super::python_env::{EnvSetupError, PythonEnvironment};
use super::python_protocol::{ProtocolError, PythonEvent, parse_line};
use super::xet_poller::XetPoller;

// ============================================================================
// Types
// ============================================================================

/// Callback for download progress: (`downloaded_bytes`, `total_bytes`).
///
/// Wrapped in an `Arc` so the bridge can share the same callback with the
/// [`XetPoller`] background task (which needs `'static` ownership) without
/// boxing through extra channels.
pub type ProgressCallback = Arc<dyn Fn(u64, u64) + Send + Sync>;

// ============================================================================
// Constants
// ============================================================================

const CANCELLED_MSG: &str = "fast download cancelled by user";

// ============================================================================
// Error Types
// ============================================================================

/// Errors that can occur during fast download.
#[derive(Error, Debug)]
pub enum PythonBridgeError {
    #[error("Environment setup failed: {0}")]
    Env(#[from] EnvSetupError),

    #[error("Protocol error: {0}")]
    Protocol(#[from] ProtocolError),

    #[error("Download process failed: {0}")]
    ProcessFailed(String),

    #[error("Download unavailable: {0}")]
    Unavailable(String),

    #[error("{}", CANCELLED_MSG)]
    Cancelled,
}

// ============================================================================
// Request Types
// ============================================================================

/// Request payload for running the fast downloader.
pub struct FastDownloadRequest<'a> {
    pub repo_id: &'a str,
    pub revision: &'a str,
    pub repo_type: &'a str,
    pub destination: &'a Path,
    pub files: &'a [String],
    pub token: Option<&'a str>,
    pub force: bool,
    /// Progress sink for `(downloaded, total)` byte updates. Shared with the
    /// xet stat-fallback poller so synthetic progress events surface through
    /// the same channel as real Python `tqdm` events.
    pub progress: Option<ProgressCallback>,
    /// Optional total size hint forwarded to synthetic progress events when
    /// the Python helper goes silent. `None` means "unknown" — the bar will
    /// display downloaded bytes without a percentage.
    pub expected_total: Option<u64>,
    /// Cancellation token for external cancellation.
    pub cancel_token: Option<CancellationToken>,
}

// ============================================================================
// Public API
// ============================================================================

/// Ensure the fast download helper is ready (env + script prepared).
pub async fn ensure_fast_helper_ready() -> Result<(), PythonBridgeError> {
    PythonEnvironment::prepare().await?;
    Ok(())
}

/// Preflight the fast download helper.
///
/// Validates that a usable Python interpreter exists and can import the
/// standard library (including `encodings`). This does not create the venv.
///
/// Returns the resolved `sys.executable` string on success.
pub async fn preflight_fast_helper() -> Result<String, PythonBridgeError> {
    Ok(PythonEnvironment::preflight().await?)
}

/// Run the fast download using the embedded Python helper.
pub async fn run_fast_download(request: &FastDownloadRequest<'_>) -> Result<(), PythonBridgeError> {
    if request.files.is_empty() {
        return Ok(());
    }

    let env = PythonEnvironment::prepare().await?;

    run_download_process(&env, request).await
}

// ============================================================================
// Process Orchestration
// ============================================================================

#[allow(clippy::too_many_lines)]
async fn run_download_process(
    env: &PythonEnvironment,
    request: &FastDownloadRequest<'_>,
) -> Result<(), PythonBridgeError> {
    let mut cmd = async_cmd(env.python_path());
    cmd.arg(env.script_path())
        .arg("--repo-id")
        .arg(request.repo_id)
        .arg("--revision")
        .arg(request.revision)
        .arg("--repo-type")
        .arg(request.repo_type)
        .arg("--dest")
        .arg(request.destination)
        .kill_on_drop(true)
        .env("PYTHONUNBUFFERED", "1")
        .env("PYTHONNOUSERSITE", "1")
        .env("HF_HUB_DISABLE_TELEMETRY", "1");

    // Denylist-based environment isolation.
    // Prevent conda/venv pollution (PYTHONHOME/PYTHONPATH) from breaking stdlib imports.
    for key in [
        "PYTHONHOME",
        "PYTHONPATH",
        "PYTHONUSERBASE",
        "VIRTUAL_ENV",
        "CONDA_PREFIX",
        "CONDA_DEFAULT_ENV",
        "CONDA_PROMPT_MODIFIER",
        "CONDA_SHLVL",
        "CONDA_EXE",
        "CONDA_PYTHON_EXE",
        "_CE_CONDA",
        "_CE_M",
    ] {
        cmd.env_remove(key);
    }

    if let Some(token) = request.token {
        cmd.arg("--token").arg(token);
    }
    if request.force {
        cmd.arg("--force");
    }
    for file in request.files {
        cmd.arg("--file").arg(file);
    }

    let mut child = cmd
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .map_err(|e| PythonBridgeError::ProcessFailed(format!("Failed to spawn: {e}")))?;

    let stdout = child
        .stdout
        .take()
        .ok_or_else(|| PythonBridgeError::ProcessFailed("Missing stdout".to_string()))?;
    let stderr = child
        .stderr
        .take()
        .ok_or_else(|| PythonBridgeError::ProcessFailed("Missing stderr".to_string()))?;

    let mut lines = BufReader::new(stdout).lines();
    let mut stderr_reader = BufReader::new(stderr);
    let stderr_task = tokio::spawn(async move {
        let mut buf = Vec::new();
        let _ = stderr_reader.read_to_end(&mut buf).await;
        buf
    });

    // CLI progress if no callback provided
    let mut cli_progress = if request.progress.is_none() {
        Some(CliProgressPrinter::new())
    } else {
        None
    };

    // Spawn the xet stat-fallback poller. It only emits synthetic progress
    // events when the Python helper goes silent (typical of the hf-xet fast
    // path); while real `tqdm` events are flowing it stays dormant.
    //
    // We hand the poller the destination *directory* (not the per-file
    // final paths) because `huggingface_hub` streams bytes into a
    // `<dest>/.cache/huggingface/download/<filename>.incomplete` temp file
    // while the transfer is in flight and only renames to the final path on
    // completion. The poller walks the directory recursively so the
    // in-progress bytes are accounted for.
    let xet_poller = request.progress.as_ref().map(|cb| {
        let targets = vec![request.destination.to_path_buf()];
        XetPoller::spawn(targets, request.expected_total, Arc::clone(cb))
    });

    let mut ctrl_c = Box::pin(signal::ctrl_c());
    let cancel_token = request.cancel_token.clone();

    // Event loop
    loop {
        tokio::select! {
            // External cancellation
            () = async {
                if let Some(ref token) = cancel_token {
                    token.cancelled().await;
                } else {
                    std::future::pending::<()>().await;
                }
            } => {
                let _ = child.kill().await;
                finish_progress(&mut cli_progress);
                if let Some(p) = xet_poller { p.shutdown(); }
                return Err(PythonBridgeError::Cancelled);
            }

            // Ctrl+C from terminal
            _ = &mut ctrl_c => {
                let _ = child.kill().await;
                finish_progress(&mut cli_progress);
                if let Some(p) = xet_poller { p.shutdown(); }
                return Err(PythonBridgeError::Cancelled);
            }

            // Process stdout lines
            line = lines.next_line() => {
                let line = line.map_err(|e| PythonBridgeError::ProcessFailed(e.to_string()))?;
                let Some(line) = line else { break; };

                if line.trim().is_empty() {
                    continue;
                }

                if let Ok(event) = parse_line(&line) {
                    match handle_event(event, request, &mut cli_progress, xet_poller.as_ref()) {
                        Ok(()) => {}
                        Err(e) => {
                            let _ = child.kill().await;
                            finish_progress(&mut cli_progress);
                            if let Some(p) = xet_poller { p.shutdown(); }
                            return Err(e);
                        }
                    }
                } else {
                    // Non-protocol line — print to console
                    finish_progress(&mut cli_progress);
                    println!("[fast-path] {line}");
                }
            }
        }
    }

    // Wait for process exit
    let status = child
        .wait()
        .await
        .map_err(|e| PythonBridgeError::ProcessFailed(e.to_string()))?;

    finish_progress(&mut cli_progress);
    if let Some(p) = xet_poller {
        p.shutdown();
    }

    let stderr_buf = stderr_task.await.unwrap_or_default();
    let stderr_text = String::from_utf8_lossy(&stderr_buf).trim().to_string();

    if !status.success() {
        let reason = if stderr_text.is_empty() {
            format!("exited with status {status}")
        } else {
            stderr_text
        };
        return Err(PythonBridgeError::ProcessFailed(reason));
    }

    Ok(())
}

// ============================================================================
// Event Handling
// ============================================================================

fn handle_event(
    event: PythonEvent,
    request: &FastDownloadRequest<'_>,
    cli_progress: &mut Option<CliProgressPrinter>,
    xet_poller: Option<&XetPoller>,
) -> Result<(), PythonBridgeError> {
    match event {
        PythonEvent::Progress {
            file,
            downloaded,
            total,
        } => {
            // A real Python event arrived — keep the stat fallback dormant.
            if let Some(p) = xet_poller {
                p.note_real_event();
            }
            if let Some(cb) = request.progress.as_ref() {
                cb(downloaded, total);
            } else if let Some(printer) = cli_progress.as_mut() {
                printer.update(file.as_deref(), downloaded, total);
            }
            Ok(())
        }

        PythonEvent::Unavailable { reason } => Err(PythonBridgeError::Unavailable(reason)),

        PythonEvent::Error { message } => Err(PythonBridgeError::ProcessFailed(message)),

        PythonEvent::Complete => Ok(()),
    }
}

fn finish_progress(printer: &mut Option<CliProgressPrinter>) {
    if let Some(p) = printer.as_mut() {
        p.finish();
    }
}
