//! Download service for HuggingFace model downloads.
//!
//! This service provides managed downloads with progress tracking,
//! cancellation support, and download queue management.
//!
//! Types are defined in the `download_models` module for better organization.

use super::download_models::{
    DownloadError, DownloadQueueItem, DownloadQueueStatus, DownloadStatus, QueuedDownload,
};
use super::huggingface_service::HuggingFaceService;
use anyhow::Result;
use std::collections::{HashMap, VecDeque};
use std::sync::Arc;
use tokio::sync::RwLock;
use tokio_util::sync::CancellationToken;

/// Type alias for storing child process PIDs for synchronous termination on shutdown.
/// Uses std::sync::RwLock (not tokio) for synchronous access in shutdown handler.
pub type PidStorage = Arc<std::sync::RwLock<HashMap<String, u32>>>;

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
    /// Stores PIDs of active child processes for synchronous termination on app shutdown.
    /// Uses std::sync::RwLock for synchronous access from the shutdown handler.
    child_pids: PidStorage,
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
            child_pids: Arc::new(std::sync::RwLock::new(HashMap::new())),
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
            return Err(anyhow::anyhow!("No shard filenames provided"));
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

    /// Add a sharded model download to the queue with file size information.
    ///
    /// Similar to `queue_sharded_download` but includes file sizes for aggregate
    /// progress tracking in the UI.
    pub async fn queue_sharded_download_with_sizes(
        &self,
        model_id: String,
        quantization: String,
        shard_files: Vec<(String, u64)>,
    ) -> Result<usize> {
        let shard_count = shard_files.len();
        if shard_count == 0 {
            return Err(anyhow::anyhow!("No shard files provided"));
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

        // Remove from failed list if retrying
        {
            let mut failed = self.failed_downloads.write().await;
            failed.retain(|item| item.model_id != model_id);
        }

        // Add all shards to queue with size information
        let first_position = {
            let mut queue = self.pending_queue.write().await;
            let base_position = active_count + queue.len() + 1;

            let (_, shard_items, _total_size) = QueuedDownload::create_shard_batch_with_sizes(
                &model_id,
                &quantization,
                &shard_files,
            );

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

    /// Reorder a queued item (or shard group) to a new position in the queue.
    ///
    /// For sharded models, all shards with the same `group_id` are moved together
    /// as a unit, preserving their relative order within the group.
    ///
    /// # Arguments
    ///
    /// * `model_id` - The model ID to move (used to identify the group)
    /// * `new_position` - The target position (0-based index in the pending queue)
    ///
    /// # Returns
    ///
    /// The actual position where the item(s) were placed.
    ///
    /// # Errors
    ///
    /// Returns an error if the model is not found in the pending queue.
    pub async fn reorder_queue(&self, model_id: &str, new_position: usize) -> Result<usize> {
        let mut queue = self.pending_queue.write().await;

        // Find the item to get its group_id (if any)
        let group_id = queue
            .iter()
            .find(|item| item.model_id == model_id)
            .and_then(|item| item.group_id.clone());

        // Collect indices and items to move (either single item or entire shard group)
        let items_to_move: Vec<QueuedDownload> = if let Some(ref gid) = group_id {
            // Move all items in the shard group together
            queue
                .iter()
                .filter(|item| item.group_id.as_deref() == Some(gid))
                .cloned()
                .collect()
        } else {
            // Single item (non-sharded)
            queue
                .iter()
                .filter(|item| item.model_id == model_id)
                .cloned()
                .collect()
        };

        if items_to_move.is_empty() {
            return Err(DownloadError::NotInQueue {
                model_id: model_id.to_string(),
            }
            .into());
        }

        // Remove the items from queue
        if let Some(ref gid) = group_id {
            queue.retain(|item| item.group_id.as_deref() != Some(gid));
        } else {
            queue.retain(|item| item.model_id != model_id);
        }

        // Calculate actual insertion position (clamped to queue bounds)
        let insert_pos = new_position.min(queue.len());

        // Insert items at new position (in order, to preserve shard sequence)
        for (offset, item) in items_to_move.into_iter().enumerate() {
            let pos = insert_pos + offset;
            // VecDeque insert: convert to vec temporarily for easier insertion
            if pos >= queue.len() {
                queue.push_back(item);
            } else {
                // Insert by rotating: push_back then rotate
                queue.push_back(item);
                // Rotate the last element to the target position
                let len = queue.len();
                for i in (pos + 1..len).rev() {
                    queue.swap(i, i - 1);
                }
            }
        }

        Ok(insert_pos)
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
            Some(cancel_token.clone()),    // Pass token for cancellation
            Some(self.child_pids.clone()), // PID storage for shutdown termination
            Some(model_id.clone()),        // PID key
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

    /// Download a single shard file from HuggingFace Hub.
    ///
    /// This is used internally by the queue processor for sharded models.
    /// It downloads a specific file rather than auto-detecting files.
    ///
    /// # Arguments
    ///
    /// * `model_id` - HuggingFace model ID
    /// * `filename` - Specific filename to download (e.g., "Q4_K_M/model-00001-of-00003.gguf")
    /// * `quantization` - Quantization type for metadata
    /// * `is_last_shard` - Whether this is the last shard (triggers database add)
    /// * `progress_callback` - Optional callback for progress updates
    async fn download_shard(
        &self,
        model_id: String,
        filename: String,
        quantization: String,
        is_last_shard: bool,
        progress_callback: Option<&crate::commands::download::ProgressCallback>,
    ) -> Result<String> {
        use crate::commands::download::{
            DownloadContext, SessionOptions, download_specific_file, get_first_shard_filename,
            get_models_directory, sanitize_model_name,
        };

        let cancel_token = CancellationToken::new();

        // Use a unique key for tracking (model_id + filename for shards)
        let tracking_key = format!("{}:{}", model_id, filename);

        // Check if download is already running
        {
            let mut downloads = self.active_downloads.write().await;
            if downloads.contains_key(&tracking_key) {
                return Err(DownloadError::AlreadyRunning {
                    model_id: tracking_key.clone(),
                }
                .into());
            }
            downloads.insert(tracking_key.clone(), cancel_token.clone());
        }

        // Get models directory and commit SHA
        let models_dir = get_models_directory()?;

        // Get commit SHA from HuggingFace API using shared service
        let hf_service = HuggingFaceService::new();
        let commit_sha = hf_service.get_commit_sha(&model_id).await?;

        // For sharded models, compute the first shard path for database registration.
        // llama-server requires the first shard to be specified when loading split models.
        let first_shard_path = if is_last_shard {
            let first_shard_filename = get_first_shard_filename(&filename);
            let model_dir = models_dir.join(sanitize_model_name(&model_id));
            Some(model_dir.join(&first_shard_filename))
        } else {
            None
        };

        // Create download context with cancellation token and PID storage
        let context = DownloadContext {
            model_id: &model_id,
            quantization: Some(&quantization),
            models_dir: &models_dir,
            force: false,
            add_to_db: is_last_shard, // Only add to DB on last shard
            session: SessionOptions {
                auth_token: None,
                progress_callback,
                cancel_token: Some(cancel_token.clone()),
                pid_storage: Some(self.child_pids.clone()),
                pid_key: Some(tracking_key.clone()),
            },
            first_shard_path,
        };

        // Execute download with cancellation support
        let download_future = download_specific_file(&filename, &commit_sha, &context);
        tokio::pin!(download_future);

        let result = tokio::select! {
            res = &mut download_future => {
                res.map(|_| format!("Shard {} downloaded successfully", filename))
            }
            _ = cancel_token.cancelled() => {
                Err(DownloadError::Cancelled { model_id: tracking_key.clone() }.into())
            }
        };

        // Clean up tracking
        self.active_downloads.write().await.remove(&tracking_key);

        result
    }

    /// Process the download queue, downloading items one at a time.
    ///
    /// This method should be called after adding items to the queue.
    /// It will continue processing until the queue is empty.
    ///
    /// For sharded models, each shard is downloaded individually. If any shard
    /// fails, all remaining shards in the group are removed from the queue.
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
            let is_shard = item.shard_info.is_some();

            // Build display name for events (include shard info if applicable)
            let display_name = if let Some(ref shard) = item.shard_info {
                format!(
                    "{} (shard {}/{})",
                    model_id,
                    shard.shard_index + 1,
                    shard.total_shards
                )
            } else {
                model_id.clone()
            };

            // Emit "started" event
            let queue_status = self.get_queue_status().await;
            let queue_len = queue_status.pending.len() + 1;
            let start_event =
                crate::commands::download::DownloadProgressEvent::starting(&display_name)
                    .with_queue_info(1, queue_len);
            progress_callback(start_event);

            // Calculate aggregate information for sharded downloads
            let (shard_index, total_shards, shard_filename, _shard_size, completed_shards_size) =
                if let Some(ref shard) = item.shard_info {
                    // Calculate size of already completed shards by looking at queue status
                    // (all shards before this one in the same group are completed)
                    let completed_size = self.get_completed_shards_size(&item).await;
                    (
                        shard.shard_index,
                        shard.total_shards,
                        shard.filename.clone(),
                        shard.file_size.unwrap_or(0),
                        completed_size,
                    )
                } else {
                    (0, 1, String::new(), 0, 0)
                };

            // Get total size across all shards for this model
            let aggregate_total = if is_shard {
                self.get_shard_group_total_size(&item).await
            } else {
                0
            };

            // Create a wrapper callback that converts (u64, u64) to DownloadProgressEvent
            let callback_clone = progress_callback.clone();
            let display_name_for_callback = model_id.clone(); // Use model_id, not display_name with shard info
            let queue_len_for_callback = queue_len;

            // Capture shard info for the callback
            let shard_index_for_cb = shard_index;
            let total_shards_for_cb = total_shards;
            let shard_filename_for_cb = shard_filename.clone();
            let completed_shards_size_for_cb = completed_shards_size;
            let aggregate_total_for_cb = aggregate_total;
            let is_shard_for_cb = is_shard;

            // Use throttle with EWA speed calculation
            let throttle = crate::commands::download::ProgressThrottle::responsive_ui();

            let progress_cb: crate::commands::download::ProgressCallback =
                Box::new(move |downloaded: u64, total: u64| {
                    // Check throttle and get EWA speed
                    let Some(speed) = throttle.should_emit_with_speed(downloaded, total) else {
                        return;
                    };

                    let event = if is_shard_for_cb && total_shards_for_cb > 1 {
                        // For sharded downloads, include aggregate progress
                        let aggregate_downloaded = completed_shards_size_for_cb + downloaded;
                        let aggregate_total_effective = if aggregate_total_for_cb > 0 {
                            aggregate_total_for_cb
                        } else {
                            // Fall back to current shard total * shard count as estimate
                            total * total_shards_for_cb as u64
                        };

                        crate::commands::download::DownloadProgressEvent::progress_with_shard(
                            &display_name_for_callback,
                            downloaded,
                            total,
                            shard_index_for_cb,
                            total_shards_for_cb,
                            &shard_filename_for_cb,
                            aggregate_downloaded,
                            aggregate_total_effective,
                            speed,
                        )
                        .with_queue_info(1, queue_len_for_callback)
                    } else {
                        // Non-sharded download
                        crate::commands::download::DownloadProgressEvent::progress(
                            &display_name_for_callback,
                            downloaded,
                            total,
                            speed,
                        )
                        .with_queue_info(1, queue_len_for_callback)
                    };
                    callback_clone(event);
                });

            // Execute download - use shard-specific method if this is a shard
            let result = if is_shard {
                let shard_info = item.shard_info.as_ref().unwrap();
                let is_last_shard = self.is_last_shard_in_group(&item).await;
                self.download_shard(
                    model_id.clone(),
                    shard_info.filename.clone(),
                    quantization.clone().unwrap_or_default(),
                    is_last_shard,
                    Some(&progress_cb),
                )
                .await
            } else {
                self.download(model_id.clone(), quantization.clone(), Some(&progress_cb))
                    .await
            };

            match result {
                Ok(_) => {
                    // Emit completed event
                    let complete_event =
                        crate::commands::download::DownloadProgressEvent::completed(
                            &display_name,
                            Some("Download completed successfully"),
                        );
                    progress_callback(complete_event);
                }
                Err(e) => {
                    // Check if cancelled
                    let error_msg = e.to_string();
                    if error_msg.contains("cancelled") {
                        // If this was a shard, remove all remaining shards in the group
                        if let Some(ref group_id) = item.group_id {
                            self.remove_shard_group(group_id).await;
                        }

                        let skip_event = crate::commands::download::DownloadProgressEvent::skipped(
                            &display_name,
                            "Cancelled by user",
                        );
                        progress_callback(skip_event);
                    } else {
                        // Mark as failed - clone the item to preserve shard info
                        let mut failed_item = item.clone();
                        failed_item.queued_at = None;
                        self.mark_failed(failed_item).await;

                        // If this was a shard, mark entire group as failed
                        if let Some(ref group_id) = item.group_id {
                            // Move remaining shards to failed list
                            let removed = self.fail_shard_group(group_id).await;
                            if removed > 0 {
                                let group_msg =
                                    format!("Shard failed, {} remaining shards cancelled", removed);
                                let group_event =
                                    crate::commands::download::DownloadProgressEvent::errored(
                                        &model_id, &group_msg,
                                    );
                                progress_callback(group_event);
                            }
                        }

                        let error_event = crate::commands::download::DownloadProgressEvent::errored(
                            &display_name,
                            &error_msg,
                        );
                        progress_callback(error_event);

                        // Emit skipped event to indicate we're moving to next
                        let skip_event = crate::commands::download::DownloadProgressEvent::skipped(
                            &display_name,
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

    /// Check if the given item is the last shard in its group.
    async fn is_last_shard_in_group(&self, item: &QueuedDownload) -> bool {
        let Some(ref group_id) = item.group_id else {
            return true; // Not a shard, treat as "last"
        };

        let queue = self.pending_queue.read().await;
        !queue.iter().any(|q| q.group_id.as_ref() == Some(group_id))
    }

    /// Get the total size of already completed shards in the same group.
    ///
    /// This is used to calculate aggregate progress for sharded downloads.
    /// Completed shards are those with a lower shard_index than the current item.
    async fn get_completed_shards_size(&self, item: &QueuedDownload) -> u64 {
        let Some(ref shard_info) = item.shard_info else {
            return 0;
        };
        let Some(ref _group_id) = item.group_id else {
            return 0;
        };

        // The current shard's index tells us how many shards have been completed
        // Sum up the sizes of shards 0 to shard_index-1
        // Since we process shards in order and they're no longer in the queue once completed,
        // we need to estimate based on the current shard's size and position

        // If we have file_size info for the current shard, estimate that all previous
        // shards are roughly the same size (common for sharded GGUF models)
        let current_size = shard_info.file_size.unwrap_or(0);
        let completed_count = shard_info.shard_index;

        current_size * completed_count as u64
    }

    /// Get the total size across all shards in a shard group.
    ///
    /// This calculates the aggregate total by summing file sizes from the current
    /// item's shard_info and remaining pending shards in the same group.
    async fn get_shard_group_total_size(&self, item: &QueuedDownload) -> u64 {
        let Some(ref shard_info) = item.shard_info else {
            return 0;
        };
        let Some(ref group_id) = item.group_id else {
            return 0;
        };

        // Start with the current item's size
        let mut total = shard_info.file_size.unwrap_or(0);

        // Add sizes from remaining pending shards in the same group
        let queue = self.pending_queue.read().await;
        for pending in queue.iter() {
            #[allow(clippy::collapsible_if)]
            if pending.group_id.as_ref() == Some(group_id) {
                if let Some(ref pending_shard) = pending.shard_info {
                    total += pending_shard.file_size.unwrap_or(0);
                }
            }
        }

        // If we have the current shard's size, estimate total for all shards
        // (including completed ones) based on average shard size
        if total > 0 && shard_info.total_shards > 0 {
            let current_size = shard_info.file_size.unwrap_or(0);
            let remaining_count = queue
                .iter()
                .filter(|p| p.group_id.as_ref() == Some(group_id))
                .count()
                + 1; // +1 for current

            // Estimate completed shards size
            let completed_count = shard_info.total_shards - remaining_count;
            total += current_size * completed_count as u64;
        }

        total
    }

    /// Move all remaining items in a shard group to the failed list.
    ///
    /// Returns the number of items moved.
    async fn fail_shard_group(&self, group_id: &str) -> usize {
        let mut queue = self.pending_queue.write().await;
        let mut failed = self.failed_downloads.write().await;

        let mut removed = Vec::new();
        queue.retain(|item| {
            if item.group_id.as_deref() == Some(group_id) {
                let mut failed_item = item.clone();
                failed_item.queued_at = None;
                removed.push(failed_item);
                false
            } else {
                true
            }
        });

        let count = removed.len();
        failed.extend(removed);
        count
    }

    /// Cancel an in-flight download.
    ///
    /// # Arguments
    ///
    /// * `model_id` - The model ID of the download to cancel. For sharded downloads,
    ///   this will cancel any active shard that belongs to this model.
    ///
    /// # Errors
    ///
    /// Returns an error if no download is running for this model.
    pub async fn cancel(&self, model_id: &str) -> Result<()> {
        let token = {
            let mut downloads = self.active_downloads.write().await;
            // Try exact match first (for non-sharded downloads)
            if let Some(token) = downloads.remove(model_id) {
                Some(token)
            } else {
                // For sharded downloads, the key is "model_id:filename"
                // Find any key that starts with "model_id:"
                let prefix = format!("{}:", model_id);
                let key_to_remove = downloads.keys().find(|k| k.starts_with(&prefix)).cloned();
                key_to_remove.and_then(|k| downloads.remove(&k))
            }
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

    /// Cancel all active downloads and wait for them to stop.
    ///
    /// This method cancels all in-flight downloads and waits up to the specified
    /// timeout for the Python subprocesses to terminate. Used for graceful app shutdown.
    ///
    /// # Arguments
    ///
    /// * `timeout` - Maximum time to wait for downloads to stop
    ///
    /// # Returns
    ///
    /// Returns the number of downloads that were cancelled.
    pub async fn cancel_all_and_wait(&self, timeout: std::time::Duration) -> usize {
        use tracing::info;

        // Get all active download keys and their tokens
        let tokens: Vec<(String, CancellationToken)> = {
            let mut downloads = self.active_downloads.write().await;
            downloads.drain().collect()
        };

        let count = tokens.len();
        if count == 0 {
            return 0;
        }

        info!(
            count = count,
            "Cancelling all active downloads for shutdown"
        );

        // Cancel all tokens - this signals the download tasks to stop
        for (key, token) in &tokens {
            info!(key = %key, "Cancelling download");
            token.cancel();
        }

        // Wait a bit for Python processes to receive the kill signal and terminate
        // The cancel triggers child.kill().await in the download task
        tokio::time::sleep(timeout).await;

        info!(count = count, "Download cancellation complete");
        count
    }

    /// Get a clone of the PID storage for passing to download functions.
    ///
    /// This allows the download functions to register their child process PIDs
    /// so they can be killed synchronously on app shutdown.
    pub fn child_pids(&self) -> PidStorage {
        self.child_pids.clone()
    }

    /// Synchronously kill all active child processes.
    ///
    /// This method is designed for app shutdown and does NOT require an async runtime.
    /// It sends SIGKILL (Unix) or TerminateProcess (Windows) to all tracked child processes.
    ///
    /// # Returns
    ///
    /// Returns the number of processes that were signaled to terminate.
    pub fn kill_all_processes_sync(&self) -> usize {
        use tracing::{debug, info};

        // Get all PIDs - use write lock to drain the map
        let pids: Vec<(String, u32)> = match self.child_pids.write() {
            Ok(mut guard) => {
                debug!(tracked_count = guard.len(), "Draining PID storage");
                guard.drain().collect()
            }
            Err(poisoned) => {
                info!("PID storage lock was poisoned, recovering");
                poisoned.into_inner().drain().collect()
            }
        };

        let count = pids.len();
        if count == 0 {
            debug!("No child processes tracked - nothing to kill");
            return 0;
        }

        info!(count = count, "Killing all child processes for shutdown");

        for (key, pid) in &pids {
            info!(key = %key, pid = pid, "Sending SIGKILL to process");
            kill_process_by_pid(*pid);
        }

        info!(count = count, "Process termination signals sent");
        count
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

    /// Detect shard files for a model/quantization from HuggingFace.
    ///
    /// Queries the HuggingFace API to find all GGUF files matching the specified
    /// quantization. Returns an ordered list of filenames if multiple shards are
    /// found, or a single-element list for non-sharded models.
    ///
    /// # Arguments
    ///
    /// * `model_id` - HuggingFace model ID (e.g., "unsloth/Llama-3.2-1B-Instruct-GGUF")
    /// * `quantization` - Quantization type (e.g., "Q4_K_M")
    ///
    /// # Returns
    ///
    /// Returns `Ok(Vec<String>)` containing ordered shard filenames, or an error
    /// if the model/quantization is not found.
    ///
    /// # Examples
    ///
    /// ```rust,ignore
    /// let service = DownloadService::new();
    /// // Non-sharded model returns single file
    /// let files = service.detect_shard_files("TheBloke/Llama-2-7B-GGUF", "Q4_K_M").await?;
    /// assert_eq!(files.len(), 1);
    ///
    /// // Sharded model returns multiple files in order
    /// let files = service.detect_shard_files("big/model-GGUF", "Q4_K_M").await?;
    /// assert!(files.len() > 1);
    /// assert!(files[0].contains("00001"));
    /// ```
    pub async fn detect_shard_files(
        &self,
        model_id: &str,
        quantization: &str,
    ) -> Result<Vec<String>> {
        let files_with_sizes = self
            .detect_shard_files_with_sizes(model_id, quantization)
            .await?;
        Ok(files_with_sizes.into_iter().map(|(name, _)| name).collect())
    }

    /// Detect shard files with their sizes for a model/quantization from HuggingFace.
    ///
    /// Similar to `detect_shard_files` but also returns file sizes for aggregate
    /// progress tracking in the UI.
    ///
    /// # Returns
    ///
    /// Returns `Ok(Vec<(filename, size_bytes)>)` containing ordered shard info.
    pub async fn detect_shard_files_with_sizes(
        &self,
        model_id: &str,
        quantization: &str,
    ) -> Result<Vec<(String, u64)>> {
        // Use HuggingFaceService to find GGUF files (DRY - no duplicate API calls)
        let hf_service = HuggingFaceService::new();
        let matching_files = hf_service
            .find_gguf_files_for_quantization(model_id, quantization)
            .await?;

        if matching_files.is_empty() {
            return Err(anyhow::anyhow!(
                "No GGUF files found for quantization '{}'",
                quantization
            ));
        }

        Ok(matching_files)
    }

    /// Smart queue method that auto-detects shards and queues appropriately.
    ///
    /// This is the preferred method for GUI downloads. It:
    /// 1. Queries HuggingFace to detect if the model is sharded
    /// 2. Creates individual queue items for each shard (with shared group_id)
    /// 3. Or creates a single queue item for non-sharded models
    ///
    /// # Arguments
    ///
    /// * `model_id` - HuggingFace model ID
    /// * `quantization` - Quantization type
    ///
    /// # Returns
    ///
    /// Returns `(queue_position, shard_count)` where shard_count is 1 for
    /// non-sharded models.
    pub async fn queue_download_auto(
        &self,
        model_id: String,
        quantization: String,
    ) -> Result<(usize, usize)> {
        // Detect shard files from HuggingFace
        // Detect shard files with sizes from HuggingFace
        let shard_files_with_sizes = self
            .detect_shard_files_with_sizes(&model_id, &quantization)
            .await?;
        let shard_count = shard_files_with_sizes.len();

        if shard_count == 1 {
            // Non-sharded model - use regular queue method
            let position = self.queue_download(model_id, Some(quantization)).await?;
            Ok((position, 1))
        } else {
            // Sharded model - queue each shard separately with sizes
            let position = self
                .queue_sharded_download_with_sizes(model_id, quantization, shard_files_with_sizes)
                .await?;
            Ok((position, shard_count))
        }
    }

    /// Retry a failed download by model_id.
    ///
    /// For sharded models, this re-queues all shards. The download_specific_file
    /// function will automatically skip shards that have already been downloaded.
    ///
    /// # Arguments
    ///
    /// * `model_id` - HuggingFace model ID to retry
    ///
    /// # Returns
    ///
    /// Returns `(queue_position, shard_count)` on success.
    ///
    /// # Errors
    ///
    /// Returns an error if the model is not in the failed list or if re-queuing fails.
    pub async fn retry_failed_download(&self, model_id: &str) -> Result<(usize, usize)> {
        // Find the failed item(s) for this model
        let failed_item = {
            let failed = self.failed_downloads.read().await;
            failed
                .iter()
                .find(|item| item.model_id == model_id)
                .cloned()
        };

        let Some(item) = failed_item else {
            return Err(anyhow::anyhow!(
                "No failed download found for model '{}'",
                model_id
            ));
        };

        // Use quantization from failed item if available
        let quantization = item.quantization.clone().unwrap_or_default();

        // Re-queue using auto-detect (handles both sharded and non-sharded)
        // This will:
        // 1. Remove from failed list (via queue_download/queue_sharded_download)
        // 2. Re-detect shard files
        // 3. Queue appropriately
        // 4. During download, already-completed shards will be skipped
        self.queue_download_auto(model_id.to_string(), quantization)
            .await
    }

    /// Get information about which shards are already downloaded for a model.
    ///
    /// This can be used to show users which shards will be skipped on retry.
    ///
    /// # Arguments
    ///
    /// * `model_id` - HuggingFace model ID
    /// * `quantization` - Quantization type
    ///
    /// # Returns
    ///
    /// Returns a tuple of `(completed_shards, total_shards, completed_filenames)`.
    pub async fn get_shard_download_status(
        &self,
        model_id: &str,
        quantization: &str,
    ) -> Result<(usize, usize, Vec<String>)> {
        use crate::commands::download::{get_models_directory, sanitize_model_name};

        // Detect all shard files from HuggingFace
        let shard_files = self.detect_shard_files(model_id, quantization).await?;
        let total_shards = shard_files.len();

        // Get models directory
        let models_dir = get_models_directory()?;
        let model_dir = models_dir.join(sanitize_model_name(model_id));

        // Check which shards already exist locally
        let mut completed = Vec::new();
        for filename in &shard_files {
            let local_path = model_dir.join(filename);
            if local_path.exists() {
                completed.push(filename.clone());
            }
        }

        Ok((completed.len(), total_shards, completed))
    }
}

/// Kill a process by PID using platform-specific APIs.
///
/// This is a synchronous function that sends a termination signal to the process.
/// On Unix (macOS/Linux), it sends SIGKILL. On Windows, it uses TerminateProcess.
///
/// # Arguments
///
/// * `pid` - The process ID to terminate
#[cfg(unix)]
fn kill_process_by_pid(pid: u32) {
    use nix::sys::signal::{Signal, kill};
    use nix::unistd::Pid;
    use tracing::warn;

    let nix_pid = Pid::from_raw(pid as i32);
    if let Err(e) = kill(nix_pid, Signal::SIGKILL) {
        // ESRCH means process doesn't exist (already terminated) - that's fine
        if e != nix::errno::Errno::ESRCH {
            warn!(pid = pid, error = %e, "Failed to kill process");
        }
    }
}

#[cfg(windows)]
fn kill_process_by_pid(pid: u32) {
    use tracing::warn;
    use windows::Win32::Foundation::CloseHandle;
    use windows::Win32::System::Threading::{OpenProcess, PROCESS_TERMINATE, TerminateProcess};

    unsafe {
        let handle = OpenProcess(PROCESS_TERMINATE, false, pid);
        match handle {
            Ok(h) => {
                if let Err(e) = TerminateProcess(h, 1) {
                    warn!(pid = pid, error = ?e, "Failed to terminate process");
                }
                let _ = CloseHandle(h);
            }
            Err(e) => {
                // ERROR_INVALID_PARAMETER means process doesn't exist - that's fine
                warn!(pid = pid, error = ?e, "Failed to open process for termination");
            }
        }
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

    #[tokio::test]
    async fn test_queue_sharded_download() {
        let service = DownloadService::new();
        let shard_files = vec![
            "Q4_K_M/model-00001-of-00003.gguf".to_string(),
            "Q4_K_M/model-00002-of-00003.gguf".to_string(),
            "Q4_K_M/model-00003-of-00003.gguf".to_string(),
        ];

        let result = service
            .queue_sharded_download("test/model".to_string(), "Q4_K_M".to_string(), shard_files)
            .await;

        assert!(result.is_ok());
        assert_eq!(result.unwrap(), 1); // First position

        let status = service.get_queue_status().await;
        assert_eq!(status.pending.len(), 3); // 3 shards

        // Verify shard info is set correctly
        assert!(status.pending[0].shard_info.is_some());
        let shard = status.pending[0].shard_info.as_ref().unwrap();
        assert_eq!(shard.shard_index, 0);
        assert_eq!(shard.total_shards, 3);

        // All items should share the same group_id
        let group_id = status.pending[0].group_id.as_ref().unwrap();
        assert!(
            status
                .pending
                .iter()
                .all(|p| p.group_id.as_ref() == Some(group_id))
        );
    }

    #[tokio::test]
    async fn test_remove_shard_group() {
        let service = DownloadService::new();
        let shard_files = vec![
            "model-00001.gguf".to_string(),
            "model-00002.gguf".to_string(),
        ];

        service
            .queue_sharded_download("test/model".to_string(), "Q4_K_M".to_string(), shard_files)
            .await
            .unwrap();

        let status = service.get_queue_status().await;
        let group_id = status.pending[0].group_id.clone().unwrap();

        // Remove all shards in the group
        let removed = service.remove_shard_group(&group_id).await;
        assert_eq!(removed, 2);

        let status_after = service.get_queue_status().await;
        assert_eq!(status_after.pending.len(), 0);
    }

    #[tokio::test]
    async fn test_fail_shard_group() {
        let service = DownloadService::new();
        let shard_files = vec![
            "model-00001.gguf".to_string(),
            "model-00002.gguf".to_string(),
        ];

        service
            .queue_sharded_download("test/model".to_string(), "Q4_K_M".to_string(), shard_files)
            .await
            .unwrap();

        let status = service.get_queue_status().await;
        let group_id = status.pending[0].group_id.clone().unwrap();

        // Fail all shards in the group
        let failed_count = service.fail_shard_group(&group_id).await;
        assert_eq!(failed_count, 2);

        let status_after = service.get_queue_status().await;
        assert_eq!(status_after.pending.len(), 0);
        assert_eq!(status_after.failed.len(), 2);
    }

    #[tokio::test]
    async fn test_queue_capacity_for_shards() {
        let service = DownloadService::new();
        service.set_max_queue_size(3).await;

        // Queue 1 regular download
        service
            .queue_download("model1".to_string(), None)
            .await
            .unwrap();

        // Try to queue 3 shards - should fail (would exceed max of 3)
        let shard_files = vec![
            "model-00001.gguf".to_string(),
            "model-00002.gguf".to_string(),
            "model-00003.gguf".to_string(),
        ];

        let result = service
            .queue_sharded_download(
                "test/sharded".to_string(),
                "Q4_K_M".to_string(),
                shard_files,
            )
            .await;

        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("full"));
    }

    #[tokio::test]
    async fn test_create_shard_batch() {
        let (group_id, shards) = QueuedDownload::create_shard_batch(
            "test/model",
            "Q4_K_M",
            &[
                "model-00001.gguf".to_string(),
                "model-00002.gguf".to_string(),
            ],
        );

        assert!(!group_id.is_empty());
        assert_eq!(shards.len(), 2);

        // Check first shard
        assert_eq!(shards[0].model_id, "test/model");
        assert_eq!(shards[0].quantization, Some("Q4_K_M".to_string()));
        assert_eq!(shards[0].group_id, Some(group_id.clone()));
        let info0 = shards[0].shard_info.as_ref().unwrap();
        assert_eq!(info0.shard_index, 0);
        assert_eq!(info0.total_shards, 2);
        assert_eq!(info0.filename, "model-00001.gguf");

        // Check second shard
        let info1 = shards[1].shard_info.as_ref().unwrap();
        assert_eq!(info1.shard_index, 1);
        assert_eq!(info1.total_shards, 2);
        assert_eq!(info1.filename, "model-00002.gguf");
    }
}
