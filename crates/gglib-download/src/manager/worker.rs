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

use gglib_core::download::{DownloadError, DownloadId, Quantization};
use gglib_core::ports::DownloadManagerConfig;

use crate::cli_exec::{FastDownloadRequest, PythonBridgeError, run_fast_download};

use super::paths::DownloadDestination;

/// Dependencies for the download worker.
///
/// These are cloned Arc references to ports, allowing the worker
/// to operate independently of the manager's state.
#[derive(Clone)]
pub struct WorkerDeps {
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
    /// Git revision/tag/commit (e.g., "main", "v1.0", SHA).
    pub revision: Option<String>,
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
pub async fn run_job(job: DownloadJob, deps: &WorkerDeps) -> Result<CompletedJob, DownloadError> {
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
        id: job.id,
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
        force: true,
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

    #[test]
    fn test_percent_encode_revision() {
        // Normal alphanumeric revisions pass through
        assert_eq!(percent_encode_revision("main"), "main");
        assert_eq!(percent_encode_revision("v1.0.2"), "v1.0.2");
        assert_eq!(percent_encode_revision("abc123-def"), "abc123-def");

        // Branch names with slashes
        assert_eq!(
            percent_encode_revision("feature/branch"),
            "feature%2Fbranch"
        );
        assert_eq!(percent_encode_revision("hotfix/v1.2"), "hotfix%2Fv1.2");

        // Special characters that could cause ambiguity
        assert_eq!(percent_encode_revision("tag#123"), "tag%23123");
        assert_eq!(percent_encode_revision("user@commit"), "user%40commit");

        // Complex case
        assert_eq!(
            percent_encode_revision("feature/test@v1#fix"),
            "feature%2Ftest%40v1%23fix"
        );

        // Unicode (UTF-8 encoding)
        let encoded = percent_encode_revision("café/模型#x@y");
        assert!(encoded.contains("%C3%A9"), "Should contain UTF-8 encoded é");
        assert!(encoded.contains("%2F"), "Should encode /");
        assert!(encoded.contains("%23"), "Should encode #");
        assert!(encoded.contains("%40"), "Should encode @");
        // Verify it encodes the CJK character (模 = E6 A8 A1 in UTF-8)
        assert!(
            encoded.contains("%E6%A8%A1"),
            "Should contain UTF-8 encoded 模"
        );

        // Full verification
        assert_eq!(percent_encode_revision("café"), "caf%C3%A9");
    }
}
