//! Download worker pipeline.
//!
//! This module contains the core download execution logic, isolated from
//! the queue orchestration. The worker operates on value types and cloned
//! Arc dependencies, with no access to the manager's queue locks.
//!
//! # Design Principles
//!
//! - Worker receives a `DownloadJob` (value type) and `WorkerDeps` (cloned Arcs)
//! - Worker only writes to `watch::Sender` for progress, never emits events directly
//! - Cancellation is handled via `tokio::select!` around IO operations
//! - Registration errors are soft-fail (logged, don't fail the download)

use std::path::{Path, PathBuf};
use std::sync::Arc;

use tokio::sync::watch;
use tokio_util::sync::CancellationToken;

use gglib_core::download::{DownloadError, DownloadId, Quantization};
use gglib_core::ports::{CompletedDownload, DownloadManagerConfig, ModelRegistrarPort};

use crate::cli_exec::{FastDownloadRequest, PythonBridgeError, run_fast_download};

use super::paths::DownloadDestination;

/// Dependencies for the download worker.
///
/// These are cloned Arc references to ports, allowing the worker
/// to operate independently of the manager's state.
#[derive(Clone)]
pub struct WorkerDeps {
    /// Port for registering completed downloads as models.
    pub model_registrar: Arc<dyn ModelRegistrarPort>,
    /// Configuration (models directory, HF token, etc.).
    pub config: DownloadManagerConfig,
}

/// A download job to be executed by the worker.
///
/// This is a value type containing all information needed to execute
/// a download, with no references back to the manager.
pub struct DownloadJob {
    /// The download ID.
    pub id: DownloadId,
    /// Planned destination (model directory + files).
    pub destination: DownloadDestination,
    /// Cancellation token for this job.
    pub cancel: CancellationToken,
    /// Progress sender for this job.
    pub progress_tx: watch::Sender<ProgressUpdate>,
}

/// Progress update sent through the watch channel.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ProgressUpdate {
    /// Bytes downloaded so far.
    pub downloaded: u64,
    /// Total bytes to download.
    pub total: u64,
    /// Monotonically increasing sequence number for change detection.
    pub seq: u64,
}

impl ProgressUpdate {
    /// Create a new progress update with a sequence number.
    pub const fn new(downloaded: u64, total: u64, seq: u64) -> Self {
        Self {
            downloaded,
            total,
            seq,
        }
    }
}

/// Result of a successful download.
#[derive(Debug, Clone)]
pub struct CompletedJob {
    /// The download ID.
    pub id: DownloadId,
    /// Path to the primary downloaded file.
    pub primary_path: PathBuf,
    /// All downloaded file paths.
    pub all_paths: Vec<PathBuf>,
}

/// Run a download job to completion.
///
/// This function executes the full download pipeline:
/// 1. Ensures destination directory exists
/// 2. Downloads files with progress reporting
/// 3. Registers the completed model (soft-fail on error)
///
/// Progress is reported through `job.progress_tx` only; no events are emitted.
/// The bridge task (spawned by the manager) handles event emission.
///
/// # Cancellation
///
/// The job can be cancelled via `job.cancel`. When cancelled, this returns
/// `Err(DownloadError::Cancelled)`.
pub async fn run_job(job: DownloadJob, deps: &WorkerDeps) -> Result<CompletedJob, DownloadError> {
    // Step 1: Ensure destination directory exists
    job.destination.ensure_dir()?;

    // Step 2: Execute download with cancellation support
    let download_result = execute_download(&job, deps).await;

    // Handle cancellation or errors
    let () = download_result?;

    // Step 3: Register completed model (soft-fail)
    let primary_path = job
        .destination
        .primary_path()
        .ok_or_else(|| DownloadError::other("No files in download"))?;
    let all_paths = job.destination.all_paths();

    register_model(
        &deps.model_registrar,
        &job.id,
        &primary_path,
        &all_paths,
        &job.destination.files,
    )
    .await;

    Ok(CompletedJob {
        id: job.id,
        primary_path,
        all_paths,
    })
}

/// Execute the actual file download with progress and cancellation.
async fn execute_download(job: &DownloadJob, deps: &WorkerDeps) -> Result<(), DownloadError> {
    use std::sync::atomic::{AtomicU64, Ordering};

    // Sequence counter for progress updates
    let seq = Arc::new(AtomicU64::new(0));

    // Create progress callback that updates watch channel
    let progress_tx = job.progress_tx.clone();
    let seq_clone = Arc::clone(&seq);
    let progress_callback: Box<dyn Fn(u64, u64) + Send + Sync> =
        Box::new(move |downloaded: u64, total: u64| {
            let current_seq = seq_clone.fetch_add(1, Ordering::Relaxed);
            // send_modify avoids clone and is infallible
            progress_tx.send_modify(|state| {
                state.downloaded = downloaded;
                state.total = total;
                state.seq = current_seq + 1;
            });
        });

    // Build download request
    let request = FastDownloadRequest {
        repo_id: job.id.model_id(),
        revision: "main",
        repo_type: "model",
        destination: &job.destination.model_dir,
        files: &job.destination.files,
        token: deps.config.hf_token.as_deref(),
        force: false,
        progress: Some(&progress_callback),
        cancel_token: Some(job.cancel.clone()),
    };

    // Execute with cancellation support via select
    tokio::select! {
        biased;

        () = job.cancel.cancelled() => {
            Err(DownloadError::Cancelled)
        }

        result = run_fast_download(&request) => {
            result.map_err(|e| match e {
                PythonBridgeError::Cancelled => DownloadError::Cancelled,
                other => DownloadError::other(other.to_string()),
            })
        }
    }
}

/// Register a completed download as a model (soft-fail).
///
/// Logs success or warning but never fails the download.
async fn register_model(
    registrar: &Arc<dyn ModelRegistrarPort>,
    id: &DownloadId,
    primary_path: &Path,
    all_paths: &[PathBuf],
    files: &[String],
) {
    // Determine quantization from ID or filename
    let quantization = id.quantization().map_or_else(
        || {
            files
                .first()
                .map_or(Quantization::Unknown, |f| Quantization::from_filename(f))
        },
        Quantization::from_filename,
    );

    let completed = CompletedDownload {
        primary_path: primary_path.to_path_buf(),
        all_paths: all_paths.to_vec(),
        quantization,
        repo_id: id.model_id().to_string(),
        commit_sha: "main".to_string(), // TODO: capture actual commit SHA
        is_sharded: files.len() > 1,
        total_bytes: 0, // TODO: track total bytes
    };

    match registrar.register_model(&completed).await {
        Ok(model) => {
            tracing::info!(
                download_id = %id,
                model_id = model.id,
                model_name = %model.name,
                "Model registered successfully"
            );
        }
        Err(e) => {
            tracing::warn!(
                download_id = %id,
                error = %e,
                path = %primary_path.display(),
                "Failed to register model - file downloaded but won't appear in library"
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn progress_update_new_creates_with_seq() {
        let update = ProgressUpdate::new(100, 1000, 5);
        assert_eq!(update.downloaded, 100);
        assert_eq!(update.total, 1000);
        assert_eq!(update.seq, 5);
    }

    #[test]
    fn progress_update_default_is_zero() {
        let update = ProgressUpdate::default();
        assert_eq!(update.downloaded, 0);
        assert_eq!(update.total, 0);
        assert_eq!(update.seq, 0);
    }
}
