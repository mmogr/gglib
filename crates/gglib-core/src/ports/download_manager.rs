//! Download manager port definition.
//!
//! This port defines the public interface for the download subsystem.
//! It abstracts away all implementation details (Python subprocess,
//! cancellation tokens, `HuggingFace` client) behind a clean async API.
//!
//! # Design
//!
//! - Only core download domain types in signatures
//! - No Python types, `CancellationToken`, or HF types leak through
//! - Consistent with other ports (`HfClientPort`, `McpServerRepository`)

use async_trait::async_trait;
use std::path::PathBuf;

use crate::download::{DownloadError, DownloadId, Quantization, QueueSnapshot};

/// Request to queue a new download.
///
/// This is a pure data structure containing all information needed
/// to initiate a download. Infrastructure concerns (tokens, paths)
/// are handled internally by the implementation.
#[derive(Debug, Clone)]
pub struct DownloadRequest {
    /// Repository ID on `HuggingFace` (e.g., `unsloth/Llama-3-GGUF`).
    pub repo_id: String,
    /// The quantization to download.
    pub quantization: Quantization,
    /// Git revision/commit SHA (defaults to "main" if not specified).
    pub revision: Option<String>,
    /// Force re-download even if file exists locally.
    pub force: bool,
    /// Add to local model database after download.
    pub add_to_db: bool,
}

impl DownloadRequest {
    /// Create a new download request with required fields.
    pub fn new(repo_id: impl Into<String>, quantization: Quantization) -> Self {
        Self {
            repo_id: repo_id.into(),
            quantization,
            revision: None,
            force: false,
            add_to_db: true,
        }
    }

    /// Set the revision/commit SHA.
    #[must_use]
    pub fn with_revision(mut self, revision: impl Into<String>) -> Self {
        self.revision = Some(revision.into());
        self
    }

    /// Set whether to force re-download.
    #[must_use]
    pub const fn with_force(mut self, force: bool) -> Self {
        self.force = force;
        self
    }

    /// Set whether to add to database after download.
    #[must_use]
    pub const fn with_add_to_db(mut self, add_to_db: bool) -> Self {
        self.add_to_db = add_to_db;
        self
    }
}

/// Configuration for creating a download manager.
///
/// Contains paths and limits that the download manager needs.
/// Infrastructure-specific options are handled internally.
#[derive(Debug, Clone)]
pub struct DownloadManagerConfig {
    /// Directory where models are stored.
    pub models_directory: PathBuf,
    /// Maximum concurrent downloads.
    pub max_concurrent: u32,
    /// Maximum queue size.
    pub max_queue_size: u32,
    /// `HuggingFace` authentication token (for private repos).
    pub hf_token: Option<String>,
}

impl Default for DownloadManagerConfig {
    fn default() -> Self {
        Self {
            models_directory: PathBuf::from("."),
            max_concurrent: 1,
            max_queue_size: 10,
            hf_token: None,
        }
    }
}

impl DownloadManagerConfig {
    /// Create a new config with the models directory.
    #[must_use]
    pub fn new(models_directory: PathBuf) -> Self {
        Self {
            models_directory,
            ..Default::default()
        }
    }

    /// Set the maximum concurrent downloads.
    #[must_use]
    pub const fn with_max_concurrent(mut self, max: u32) -> Self {
        self.max_concurrent = max;
        self
    }

    /// Set the maximum queue size.
    #[must_use]
    pub const fn with_max_queue_size(mut self, max: u32) -> Self {
        self.max_queue_size = max;
        self
    }

    /// Set the `HuggingFace` token.
    #[must_use]
    pub fn with_hf_token(mut self, token: Option<String>) -> Self {
        self.hf_token = token;
        self
    }
}

/// Port for managing downloads.
///
/// This is the main interface for the download subsystem. Implementations
/// handle all the complexity of queuing, progress tracking, cancellation,
/// and model registration internally.
///
/// # Usage
///
/// ```ignore
/// let manager: Arc<dyn DownloadManagerPort> = /* ... */;
///
/// // Queue a download
/// let request = DownloadRequest::new("unsloth/Llama-3-GGUF", Quantization::Q4KM);
/// let id = manager.queue_download(request).await?;
///
/// // Check status
/// let snapshot = manager.get_queue_snapshot().await?;
///
/// // Cancel if needed
/// manager.cancel_download(&id).await?;
/// ```
use std::sync::Arc;

#[async_trait]
pub trait DownloadManagerPort: Send + Sync {
    /// Queue a new download.
    ///
    /// Returns the download's ID, for tracking or cancelling it.
    /// The download will be processed according to the manager's concurrency settings.
    async fn queue_download(&self, request: DownloadRequest) -> Result<DownloadId, DownloadError>;

    /// Queue a download with smart quantization selection.
    ///
    /// This is the recommended method for GUI adapters when the quantization
    /// may be optional. It:
    /// 1. Selects the best quantization if none specified
    /// 2. Validates the requested quantization exists
    /// 3. Queues the download and starts processing
    ///
    /// # Quantization Selection Rules
    ///
    /// - If a quantization is provided, validates it exists in the repository
    /// - If none provided and 1 option exists, auto-picks it (pre-quantized model)
    /// - If none provided and multiple exist, uses default preference order
    /// - Returns error if requested quant not found or no suitable default
    ///
    /// # Arguments
    ///
    /// * `repo_id` - `HuggingFace` repository ID (e.g., "unsloth/Llama-3-GGUF")
    /// * `quantization` - Optional quantization name (e.g., "`Q4_K_M`", "`Q8_0`")
    ///
    /// # Returns
    ///
    /// The download's ID: the row to watch in the queue snapshot, and the
    /// entry to read once it has ended. A request for a download already
    /// waiting or running answers that download's ID and queues nothing.
    async fn queue_smart(
        self: Arc<Self>,
        repo_id: String,
        quantization: Option<String>,
    ) -> Result<DownloadId, DownloadError>;

    /// Get a snapshot of the current queue state.
    ///
    /// Returns all queued, active, and recently completed/failed downloads.
    /// This is used by UIs to display download status.
    async fn get_queue_snapshot(&self) -> Result<QueueSnapshot, DownloadError>;

    /// Cancel a download: every file of it.
    ///
    /// A download waiting, or between two of its files, ends at once with a
    /// cancelled outcome. One with a file being fetched has that transfer
    /// told to stop, and ends cancelled when it has.
    ///
    /// A cancel can come too late and still be answered `Ok`: once the
    /// download's last file is on disk and its model is being registered,
    /// or once a file of it has failed, the download ends completed or
    /// failed as it was going to.
    ///
    /// Returns an error when the download is neither waiting nor running;
    /// the outcome of one that has already ended is left as it is.
    async fn cancel_download(&self, id: &DownloadId) -> Result<(), DownloadError>;

    /// Cancel all active and queued downloads, each as
    /// [`Self::cancel_download`] does, so each leaves its own cancelled
    /// outcome.
    ///
    /// This is used during application shutdown or when the user
    /// wants to clear the queue.
    async fn cancel_all(&self) -> Result<(), DownloadError>;

    /// Get the number of active downloads.
    async fn active_count(&self) -> Result<u32, DownloadError>;

    // ─────────────────────────────────────────────────────────────────────────
    // Queue management operations
    // ─────────────────────────────────────────────────────────────────────────

    /// Take a download off the queue.
    ///
    /// A download waiting or running is cancelled, as
    /// [`Self::cancel_download`] does. One that has already ended has its
    /// outcome dropped from the snapshot's `finished`. Returns an error when
    /// it is neither.
    async fn remove_from_queue(&self, id: &DownloadId) -> Result<(), DownloadError>;

    /// Reorder a waiting download to a new position in the queue.
    ///
    /// Positions are 1-based and count downloads, as the snapshot numbers
    /// them: a running download holds position 1 and the first waiting one
    /// is at 2; with nothing running the first waiting one is at 1. A
    /// download moves with all of its files, and never ahead of the running
    /// one. Returns the actual position assigned (may differ if the
    /// requested position is out of bounds).
    async fn reorder_queue(&self, id: &DownloadId, new_position: u32)
    -> Result<u32, DownloadError>;

    /// Clear the record of how earlier downloads ended: every entry of the
    /// snapshot's `finished`, whatever its outcome.
    async fn clear_finished(&self) -> Result<(), DownloadError>;

    /// Update the maximum queue size.
    ///
    /// Downloads already in queue are not affected, but new downloads
    /// may be rejected if the queue is at capacity.
    async fn set_max_queue_size(&self, size: u32) -> Result<(), DownloadError>;
}
