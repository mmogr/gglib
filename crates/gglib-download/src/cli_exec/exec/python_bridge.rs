//! Python-based fast download orchestrator.
//!
//! This module coordinates the fast download process using:
//! - `python_env`: Environment setup (venv, requirements, script)
//! - `python_protocol`: JSON message parsing
//!
//! The orchestrator spawns one Python subprocess per file, streams its
//! output, and hands each progress line to the caller as a reading.

use std::path::Path;
use std::sync::Arc;

use gglib_core::utils::process::async_cmd;
use thiserror::Error;
use tokio::io::{AsyncBufReadExt, AsyncRead, AsyncReadExt, BufReader};
use tokio::process::Command;
use tokio::signal;
use tokio_util::sync::CancellationToken;

use super::python_env::{EnvSetupError, PythonEnvironment};
use super::python_protocol::{ProtocolError, PythonEvent, parse_line};
use crate::executor::{RawCallback, RawProgress};

// ============================================================================
// Types
// ============================================================================

/// Callback for a transient setup notice, e.g. "preparing fast downloader".
///
/// Fires at most a handful of times per download (venv creation, dependency
/// install). It carries no byte counts, just a message to display in their
/// place.
pub type NoticeCallback = Arc<dyn Fn(&str) + Send + Sync>;

// ============================================================================
// Constants
// ============================================================================

const CANCELLED_MSG: &str = "fast download cancelled by user";

/// How much of the helper's stderr is kept as the text of a failure.
///
/// The end of it, where a traceback names what went wrong. The helper's
/// progress bar writes to stderr for as long as a transfer runs, so the whole
/// of it is unbounded.
const STDERR_TAIL_BYTES: usize = 4096;

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
    /// The one file to fetch: its path within the repository.
    pub file: &'a str,
    /// Hub token. Handed to the helper in its environment, not in its
    /// arguments.
    pub token: Option<&'a str>,
    pub force: bool,
    /// Sink for the helper's readings of the file.
    pub progress: Option<RawCallback>,
    /// Sink for transient setup notices (env creation, dependency install).
    /// `None` routes those notices through `console_println` instead — see
    /// `PythonEnvironment::prepare`.
    pub notice: Option<NoticeCallback>,
    /// Cancellation token for external cancellation.
    pub cancel_token: Option<CancellationToken>,
}

// ============================================================================
// Public API
// ============================================================================

/// Ensure the fast download helper is ready (env + script prepared).
pub async fn ensure_fast_helper_ready() -> Result<(), PythonBridgeError> {
    ensure_fast_helper_ready_with_python(None).await
}

/// Ensure the fast download helper is ready, built from a named interpreter.
///
/// `python` outranks the interpreter search entirely, so a user who knows
/// which Python they want does not have to arrange for it to win a search.
pub async fn ensure_fast_helper_ready_with_python(
    python: Option<&std::path::Path>,
) -> Result<(), PythonBridgeError> {
    PythonEnvironment::prepare_with(None, python).await?;
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
    let env = PythonEnvironment::prepare(request.notice.as_ref()).await?;

    run_download_process(&env, request).await
}

// ============================================================================
// Process Orchestration
// ============================================================================

/// The helper's command line and environment for `request`.
fn helper_command(python: &Path, script: &Path, request: &FastDownloadRequest<'_>) -> Command {
    let mut cmd = async_cmd(python);
    cmd.arg(script)
        .arg("--repo-id")
        .arg(request.repo_id)
        .arg("--revision")
        .arg(request.revision)
        .arg("--repo-type")
        .arg(request.repo_type)
        .arg("--dest")
        .arg(request.destination)
        .arg("--file")
        .arg(request.file)
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

    // In the environment, where the Hub library reads it, and not on the
    // command line.
    if let Some(token) = request.token {
        cmd.env("HF_TOKEN", token);
    }
    if request.force {
        cmd.arg("--force");
    }

    cmd
}

async fn run_download_process(
    env: &PythonEnvironment,
    request: &FastDownloadRequest<'_>,
) -> Result<(), PythonBridgeError> {
    let mut cmd = helper_command(&env.python_path(), env.script_path(), request);

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
    let stderr_task = tokio::spawn(read_tail(stderr, STDERR_TAIL_BYTES));

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
                return Err(PythonBridgeError::Cancelled);
            }

            // Ctrl+C from terminal
            _ = &mut ctrl_c => {
                let _ = child.kill().await;
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
                    if let Err(e) = handle_event(event, request) {
                        let _ = child.kill().await;
                        return Err(e);
                    }
                } else {
                    // Non-protocol line — print to console via the shared
                    // hook so it doesn't corrupt a live MultiProgress redraw
                    // (see gglib_core::telemetry::console_println).
                    gglib_core::telemetry::console_println(&format!("[fast-path] {line}"));
                }
            }
        }
    }

    // Wait for process exit
    let status = child
        .wait()
        .await
        .map_err(|e| PythonBridgeError::ProcessFailed(e.to_string()))?;

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
) -> Result<(), PythonBridgeError> {
    match event {
        PythonEvent::Progress {
            written,
            received,
            total,
        } => {
            if let Some(cb) = request.progress.as_ref() {
                cb(RawProgress::new(
                    written,
                    received,
                    (total > 0).then_some(total),
                ));
            }
            Ok(())
        }

        PythonEvent::Unavailable { reason } => Err(PythonBridgeError::Unavailable(reason)),

        PythonEvent::Error { message } => Err(PythonBridgeError::ProcessFailed(message)),

        PythonEvent::Complete => Ok(()),
    }
}

/// Read `reader` to its end, keeping only its last `keep` bytes.
async fn read_tail(mut reader: impl AsyncRead + Unpin, keep: usize) -> Vec<u8> {
    let mut tail = Vec::new();
    let mut chunk = [0_u8; 4096];
    while let Ok(read) = reader.read(&mut chunk).await {
        if read == 0 {
            break;
        }
        tail.extend_from_slice(&chunk[..read]);
        if tail.len() > keep {
            tail.drain(..tail.len() - keep);
        }
    }
    tail
}

#[cfg(test)]
#[path = "python_bridge_tests.rs"]
mod tests;
