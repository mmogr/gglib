//! Download manager implementation.
//!
//! This module provides the concrete implementation of `DownloadManagerPort`.

use std::sync::Arc;

use async_trait::async_trait;
use tokio::sync::RwLock;

use gglib_core::download::{DownloadError, DownloadId, QueueSnapshot};
use gglib_core::ports::{
    DownloadManagerConfig, DownloadManagerPort, DownloadRequest, DownloadStateRepositoryPort,
    HfClientPort, ModelRegistrarPort, QuantizationResolver,
};

use crate::queue::DownloadQueue;
use crate::resolver::HfQuantizationResolver;

/// Dependencies for creating a download manager.
///
/// This struct bundles all the ports and configuration needed
/// to construct a `DownloadManagerImpl`.
pub struct DownloadManagerDeps<R, D, H>
where
    R: ModelRegistrarPort + 'static,
    D: DownloadStateRepositoryPort + 'static,
    H: HfClientPort + 'static,
{
    /// Port for registering completed downloads as models.
    pub model_registrar: Arc<R>,
    /// Port for persisting download queue state.
    pub download_repo: Arc<D>,
    /// Port for HuggingFace API access.
    pub hf_client: Arc<H>,
    /// Configuration for the download manager.
    pub config: DownloadManagerConfig,
}

/// Build a download manager from its dependencies.
///
/// Returns an implementation of `DownloadManagerPort` that can be
/// stored as `Arc<dyn DownloadManagerPort>` in adapters.
///
/// # Example
///
/// ```ignore
/// let deps = DownloadManagerDeps {
///     model_registrar: Arc::new(registrar),
///     download_repo: Arc::new(repo),
///     hf_client: Arc::new(client),
///     config: DownloadManagerConfig::new(models_dir),
/// };
///
/// let manager: Arc<dyn DownloadManagerPort> = Arc::new(build_download_manager(deps));
/// ```
pub fn build_download_manager<R, D, H>(deps: DownloadManagerDeps<R, D, H>) -> DownloadManagerImpl
where
    R: ModelRegistrarPort + 'static,
    D: DownloadStateRepositoryPort + 'static,
    H: HfClientPort + 'static,
{
    DownloadManagerImpl::new(
        deps.model_registrar,
        deps.download_repo,
        deps.hf_client,
        deps.config,
    )
}

/// Concrete implementation of the download manager.
///
/// This struct is public but adapters should typically use
/// `Arc<dyn DownloadManagerPort>` instead of depending on this type directly.
pub struct DownloadManagerImpl {
    /// Model registrar for completed downloads.
    model_registrar: Arc<dyn ModelRegistrarPort>,
    /// Repository for persisting queue state.
    download_repo: Arc<dyn DownloadStateRepositoryPort>,
    /// HuggingFace client for API access.
    hf_client: Arc<dyn HfClientPort>,
    /// File resolver.
    resolver: HfQuantizationResolver,
    /// Queue state (protected by RwLock for async access).
    queue: RwLock<DownloadQueue>,
    /// Configuration.
    config: DownloadManagerConfig,
    /// Currently active download (if any).
    active: RwLock<Option<ActiveDownload>>,
}

/// State for an active download.
struct ActiveDownload {
    /// The download ID.
    id: DownloadId,
    // TODO: Add cancellation token, progress tracking, etc.
}

impl DownloadManagerImpl {
    /// Create a new download manager.
    fn new<R, D, H>(
        model_registrar: Arc<R>,
        download_repo: Arc<D>,
        hf_client: Arc<H>,
        config: DownloadManagerConfig,
    ) -> Self
    where
        R: ModelRegistrarPort + 'static,
        D: DownloadStateRepositoryPort + 'static,
        H: HfClientPort + 'static,
    {
        let resolver = HfQuantizationResolver::new(hf_client.clone() as Arc<dyn HfClientPort>);

        Self {
            model_registrar,
            download_repo,
            hf_client: hf_client as Arc<dyn HfClientPort>,
            resolver,
            queue: RwLock::new(DownloadQueue::new(config.max_queue_size)),
            config,
            active: RwLock::new(None),
        }
    }

    /// Check if there's an active download.
    async fn has_active(&self) -> bool {
        self.active.read().await.is_some()
    }
}

#[async_trait]
impl DownloadManagerPort for DownloadManagerImpl {
    async fn queue_download(&self, request: DownloadRequest) -> Result<DownloadId, DownloadError> {
        let id = DownloadId::new(&request.repo_id, Some(request.quantization.to_string()));

        // Resolve files for this download
        let resolution = self
            .resolver
            .resolve(&request.repo_id, request.quantization)
            .await?;

        let has_active = self.has_active().await;
        let mut queue = self.queue.write().await;

        // Queue the download (sharded or single)
        let position = if resolution.is_sharded {
            let shard_files: Vec<_> = resolution
                .files
                .iter()
                .map(|f| (f.path.clone(), f.size))
                .collect();
            queue.queue_sharded(id.clone(), shard_files, has_active)?
        } else {
            queue.queue(id.clone(), has_active)?
        };

        tracing::info!(
            id = %id,
            position = position,
            sharded = resolution.is_sharded,
            files = resolution.files.len(),
            "Download queued"
        );

        // TODO: Persist to download_repo
        // TODO: Start processing if no active download

        Ok(id)
    }

    async fn get_queue_snapshot(&self) -> Result<QueueSnapshot, DownloadError> {
        let queue = self.queue.read().await;
        let active = self.active.read().await;

        // Build current item DTO if there's an active download
        let current_dto = if let Some(ref active_download) = *active {
            // TODO: Get actual progress from the active download
            Some(gglib_core::download::QueuedDownload::new(
                active_download.id.to_string(),
                active_download.id.model_id(),
                active_download.id.to_string(),
                1,
                0,
            ))
        } else {
            None
        };

        Ok(queue.snapshot(current_dto))
    }

    async fn cancel_download(&self, id: &DownloadId) -> Result<(), DownloadError> {
        // Check if this is the active download
        {
            let active = self.active.read().await;
            if let Some(ref active_download) = *active {
                if &active_download.id == id {
                    // TODO: Cancel the active download via cancellation token
                    tracing::info!(id = %id, "Cancelling active download");
                    drop(active);
                    *self.active.write().await = None;
                    return Ok(());
                }
            }
        }

        // Otherwise try to remove from queue
        let mut queue = self.queue.write().await;
        queue.remove(id)?;

        tracing::info!(id = %id, "Removed download from queue");
        Ok(())
    }

    async fn cancel_all(&self) -> Result<(), DownloadError> {
        // Cancel active download
        {
            let mut active = self.active.write().await;
            if active.is_some() {
                // TODO: Cancel via cancellation token
                *active = None;
            }
        }

        // Clear queue
        let mut queue = self.queue.write().await;
        queue.clear();

        tracing::info!("Cancelled all downloads");
        Ok(())
    }

    async fn has_download(&self, id: &DownloadId) -> Result<bool, DownloadError> {
        // Check active
        if let Some(ref active) = *self.active.read().await {
            if &active.id == id {
                return Ok(true);
            }
        }

        // Check queue
        let queue = self.queue.read().await;
        Ok(queue.is_queued(id) || queue.is_failed(id))
    }

    async fn active_count(&self) -> Result<u32, DownloadError> {
        let active = self.active.read().await;
        Ok(if active.is_some() { 1 } else { 0 })
    }

    async fn pending_count(&self) -> Result<u32, DownloadError> {
        let queue = self.queue.read().await;
        Ok(queue.pending_len() as u32)
    }
}

#[cfg(test)]
mod tests {
    // TODO: Add tests with mock ports
}
