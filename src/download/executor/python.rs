//! Python-based download executor.
//!
//! Concrete implementation that uses the embedded Python `hf_xet_downloader.py`
//! script for fast downloads via the `hf_transfer` library.

use std::path::PathBuf;
use std::sync::Arc;

use thiserror::Error;
use tokio::io::{AsyncBufReadExt, AsyncReadExt, BufReader};
use tokio::process::Command;
use tokio::signal;
use tokio_util::sync::CancellationToken;

use crate::commands::download::python_env::{EnvSetupError, PythonEnvironment};
use crate::commands::download::python_protocol::{PythonEvent, parse_line};
use crate::download::domain::errors::DownloadError;
use crate::download::domain::events::DownloadEvent;
use crate::download::domain::types::{DownloadId, DownloadRequest, ShardInfo};
use crate::download::progress::ProgressContext;
use crate::services::core::PidStorage;

// ============================================================================
// Public Types
// ============================================================================

/// Callback for receiving download events.
pub type EventCallback = Arc<dyn Fn(DownloadEvent) + Send + Sync>;

/// Configuration for a shard group download (multiple shards, same model).
#[derive(Debug, Clone)]
pub struct ShardGroup {
    /// The parent download ID.
    pub download_id: DownloadId,
    /// List of shard files to download.
    pub files: Vec<ShardFile>,
    /// Total size across all shards.
    pub total_size: u64,
}

/// A single shard file within a group.
#[derive(Debug, Clone)]
pub struct ShardFile {
    /// Filename of this shard.
    pub filename: String,
    /// Size of this shard in bytes.
    pub size: u64,
}

/// Result of executing a download.
#[derive(Debug)]
pub enum ExecutionResult {
    /// Download completed successfully.
    Completed,
    /// Download was cancelled.
    Cancelled,
}

/// Errors during execution.
#[derive(Debug, Error)]
pub enum ExecutionError {
    #[error("Environment setup failed: {0}")]
    EnvironmentSetup(#[from] EnvSetupError),

    #[error("Process failed: {0}")]
    ProcessFailed(String),

    #[error("Resource unavailable: {0}")]
    Unavailable(String),
}

impl From<ExecutionError> for DownloadError {
    fn from(err: ExecutionError) -> Self {
        match err {
            ExecutionError::EnvironmentSetup(e) => DownloadError::environment(e.to_string()),
            ExecutionError::ProcessFailed(msg) => DownloadError::process_failed(msg),
            ExecutionError::Unavailable(msg) => DownloadError::not_found(msg),
        }
    }
}

// ============================================================================
// Executor
// ============================================================================

/// Concrete Python-based download executor.
///
/// Uses the embedded `hf_xet_downloader.py` script for fast downloads.
/// No trait abstraction - Python/hf_xet is the only supported backend.
#[derive(Clone)]
pub struct PythonDownloadExecutor {
    /// PID storage for process tracking.
    pid_storage: Option<PidStorage>,
}

impl PythonDownloadExecutor {
    /// Create a new executor.
    pub fn new() -> Self {
        Self { pid_storage: None }
    }

    /// Create a new executor with PID storage for process tracking.
    pub fn with_pid_storage(pid_storage: PidStorage) -> Self {
        Self {
            pid_storage: Some(pid_storage),
        }
    }

    /// Ensure the Python environment is ready.
    ///
    /// Call this during app initialization to avoid first-download latency.
    pub async fn prepare_environment(&self) -> Result<(), ExecutionError> {
        PythonEnvironment::prepare().await?;
        Ok(())
    }

    /// Execute a download request.
    ///
    /// This method handles single-file downloads. For sharded models,
    /// use `execute_shard_group` instead.
    pub async fn execute(
        &self,
        request: &DownloadRequest,
        on_event: &EventCallback,
        cancel_token: CancellationToken,
    ) -> Result<ExecutionResult, ExecutionError> {
        let ctx = ProgressContext::new(request.id.to_string());
        on_event(ctx.build_started());

        let result = self
            .run_download(
                &request.repo_id,
                request.revision.as_deref().unwrap_or("main"),
                &request.destination,
                &request.files,
                request.token.as_deref(),
                request.force,
                &ctx,
                on_event,
                cancel_token.clone(),
                Some(request.id.to_string()),
            )
            .await;

        match result {
            Ok(ExecutionResult::Completed) => {
                on_event(ctx.build_completed(None));
                Ok(ExecutionResult::Completed)
            }
            Ok(ExecutionResult::Cancelled) => {
                on_event(ctx.build_cancelled());
                Ok(ExecutionResult::Cancelled)
            }
            Err(e) => {
                on_event(ctx.build_failed(&e.to_string()));
                Err(e)
            }
        }
    }

    /// Execute a shard group download.
    ///
    /// Downloads multiple shards sequentially, emitting aggregate progress.
    pub async fn execute_shard_group(
        &self,
        group: &ShardGroup,
        request: &DownloadRequest,
        on_event: &EventCallback,
        cancel_token: CancellationToken,
    ) -> Result<ExecutionResult, ExecutionError> {
        let total_shards = group.files.len();
        let mut completed_size: u64 = 0;

        for (index, shard) in group.files.iter().enumerate() {
            let shard_info = ShardInfo::new(index as u32, total_shards as u32, &shard.filename);
            let ctx = ProgressContext::new_sharded(
                request.id.to_string(),
                shard_info.clone(),
                group.total_size,
                completed_size,
            );

            // Emit started for first shard only
            if index == 0 {
                on_event(ctx.build_started());
            }

            let result = self
                .run_download(
                    &request.repo_id,
                    request.revision.as_deref().unwrap_or("main"),
                    &request.destination,
                    std::slice::from_ref(&shard.filename),
                    request.token.as_deref(),
                    request.force,
                    &ctx,
                    on_event,
                    cancel_token.clone(),
                    Some(format!("{}:{}", request.id, index)),
                )
                .await?;

            if matches!(result, ExecutionResult::Cancelled) {
                on_event(ctx.build_cancelled());
                return Ok(ExecutionResult::Cancelled);
            }

            completed_size += shard.size;
        }

        // Emit final completion
        let ctx = ProgressContext::new(request.id.to_string());
        on_event(ctx.build_completed(Some(&format!("Downloaded {} shards", total_shards))));

        Ok(ExecutionResult::Completed)
    }

    /// Internal download runner.
    #[allow(clippy::too_many_arguments)]
    async fn run_download(
        &self,
        repo_id: &str,
        revision: &str,
        destination: &PathBuf,
        files: &[String],
        token: Option<&str>,
        force: bool,
        ctx: &ProgressContext,
        on_event: &EventCallback,
        cancel_token: CancellationToken,
        pid_key: Option<String>,
    ) -> Result<ExecutionResult, ExecutionError> {
        if files.is_empty() {
            return Ok(ExecutionResult::Completed);
        }

        let env = PythonEnvironment::prepare().await?;

        let mut cmd = Command::new(env.python_path());
        cmd.arg(env.script_path())
            .arg("--repo-id")
            .arg(repo_id)
            .arg("--revision")
            .arg(revision)
            .arg("--repo-type")
            .arg("model")
            .arg("--dest")
            .arg(destination)
            .kill_on_drop(true)
            .env("PYTHONUNBUFFERED", "1")
            .env("HF_HUB_DISABLE_TELEMETRY", "1");

        if let Some(token) = token {
            cmd.arg("--token").arg(token);
        }
        if force {
            cmd.arg("--force");
        }
        for file in files {
            cmd.arg("--file").arg(file);
        }

        let mut child = cmd
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped())
            .spawn()
            .map_err(|e| ExecutionError::ProcessFailed(format!("Failed to spawn: {e}")))?;

        // Store PID for tracking
        let pid = child.id();
        let pid_key_for_cleanup = pid_key.clone();
        let pid_storage_for_cleanup = self.pid_storage.clone();

        if let (Some(storage), Some(key), Some(pid)) = (&self.pid_storage, &pid_key, pid) {
            tracing::info!(pid = pid, key = %key, "Storing Python subprocess PID");
            if let Ok(mut guard) = storage.write() {
                guard.insert(key.clone(), pid);
            }
        }

        let cleanup_pid = || {
            #[allow(clippy::collapsible_if)]
            if let (Some(storage), Some(key)) = (&pid_storage_for_cleanup, &pid_key_for_cleanup) {
                if let Ok(mut guard) = storage.write() {
                    guard.remove(key);
                    tracing::debug!(key = %key, "Removed PID from tracking");
                }
            }
        };

        let stdout = child
            .stdout
            .take()
            .ok_or_else(|| ExecutionError::ProcessFailed("Missing stdout".to_string()))?;
        let stderr = child
            .stderr
            .take()
            .ok_or_else(|| ExecutionError::ProcessFailed("Missing stderr".to_string()))?;

        let mut lines = BufReader::new(stdout).lines();
        let mut stderr_reader = BufReader::new(stderr);
        let stderr_task = tokio::spawn(async move {
            let mut buf = Vec::new();
            let _ = stderr_reader.read_to_end(&mut buf).await;
            buf
        });

        let mut ctrl_c = Box::pin(signal::ctrl_c());

        // Event loop
        loop {
            tokio::select! {
                // External cancellation
                _ = cancel_token.cancelled() => {
                    cleanup_pid();
                    let _ = child.kill().await;
                    return Ok(ExecutionResult::Cancelled);
                }

                // Ctrl+C from terminal
                _ = &mut ctrl_c => {
                    cleanup_pid();
                    let _ = child.kill().await;
                    return Ok(ExecutionResult::Cancelled);
                }

                // Process stdout lines
                line = lines.next_line() => {
                    let line = line.map_err(|e| ExecutionError::ProcessFailed(e.to_string()))?;
                    let Some(line) = line else { break; };

                    if line.trim().is_empty() {
                        continue;
                    }

                    match parse_line(&line) {
                        Ok(event) => {
                            self.handle_python_event(event, ctx, on_event)?;
                        }
                        Err(_) => {
                            // Non-protocol line — log it
                            tracing::debug!(line = %line, "Non-protocol output from Python");
                        }
                    }
                }
            }
        }

        // Wait for process exit
        let status = child
            .wait()
            .await
            .map_err(|e| ExecutionError::ProcessFailed(e.to_string()))?;

        cleanup_pid();

        let stderr_buf = stderr_task.await.unwrap_or_default();
        let stderr_text = String::from_utf8_lossy(&stderr_buf).trim().to_string();

        if !status.success() {
            let reason = if stderr_text.is_empty() {
                format!("exited with status {status}")
            } else {
                stderr_text
            };
            return Err(ExecutionError::ProcessFailed(reason));
        }

        Ok(ExecutionResult::Completed)
    }

    /// Handle a Python protocol event.
    fn handle_python_event(
        &self,
        event: PythonEvent,
        ctx: &ProgressContext,
        on_event: &EventCallback,
    ) -> Result<(), ExecutionError> {
        match event {
            PythonEvent::Progress {
                downloaded, total, ..
            } => {
                if let Some(progress_event) = ctx.build_progress(downloaded, total) {
                    on_event(progress_event);
                }
                Ok(())
            }

            PythonEvent::Unavailable { reason } => Err(ExecutionError::Unavailable(reason)),

            PythonEvent::Error { message } => Err(ExecutionError::ProcessFailed(message)),

            PythonEvent::Complete => Ok(()),
        }
    }
}

impl Default for PythonDownloadExecutor {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_executor_creation() {
        let executor = PythonDownloadExecutor::new();
        assert!(executor.pid_storage.is_none());
    }

    #[test]
    fn test_shard_group() {
        let group = ShardGroup {
            download_id: DownloadId::new("test/model", Some("Q4_K_M")),
            files: vec![
                ShardFile {
                    filename: "model-00001.gguf".to_string(),
                    size: 1000,
                },
                ShardFile {
                    filename: "model-00002.gguf".to_string(),
                    size: 1000,
                },
            ],
            total_size: 2000,
        };

        assert_eq!(group.files.len(), 2);
        assert_eq!(group.total_size, 2000);
    }
}
