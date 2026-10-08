//! Download worker pipeline.
//!
//! This module contains the core download execution logic, isolated from
//! the queue orchestration. The worker operates on value types and cloned
//! Arc dependencies, with no access to the manager's queue locks.
//!
//! # Design Principles
//!
//! - Worker receives a `DownloadJob` (value type) and `WorkerDeps` (cloned Arcs)
//! - Worker only writes to `watch::Sender`, progress and notes alike, and never emits events
//! - Cancellation is handled via `tokio::select!` around IO operations
//! - Registration is deferred to the manager after shard group completion

use std::fmt::Write;
use std::path::PathBuf;
use std::sync::Arc;

use tokio::sync::watch;
use tokio_util::sync::CancellationToken;

use gglib_core::download::{DownloadError, DownloadId, Quantization};
use gglib_core::ports::DownloadManagerConfig;

use crate::executor::{DownloadPlan, FileProgress, download_file};

use super::paths::DownloadDestination;

/// Dependencies for the download worker.
///
/// The worker operates independently of the manager's state: what it has to
/// say, progress or a note, it writes to the job's `watch::Sender`.
#[derive(Clone)]
pub(crate) struct WorkerDeps {
    /// Configuration: the Hub token the transfer asks with. Where the file
    /// goes is the job's `destination`.
    pub config: DownloadManagerConfig,
}

/// A download job to be executed by the worker.
///
/// This is a value type containing all information needed to execute
/// a download, with no references back to the manager.
pub(crate) struct DownloadJob {
    /// The download ID.
    pub id: DownloadId,
    /// Planned destination (model directory + files).
    pub destination: DownloadDestination,
    /// Git revision/tag/commit (e.g., "main", "v1.0", SHA).
    pub revision: Option<String>,
    /// Cancellation token for this job.
    pub cancel: CancellationToken,
    /// Progress sender for this job.
    pub progress_tx: watch::Sender<ProgressUpdate>,
    /// The file's size from HF metadata, if known. It sizes the bar before
    /// the first byte arrives.
    pub expected_size: Option<u64>,
}

/// Progress update sent through the watch channel.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(crate) struct ProgressUpdate {
    /// How far the job's file has got.
    pub progress: FileProgress,
    /// A note standing in for progress while there is none to show, e.g. the
    /// accelerator's environment being prepared.
    pub notice: Option<String>,
}

impl ProgressUpdate {
    /// Take the file's latest progress.
    ///
    /// A notice is dropped once bytes arrive again, on disk or off the
    /// network: it stood in for progress, and now there is some. A count
    /// that falls is the announced restart, so the notice that announced it
    /// stays.
    pub(crate) fn advance(&mut self, progress: FileProgress) {
        if progress.bytes > self.progress.bytes || progress.wire > self.progress.wire {
            self.notice = None;
        }
        self.progress = progress;
    }

    /// Take a note on the file. It is shown in place of progress until
    /// bytes arrive again.
    pub(crate) fn note(&mut self, message: &str) {
        self.notice = Some(message.to_string());
    }
}

/// Result of a successful download.
#[derive(Debug, Clone)]
pub(crate) struct CompletedJob {
    /// Path to the primary downloaded file.
    pub primary_path: PathBuf,
    /// All downloaded file paths.
    pub all_paths: Vec<PathBuf>,
    /// Repository ID for model registration.
    pub repo_id: String,
    /// Commit SHA for model registration.
    pub commit_sha: String,
    /// Quantization for model registration.
    pub quantization: Quantization,
    /// List of file names downloaded.
    pub files: Vec<String>,
}

/// Percent-encode a revision string to safely use in `model_key`.
///
/// Encodes characters that could cause ambiguity in the key format:
/// - `/` → `%2F` (branch names like `feature/branch`)
/// - `#` → `%23` (could conflict with filename delimiter)
/// - `@` → `%40` (could conflict with revision delimiter)
///
/// Uses proper UTF-8 byte encoding for all non-ASCII characters.
fn percent_encode_revision(revision: &str) -> String {
    let mut out = String::new();
    for b in revision.as_bytes() {
        match *b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                out.push(*b as char);
            }
            b'/' => out.push_str("%2F"),
            b'#' => out.push_str("%23"),
            b'@' => out.push_str("%40"),
            b => write!(&mut out, "%{b:02X}").unwrap(),
        }
    }
    out
}

/// Run a download job to completion.
///
/// This function executes the full download pipeline:
/// 1. Ensures destination directory exists
/// 2. Downloads files with progress reporting
///
/// Progress is reported through `job.progress_tx` only; no events are emitted.
/// The meter task (spawned by the manager) samples it and publishes the queue.
/// Model registration is deferred to the manager after all shards complete.
///
/// # Cancellation
///
/// The job can be cancelled via `job.cancel`. When cancelled, this returns
/// `Err(DownloadError::Cancelled)`.
pub(crate) async fn run_job(
    job: DownloadJob,
    deps: &WorkerDeps,
) -> Result<CompletedJob, DownloadError> {
    // Step 1: Ensure destination directory exists
    job.destination.ensure_dir()?;

    // Step 2: Execute download with cancellation support
    let download_result = execute_download(&job, deps).await;

    // Handle cancellation or errors
    let () = download_result?;

    // Step 3: Prepare result with metadata for manager
    let primary_path = job
        .destination
        .primary_path()
        .ok_or_else(|| DownloadError::other("No files in download"))?;
    let all_paths = job.destination.all_paths();

    // Extract metadata from download ID
    let repo_id = job.id.model_id().to_string();
    let commit_sha = job.revision.as_ref().map_or_else(
        || "rev:main".to_string(),
        |r| format!("rev:{}", percent_encode_revision(r)),
    );
    let quantization = job.id.quantization().map_or_else(
        || {
            job.destination
                .files
                .first()
                .map_or(Quantization::Unknown, |f| Quantization::from_filename(f))
        },
        Quantization::from_filename,
    );

    Ok(CompletedJob {
        primary_path,
        all_paths,
        repo_id,
        commit_sha,
        quantization,
        files: job.destination.files.clone(),
    })
}

/// Execute the actual file download with progress and cancellation.
async fn execute_download(job: &DownloadJob, deps: &WorkerDeps) -> Result<(), DownloadError> {
    // Create progress callback that updates watch channel
    let progress_tx = job.progress_tx.clone();
    let progress_callback: crate::cli_exec::ProgressCallback =
        Arc::new(move |progress: FileProgress| {
            // send_modify avoids clone and is infallible
            progress_tx.send_modify(|state| state.advance(progress));
        });

    // Notice callback: a transient note that carries no byte progress of its
    // own — e.g. the accelerator being unavailable and the transfer falling
    // back. It rides the watch channel beside the progress, and is shown on
    // the download's row until bytes arrive again.
    let notice_tx = job.progress_tx.clone();
    let notice_callback: crate::cli_exec::NoticeCallback = Arc::new(move |message: &str| {
        notice_tx.send_modify(|state| state.note(message));
    });

    // A job is one file of its download.
    let Some(file) = job.destination.files.first() else {
        return Ok(());
    };

    // Build download plan
    let plan = DownloadPlan {
        repo_id: job.id.model_id(),
        revision: "main",
        destination: &job.destination.model_dir,
        file,
        token: deps.config.hf_token.as_deref(),
        force: false,
        progress: Some(progress_callback),
        notice: Some(notice_callback),
        expected_size: job.expected_size,
        cancel: Some(job.cancel.clone()),
    };

    // Execute with cancellation support via select
    tokio::select! {
        biased;

        () = job.cancel.cancelled() => {
            Err(DownloadError::Cancelled)
        }

        result = download_file(&plan) => result,
    }
}

#[cfg(test)]
#[path = "worker_tests.rs"]
mod tests;
