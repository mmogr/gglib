//! Download state repository port definition.
//!
//! This port defines the interface for persisting download queue state.
//! Implementations handle durable storage of queued downloads so they
//! survive application restarts.
//!
//! # Design
//!
//! - Persists queue state and terminal results only
//! - Fine-grained progress stays in-memory (high churn, not worth persisting)
//! - Intent-based methods, not generic CRUD

use async_trait::async_trait;

use super::RepositoryError;
use crate::download::{DownloadId, DownloadStatus, QueuedDownload};

/// Port for persisting download queue state.
///
/// This trait is implemented by `gglib-db` and injected into the download
/// manager to provide durable queue storage.
///
/// # Persistence Scope
///
/// **Persisted:**
/// - Queued downloads (survive restarts)
/// - Terminal results (completed/failed with enough info for history)
/// - Retry counts (if retries are supported)
///
/// **In-memory only:**
/// - Fine-grained progress (bytes, speed)
/// - Active progress snapshots (broadcast via events)
///
/// # Usage
///
/// ```ignore
/// let repo: Arc<dyn DownloadStateRepositoryPort> = /* ... */;
/// repo.enqueue(&queued_download).await?;
/// let queue = repo.load_queue().await?;
/// ```
#[async_trait]
pub trait DownloadStateRepositoryPort: Send + Sync {
    /// Add a download to the persistent queue.
    ///
    /// The download should be in `Queued` status when enqueued.
    async fn enqueue(&self, download: &QueuedDownload) -> Result<(), RepositoryError>;

    /// Update the status of an existing download.
    ///
    /// This is used to transition downloads through their lifecycle:
    /// Queued -> Downloading -> Completed/Failed/Cancelled
    async fn update_status(
        &self,
        id: &DownloadId,
        status: DownloadStatus,
    ) -> Result<(), RepositoryError>;

    /// Load all queued downloads from persistent storage.
    ///
    /// Returns downloads that were queued but not yet completed.
    /// Used on application startup to restore the queue.
    async fn load_queue(&self) -> Result<Vec<QueuedDownload>, RepositoryError>;

    /// Mark a download as failed with an error message.
    ///
    /// This records the failure for history/retry purposes.
    async fn mark_failed(
        &self,
        id: &DownloadId,
        error_message: &str,
    ) -> Result<(), RepositoryError>;

    /// Remove a download from the persistent queue.
    ///
    /// Used when a download is cancelled or cleaned up.
    async fn remove(&self, id: &DownloadId) -> Result<(), RepositoryError>;

    /// Prune completed/failed downloads older than the given age.
    ///
    /// This is optional cleanup to prevent unbounded growth.
    async fn prune_completed(&self, older_than_days: u32) -> Result<u32, RepositoryError>;
}
