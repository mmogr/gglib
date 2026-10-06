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
//! - Registration is deferred to the manager after shard group completion

use std::fmt::Write;
use std::path::PathBuf;
use std::sync::Arc;

use tokio::sync::watch;
use tokio_util::sync::CancellationToken;

use gglib_core::download::{DownloadError, DownloadEvent, DownloadId, Quantization};
use gglib_core::ports::{AppEventEmitter, DownloadManagerConfig};

use crate::executor::{DownloadPlan, FileProgress, download_file};

use super::{app_event, paths::DownloadDestination};

/// Dependencies for the download worker.
///
/// These are cloned Arc references to ports, allowing the worker
/// to operate independently of the manager's state.
///
/// `event_emitter` is a narrow, deliberate exception to "worker only writes
/// to `watch::Sender`" above: it exists solely so `execute_download` can
/// surface [`DownloadEvent::DownloadNotice`] — a one-off, cosmetic note
/// (e.g. "preparing fast downloader") that isn't part of the progress or
/// completion state the manager sequences. Progress and terminal events
/// still flow exclusively through the watch channel and `finalize_job`.
#[derive(Clone)]
pub(crate) struct WorkerDeps {
    /// Configuration (models directory, HF token, etc.).
    pub config: DownloadManagerConfig,
    /// Event sink for [`DownloadEvent::DownloadNotice`] only.
    pub event_emitter: Arc<dyn AppEventEmitter>,
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
    /// Monotonically increasing sequence number for change detection.
    pub seq: u64,
}

impl ProgressUpdate {
    /// Test-only: production seeds the watch channel with `default()` and moves
    /// it forward with `send_modify`, never constructing one this way. Gated so
    /// `dead_code` keeps telling the truth about production reach.
    #[cfg(test)]
    pub(crate) fn new(downloaded: u64, total: u64, seq: u64) -> Self {
        Self {
            progress: FileProgress {
                bytes: downloaded,
                wire: downloaded,
                size: crate::executor::known_size(Some(total)),
            },
            notice: None,
            seq,
        }
    }

    /// Take the file's latest progress.
    ///
    /// A notice is dropped once bytes arrive again, on disk or off the
    /// network: it stood in for progress, and now there is some. A count
    /// that falls is the announced restart, so the notice that announced it
    /// stays.
    fn advance(&mut self, progress: FileProgress) {
        if progress.bytes > self.progress.bytes || progress.wire > self.progress.wire {
            self.notice = None;
        }
        self.progress = progress;
        self.seq += 1;
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
/// The bridge task (spawned by the manager) handles event emission.
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

    // Notice callback: surfaces transient notes that carry no byte progress of
    // their own — e.g. the accelerator being unavailable and the transfer
    // falling back — instead of leaving the bar looking frozen. See the doc
    // comment on `WorkerDeps`.
    let notice_id = job.id.to_string();
    let notice_emitter = Arc::clone(&deps.event_emitter);
    let notice_tx = job.progress_tx.clone();
    let notice_callback: crate::cli_exec::NoticeCallback = Arc::new(move |message: &str| {
        notice_tx.send_modify(|state| state.notice = Some(message.to_string()));
        notice_emitter.emit(app_event(DownloadEvent::DownloadNotice {
            id: notice_id.clone(),
            message: message.to_string(),
        }));
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
