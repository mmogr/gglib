//! Download service for HuggingFace model downloads.
//!
//! This service provides managed downloads with progress tracking,
//! cancellation support, and download queue management.
//!
//! Types are defined in the `download_models` module for better organization.

use super::download_models::{
    DownloadError, DownloadQueueItem, DownloadQueueStatus, DownloadStatus, QueuedDownload,
};
use anyhow::Result;
use std::collections::{HashMap, VecDeque};
use std::sync::Arc;
use std::time::Instant;
use tokio::sync::RwLock;
use tokio_util::sync::CancellationToken;

/// Service for managing HuggingFace model downloads.
///
/// Provides download management with:
/// - Progress tracking via callbacks
/// - Cancellation support
/// - Download queue with configurable max size
/// - Auto-advance to next download on completion/failure
pub struct DownloadService {
    active_downloads: Arc<RwLock<HashMap<String, CancellationToken>>>,
    pending_queue: Arc<RwLock<VecDeque<QueuedDownload>>>,
    failed_downloads: Arc<RwLock<Vec<QueuedDownload>>>,
    max_queue_size: Arc<RwLock<u32>>,
    /// Flag to track if queue processor is running
    processing: Arc<RwLock<bool>>,
}

impl DownloadService {
    /// Create a new DownloadService with default max queue size (10).
    pub fn new() -> Self {
        Self {
            active_downloads: Arc::new(RwLock::new(HashMap::new())),
            pending_queue: Arc::new(RwLock::new(VecDeque::new())),
            failed_downloads: Arc::new(RwLock::new(Vec::new())),
            max_queue_size: Arc::new(RwLock::new(10)),
            processing: Arc::new(RwLock::new(false)),
        }
    }

    /// Set the maximum queue size.
    pub async fn set_max_queue_size(&self, size: u32) {
        *self.max_queue_size.write().await = size;
    }

    /// Get the current max queue size.
    pub async fn get_max_queue_size(&self) -> u32 {
        *self.max_queue_size.read().await
    }

    /// Add a download to the queue or start immediately if nothing is running.
    ///
    /// # Arguments
    ///
    /// * `model_id` - HuggingFace model ID (e.g., "TheBloke/Llama-2-7B-GGUF")
    /// * `quantization` - Optional quantization type (e.g., "Q4_K_M")
    ///
    /// # Returns
    ///
    /// Returns the queue position (1 = will start immediately).
    ///
    /// # Errors
    ///
    /// Returns an error if:
    /// - The model is already downloading or queued
    /// - The queue is full
    pub async fn queue_download(
        &self,
        model_id: String,
        quantization: Option<String>,
    ) -> Result<usize> {
        // Check if already downloading
        if self.active_downloads.read().await.contains_key(&model_id) {
            return Err(DownloadError::AlreadyRunning {
                model_id: model_id.clone(),
            }
            .into());
        }

        // Check if already in queue
        {
            let queue = self.pending_queue.read().await;
            if queue.iter().any(|item| item.model_id == model_id) {
                return Err(DownloadError::AlreadyRunning {
                    model_id: model_id.clone(),
                }
                .into());
            }
        }

        // Check queue size (active + pending)
        let active_count = self.active_downloads.read().await.len();
        let pending_count = self.pending_queue.read().await.len();
        let max_size = *self.max_queue_size.read().await;

        if (active_count + pending_count) as u32 >= max_size {
            return Err(DownloadError::QueueFull { max_size }.into());
        }

        // Remove from failed list if retrying
        {
            let mut failed = self.failed_downloads.write().await;
            failed.retain(|item| item.model_id != model_id);
        }

        // Add to queue
        let queued_item = QueuedDownload::new(model_id.clone(), quantization);

        let position = {
            let mut queue = self.pending_queue.write().await;
            queue.push_back(queued_item);
            active_count + queue.len()
        };

        Ok(position)
    }

    /// Add a sharded model download to the queue, creating one entry per shard.
    ///
    /// Each shard is queued as a separate item with a shared `group_id` for
    /// group operations (cancel all, fail all, retry all).
    ///
    /// # Arguments
    ///
    /// * `model_id` - HuggingFace model ID (e.g., "unsloth/Llama-3.2-1B-Instruct-GGUF")
    /// * `quantization` - Quantization type (e.g., "Q4_K_M")
    /// * `shard_filenames` - Ordered list of shard filenames to download
    ///
    /// # Returns
    ///
    /// Returns the queue position of the first shard.
    ///
    /// # Errors
    ///
    /// Returns an error if:
    /// - Any shard is already downloading or queued
    /// - The queue doesn't have room for all shards
    pub async fn queue_sharded_download(
        &self,
        model_id: String,
        quantization: String,
        shard_filenames: Vec<String>,
    ) -> Result<usize> {
        let shard_count = shard_filenames.len();
        if shard_count == 0 {
            return Err(anyhow::anyhow!("No shard filenames provided").into());
        }

        // Check queue capacity for all shards
        let active_count = self.active_downloads.read().await.len();
        let pending_count = self.pending_queue.read().await.len();
        let max_size = *self.max_queue_size.read().await;

        if (active_count + pending_count + shard_count) as u32 > max_size {
            return Err(DownloadError::QueueFull { max_size }.into());
        }

        // Check if model is already in queue or downloading
        {
            let queue = self.pending_queue.read().await;
            if queue.iter().any(|item| item.model_id == model_id) {
                return Err(DownloadError::AlreadyRunning {
                    model_id: model_id.clone(),
                }
                .into());
            }
        }

        if self.active_downloads.read().await.contains_key(&model_id) {
            return Err(DownloadError::AlreadyRunning {
                model_id: model_id.clone(),
            }
            .into());
        }

        // Remove from failed list if retrying (by model_id since group_id changes each time)
        {
            let mut failed = self.failed_downloads.write().await;
            failed.retain(|item| item.model_id != model_id);
        }

        // Add all shards to queue using create_shard_batch
        let first_position = {
            let mut queue = self.pending_queue.write().await;
            let base_position = active_count + queue.len() + 1;

            let (_, shard_items) =
                QueuedDownload::create_shard_batch(&model_id, &quantization, &shard_filenames);

            for item in shard_items {
                queue.push_back(item);
            }

            base_position
        };

        Ok(first_position)
    }

    /// Remove all items belonging to a shard group from the pending queue.
    ///
    /// This is used when cancelling or failing a sharded download to remove
    /// all remaining shards from the queue.
    pub async fn remove_shard_group(&self, group_id: &str) -> usize {
        let mut queue = self.pending_queue.write().await;
        let initial_len = queue.len();
        queue.retain(|item| item.group_id.as_deref() != Some(group_id));
        initial_len - queue.len()
    }

    /// Cancel an active download and remove all related shards from queue.
    ///
    /// If the active download belongs to a shard group, this will also
    /// remove all pending shards in that group.
    pub async fn cancel_shard_group(&self, group_id: &str) -> Result<()> {
        // First, find and cancel any active download in this group
        let active_model_id = {
            let queue = self.pending_queue.read().await;
            // Check if any pending item in this group gives us information
            // about what might be actively downloading
            queue
                .iter()
                .find(|item| item.group_id.as_deref() == Some(group_id))
                .map(|item| item.model_id.clone())
        };

        // Remove all pending shards in this group
        self.remove_shard_group(group_id).await;

        // Try to cancel active download if it belongs to this group
        if let Some(model_id) = active_model_id {
            // Ignore errors - the download might have already completed
            let _ = self.cancel(&model_id).await;
        }

        Ok(())
    }

    /// Get the current status of the download queue.
    pub async fn get_queue_status(&self) -> DownloadQueueStatus {
        let active = self.active_downloads.read().await;
        let pending = self.pending_queue.read().await;
        let failed = self.failed_downloads.read().await;
        let max_size = *self.max_queue_size.read().await;

        // Current download(s)
        let current = active.keys().next().map(|model_id| DownloadQueueItem {
            model_id: model_id.clone(),
            quantization: None, // We don't track quantization for active downloads
            status: DownloadStatus::Downloading,
            position: 1,
            error: None,
            group_id: None,
            shard_info: None,
        });

        // Pending items
        let pending_items: Vec<DownloadQueueItem> = pending
            .iter()
            .enumerate()
            .map(|(idx, item)| DownloadQueueItem {
                model_id: item.model_id.clone(),
                quantization: item.quantization.clone(),
                status: DownloadStatus::Queued,
                position: idx + 2, // 1 is for current download
                error: None,
                group_id: item.group_id.clone(),
                shard_info: item.shard_info.clone(),
            })
            .collect();

        // Failed items
        let failed_items: Vec<DownloadQueueItem> = failed
            .iter()
            .enumerate()
            .map(|(idx, item)| DownloadQueueItem {
                model_id: item.model_id.clone(),
                quantization: item.quantization.clone(),
                status: DownloadStatus::Failed,
                position: idx + 1,
                error: None,
                group_id: item.group_id.clone(),
                shard_info: item.shard_info.clone(),
            })
            .collect();

        DownloadQueueStatus {
            current,
            pending: pending_items,
            failed: failed_items,
            max_size,
        }
    }

    /// Remove an item from the pending queue.
    ///
    /// Cannot remove the currently downloading item (use cancel instead).
    pub async fn remove_from_queue(&self, model_id: &str) -> Result<()> {
        let mut queue = self.pending_queue.write().await;
        let initial_len = queue.len();
        queue.retain(|item| item.model_id != model_id);

        if queue.len() == initial_len {
            // Also check failed list
            let mut failed = self.failed_downloads.write().await;
            let failed_initial = failed.len();
            failed.retain(|item| item.model_id != model_id);

            if failed.len() == failed_initial {
                return Err(DownloadError::NotInQueue {
                    model_id: model_id.to_string(),
                }
                .into());
            }
        }

        Ok(())
    }

    /// Clear all failed downloads from the list.
    pub async fn clear_failed(&self) {
        self.failed_downloads.write().await.clear();
    }

    /// Get the next item from the queue (internal use).
    async fn pop_next(&self) -> Option<QueuedDownload> {
        self.pending_queue.write().await.pop_front()
    }

    /// Mark a download as failed (internal use).
    async fn mark_failed(&self, item: QueuedDownload) {
        self.failed_downloads.write().await.push(item);
    }

    /// Download a model from HuggingFace Hub.
    ///
    /// # Arguments
    ///
    /// * `model_id` - HuggingFace model ID (e.g., "TheBloke/Llama-2-7B-GGUF")
    /// * `quantization` - Optional quantization type (e.g., "Q4_K_M")
    /// * `progress_callback` - Optional callback for progress updates
    ///
    /// # Returns
    ///
    /// Returns success message on completion.
    ///
    /// # Errors
    ///
    /// Returns an error if:
    /// - A download for this model is already running
    /// - Download fails
    /// - Download is cancelled
    pub async fn download(
        &self,
        model_id: String,
        quantization: Option<String>,
        progress_callback: Option<&crate::commands::download::ProgressCallback>,
    ) -> Result<String> {
        let cancel_token = CancellationToken::new();

        // Check if download is already running
        {
            let mut downloads = self.active_downloads.write().await;
            if downloads.contains_key(&model_id) {
                return Err(DownloadError::AlreadyRunning {
                    model_id: model_id.clone(),
                }
                .into());
            }
            downloads.insert(model_id.clone(), cancel_token.clone());
        }

        // Execute download with cancellation support
        let download_future = crate::commands::download::execute(
            model_id.clone(),
            quantization,
            false, // list_quants
            true,  // add_to_db
            None,  // token
            false, // force
            progress_callback,
        );
        tokio::pin!(download_future);

        let result = tokio::select! {
            res = &mut download_future => {
                res.map(|_| "Model downloaded successfully".to_string())
            }
            _ = cancel_token.cancelled() => {
                Err(DownloadError::Cancelled { model_id: model_id.clone() }.into())
            }
        };

        // Clean up tracking
        self.active_downloads.write().await.remove(&model_id);

        result
    }

    /// Process the download queue, downloading items one at a time.
    ///
    /// This method should be called after adding items to the queue.
    /// It will continue processing until the queue is empty.
    ///
    /// # Arguments
    ///
    /// * `progress_callback` - Callback for progress updates
    pub async fn process_queue<F>(&self, progress_callback: F)
    where
        F: Fn(crate::commands::download::DownloadProgressEvent) + Send + Sync + Clone + 'static,
    {
        // Check if already processing
        {
            let mut processing = self.processing.write().await;
            if *processing {
                return;
            }
            *processing = true;
        }

        loop {
            // Get next item from queue
            let next_item = self.pop_next().await;

            let Some(item) = next_item else {
                break;
            };

            let model_id = item.model_id.clone();
            let quantization = item.quantization.clone();

            // Emit "started" event
            let queue_status = self.get_queue_status().await;
            let queue_len = queue_status.pending.len() + 1;
            let start_event = crate::commands::download::DownloadProgressEvent::starting(&model_id)
                .with_queue_info(1, queue_len);
            progress_callback(start_event);

            // Create a wrapper callback that converts (u64, u64) to DownloadProgressEvent
            let callback_clone = progress_callback.clone();
            let model_id_for_callback = model_id.clone();
            let queue_len_for_callback = queue_len;
            let download_start_time = Instant::now();
            let progress_cb: crate::commands::download::ProgressCallback =
                Box::new(move |downloaded: u64, total: u64| {
                    let event = crate::commands::download::DownloadProgressEvent::progress(
                        &model_id_for_callback,
                        downloaded,
                        total,
                        download_start_time,
                    )
                    .with_queue_info(1, queue_len_for_callback);
                    callback_clone(event);
                });

            // Execute download
            let result = self
                .download(model_id.clone(), quantization.clone(), Some(&progress_cb))
                .await;

            match result {
                Ok(_) => {
                    // Emit completed event
                    let complete_event =
                        crate::commands::download::DownloadProgressEvent::completed(
                            &model_id,
                            Some("Download completed successfully"),
                        );
                    progress_callback(complete_event);
                }
                Err(e) => {
                    // Check if cancelled
                    let error_msg = e.to_string();
                    if error_msg.contains("cancelled") {
                        let skip_event = crate::commands::download::DownloadProgressEvent::skipped(
                            &model_id,
                            "Cancelled by user",
                        );
                        progress_callback(skip_event);
                    } else {
                        // Mark as failed - clone the item to preserve shard info
                        let mut failed_item = item.clone();
                        failed_item.queued_at = None;
                        self.mark_failed(failed_item).await;

                        let error_event = crate::commands::download::DownloadProgressEvent::errored(
                            &model_id, &error_msg,
                        );
                        progress_callback(error_event);

                        // Emit skipped event to indicate we're moving to next
                        let skip_event = crate::commands::download::DownloadProgressEvent::skipped(
                            &model_id,
                            &format!("Failed: {}", error_msg),
                        );
                        progress_callback(skip_event);
                    }
                }
            }

            // Emit updated queue status for remaining items
            let remaining_status = self.get_queue_status().await;
            for pending_item in &remaining_status.pending {
                let queued_event = crate::commands::download::DownloadProgressEvent::queued(
                    &pending_item.model_id,
                    pending_item.position,
                    remaining_status.pending.len()
                        + if remaining_status.current.is_some() {
                            1
                        } else {
                            0
                        },
                );
                progress_callback(queued_event);
            }
        }

        // Done processing
        *self.processing.write().await = false;
    }

    /// Cancel an in-flight download.
    ///
    /// # Arguments
    ///
    /// * `model_id` - The model ID of the download to cancel
    ///
    /// # Errors
    ///
    /// Returns an error if no download is running for this model.
    pub async fn cancel(&self, model_id: &str) -> Result<()> {
        let token = {
            let mut downloads = self.active_downloads.write().await;
            downloads.remove(model_id)
        };

        if let Some(token) = token {
            token.cancel();
            Ok(())
        } else {
            Err(DownloadError::NotFound {
                model_id: model_id.to_string(),
            }
            .into())
        }
    }

    /// Check if a download is currently running for a model.
    pub async fn is_downloading(&self, model_id: &str) -> bool {
        self.active_downloads.read().await.contains_key(model_id)
    }

    /// Check if a model is in the queue (either downloading or pending).
    pub async fn is_in_queue(&self, model_id: &str) -> bool {
        if self.active_downloads.read().await.contains_key(model_id) {
            return true;
        }
        self.pending_queue
            .read()
            .await
            .iter()
            .any(|item| item.model_id == model_id)
    }

    /// Get list of currently active downloads.
    pub async fn active_downloads(&self) -> Vec<String> {
        self.active_downloads.read().await.keys().cloned().collect()
    }

    /// Search HuggingFace Hub for GGUF models.
    ///
    /// This is a convenience wrapper around the search functionality.
    pub async fn search(
        &self,
        query: String,
        limit: u32,
        sort: String,
        gguf_only: bool,
    ) -> Result<()> {
        crate::commands::download::handle_search(query, limit, sort, gguf_only).await
    }
}

impl Default for DownloadService {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_download_service_creation() {
        let service = DownloadService::new();
        assert!(service.active_downloads().await.is_empty());
    }

    #[tokio::test]
    async fn test_is_downloading_false() {
        let service = DownloadService::new();
        assert!(!service.is_downloading("some-model").await);
    }

    #[tokio::test]
    async fn test_cancel_nonexistent() {
        let service = DownloadService::new();
        let result = service.cancel("nonexistent-model").await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_queue_download() {
        let service = DownloadService::new();
        let result = service
            .queue_download("test/model".to_string(), Some("Q4_K_M".to_string()))
            .await;
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), 1);

        // Second item should be position 2
        let result2 = service
            .queue_download("test/model2".to_string(), None)
            .await;
        assert!(result2.is_ok());
        assert_eq!(result2.unwrap(), 2);
    }

    #[tokio::test]
    async fn test_queue_duplicate_rejected() {
        let service = DownloadService::new();
        service
            .queue_download("test/model".to_string(), None)
            .await
            .unwrap();

        let result = service.queue_download("test/model".to_string(), None).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_queue_full() {
        let service = DownloadService::new();
        service.set_max_queue_size(2).await;

        service
            .queue_download("model1".to_string(), None)
            .await
            .unwrap();
        service
            .queue_download("model2".to_string(), None)
            .await
            .unwrap();

        let result = service.queue_download("model3".to_string(), None).await;
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("full"));
    }

    #[tokio::test]
    async fn test_remove_from_queue() {
        let service = DownloadService::new();
        service
            .queue_download("model1".to_string(), None)
            .await
            .unwrap();
        service
            .queue_download("model2".to_string(), None)
            .await
            .unwrap();

        let result = service.remove_from_queue("model2").await;
        assert!(result.is_ok());

        let status = service.get_queue_status().await;
        assert_eq!(status.pending.len(), 1);
    }

    #[tokio::test]
    async fn test_get_queue_status() {
        let service = DownloadService::new();
        service
            .queue_download("model1".to_string(), Some("Q4_K_M".to_string()))
            .await
            .unwrap();
        service
            .queue_download("model2".to_string(), None)
            .await
            .unwrap();

        let status = service.get_queue_status().await;
        assert!(status.current.is_none()); // Nothing actively downloading
        assert_eq!(status.pending.len(), 2);
        assert_eq!(status.pending[0].model_id, "model1");
        assert_eq!(status.pending[0].position, 2); // Position starts at 2 (1 is for current)
    }
}
