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

use thiserror::Error;
use tokio::io::{AsyncBufReadExt, AsyncReadExt, BufReader};
use tokio::process::Command;
use tokio::signal;
use tokio_util::sync::CancellationToken;

use super::progress::CliProgressPrinter;
use super::python_env::{EnvSetupError, PythonEnvironment};
use super::python_protocol::{ProtocolError, PythonEvent, parse_line};

// ============================================================================
// Types
// ============================================================================

/// Callback for download progress: (`downloaded_bytes`, `total_bytes`)
pub type ProgressCallback = Box<dyn Fn(u64, u64) + Send + Sync>;

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
    pub progress: Option<&'a ProgressCallback>,
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
    let mut cmd = Command::new(env.python_path());
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
        .env("HF_HUB_DISABLE_TELEMETRY", "1");

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
                return Err(PythonBridgeError::Cancelled);
            }

            // Ctrl+C from terminal
            _ = &mut ctrl_c => {
                let _ = child.kill().await;
                finish_progress(&mut cli_progress);
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
                    match handle_event(event, request, &mut cli_progress) {
                        Ok(()) => {}
                        Err(e) => {
                            let _ = child.kill().await;
                            finish_progress(&mut cli_progress);
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
) -> Result<(), PythonBridgeError> {
    match event {
        PythonEvent::Progress {
            file,
            downloaded,
            total,
        } => {
            if let Some(cb) = request.progress {
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
