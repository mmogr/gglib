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
    /// Returns the download ID which can be used to track or cancel the download.
    /// The download will be processed according to the manager's concurrency settings.
    async fn queue_download(&self, request: DownloadRequest) -> Result<DownloadId, DownloadError>;

    /// Queue a download and ensure the queue processor is running.
    ///
    /// This is the recommended method for GUI adapters. It combines queuing
    /// with worker lifecycle management, hiding the internal details of
    /// how downloads are processed.
    ///
    /// The `self: Arc<Self>` receiver allows implementations to clone the
    /// Arc and spawn worker tasks. This is object-safe and works with
    /// `Arc<dyn DownloadManagerPort>`.
    ///
    /// Returns the download ID on success.
    async fn queue_and_process(
        self: Arc<Self>,
        request: DownloadRequest,
    ) -> Result<DownloadId, DownloadError>;

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
    /// Returns (position, `shard_count`) on success.
    async fn queue_smart(
        self: Arc<Self>,
        repo_id: String,
        quantization: Option<String>,
    ) -> Result<(usize, usize), DownloadError>;

    /// Get a snapshot of the current queue state.
    ///
    /// Returns all queued, active, and recently completed/failed downloads.
    /// This is used by UIs to display download status.
    async fn get_queue_snapshot(&self) -> Result<QueueSnapshot, DownloadError>;

    /// Cancel a download.
    ///
    /// If the download is queued, it's removed from the queue.
    /// If the download is active, the underlying process is terminated.
    /// Returns an error if the download ID is not found.
    async fn cancel_download(&self, id: &DownloadId) -> Result<(), DownloadError>;

    /// Cancel all active and queued downloads.
    ///
    /// This is used during application shutdown or when the user
    /// wants to clear the queue.
    async fn cancel_all(&self) -> Result<(), DownloadError>;

    /// Check if a download with the given ID exists in the queue.
    async fn has_download(&self, id: &DownloadId) -> Result<bool, DownloadError>;

    /// Get the number of active downloads.
    async fn active_count(&self) -> Result<u32, DownloadError>;

    /// Get the number of pending (queued) downloads.
    async fn pending_count(&self) -> Result<u32, DownloadError>;

    // ─────────────────────────────────────────────────────────────────────────
    // Queue management operations
    // ─────────────────────────────────────────────────────────────────────────

    /// Remove a pending download from the queue.
    ///
    /// This is for items that haven't started yet. For active downloads,
    /// use `cancel_download` instead.
    async fn remove_from_queue(&self, id: &DownloadId) -> Result<(), DownloadError>;

    /// Reorder a download to a new position in the queue.
    ///
    /// The position is 1-based where 1 is next to run. Returns the actual
    /// position assigned (may differ if requested position is out of bounds).
    async fn reorder_queue(&self, id: &DownloadId, new_position: u32)
    -> Result<u32, DownloadError>;

    /// Cancel all downloads in a shard group.
    ///
    /// Used for canceling multi-file model downloads where shards are
    /// queued together. The `group_id` matches `QueuedDownload.group_id`.
    async fn cancel_group(&self, group_id: &str) -> Result<(), DownloadError>;

    /// Retry a failed download.
    ///
    /// Moves the download from the failures list back to the queue.
    /// Returns the position in queue where it was added.
    async fn retry(&self, id: &DownloadId) -> Result<u32, DownloadError>;

    /// Clear all failed downloads from the failures list.
    async fn clear_failed(&self) -> Result<(), DownloadError>;

    /// Update the maximum queue size.
    ///
    /// Downloads already in queue are not affected, but new downloads
    /// may be rejected if the queue is at capacity.
    async fn set_max_queue_size(&self, size: u32) -> Result<(), DownloadError>;

    /// Get the maximum queue size.
    async fn get_max_queue_size(&self) -> Result<u32, DownloadError>;
}
