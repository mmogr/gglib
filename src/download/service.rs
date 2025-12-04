//! Download manager - thin orchestration facade.
//!
//! This module provides `DownloadManager`, a lightweight facade that coordinates
//! the queue, executor, and progress emission. Target complexity: ≤30.

use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use tokio::sync::RwLock;
use tokio_util::sync::CancellationToken;

use crate::download::domain::errors::DownloadError;
use crate::download::domain::events::DownloadStatus;
use crate::download::domain::types::{DownloadId, DownloadRequest};
use crate::download::executor::{EventCallback, ExecutionResult, PythonDownloadExecutor};
use crate::download::huggingface::{FileResolution, resolve_quantization_files};
use crate::download::progress::build_queue_snapshot;
use crate::download::queue::{DownloadQueue, QueuedDownload, ShardGroupId};
use crate::services::core::{DownloadProcessManager, PidStorage};

// ============================================================================
// Configuration
// ============================================================================

/// Configuration for the download manager.
#[derive(Debug, Clone)]
pub struct DownloadManagerConfig {
    /// Base directory for downloaded models.
    pub models_dir: PathBuf,
    /// Maximum queue size.
    pub max_queue_size: usize,
    /// HuggingFace authentication token.
    pub hf_token: Option<String>,
}

impl Default for DownloadManagerConfig {
    fn default() -> Self {
        Self {
            models_dir: PathBuf::from("models"),
            max_queue_size: 100,
            hf_token: None,
        }
    }
}

// ============================================================================
// Download Manager
// ============================================================================

/// Orchestrates downloads with queue management and event emission.
///
/// This is a thin facade that coordinates:
/// - Queue state management (`DownloadQueue`)
/// - Download execution (`PythonDownloadExecutor`)
/// - Process management (`DownloadProcessManager`)
/// - Event emission to frontend
///
/// Design: Sync collaborator pattern - all domain logic is pure,
/// I/O happens at the edges.
pub struct DownloadManager {
    /// Configuration.
    config: DownloadManagerConfig,
    /// Download queue (protected by RwLock for concurrent access).
    queue: Arc<RwLock<DownloadQueue>>,
    /// Pending requests waiting to be executed.
    pending_requests: Arc<RwLock<std::collections::HashMap<String, DownloadRequest>>>,
    /// Python executor.
    executor: PythonDownloadExecutor,
    /// Event callback for frontend updates (wrapped in RwLock for runtime replacement).
    on_event: Arc<std::sync::RwLock<EventCallback>>,
    /// Cancellation tokens for active downloads.
    cancel_tokens: Arc<RwLock<std::collections::HashMap<String, CancellationToken>>>,
    /// Currently downloading item (for queue snapshots).
    current_download: Arc<RwLock<Option<QueuedDownload>>>,
    /// Process manager for synchronous termination on shutdown.
    process_manager: DownloadProcessManager,
}

impl DownloadManager {
    /// Create a new download manager.
    pub fn new(config: DownloadManagerConfig, on_event: EventCallback) -> Self {
        let process_manager = DownloadProcessManager::new();
        Self {
            queue: Arc::new(RwLock::new(DownloadQueue::new(
                config.max_queue_size as u32,
            ))),
            pending_requests: Arc::new(RwLock::new(std::collections::HashMap::new())),
            executor: PythonDownloadExecutor::with_pid_storage(process_manager.pid_storage()),
            on_event: Arc::new(std::sync::RwLock::new(on_event)),
            cancel_tokens: Arc::new(RwLock::new(std::collections::HashMap::new())),
            current_download: Arc::new(RwLock::new(None)),
            process_manager,
            config,
        }
    }

    /// Create with PID storage for process tracking.
    pub fn with_pid_storage(
        config: DownloadManagerConfig,
        on_event: EventCallback,
        pid_storage: PidStorage,
    ) -> Self {
        let process_manager = DownloadProcessManager::new();
        // Use the provided pid_storage for the executor
        Self {
            queue: Arc::new(RwLock::new(DownloadQueue::new(
                config.max_queue_size as u32,
            ))),
            pending_requests: Arc::new(RwLock::new(std::collections::HashMap::new())),
            executor: PythonDownloadExecutor::with_pid_storage(pid_storage),
            on_event: Arc::new(std::sync::RwLock::new(on_event)),
            cancel_tokens: Arc::new(RwLock::new(std::collections::HashMap::new())),
            current_download: Arc::new(RwLock::new(None)),
            process_manager,
            config,
        }
    }

    /// Set the event callback.
    ///
    /// This allows replacing the callback at runtime, e.g., to wire up
    /// broadcast channels after the manager is created.
    pub fn set_event_callback(&self, callback: EventCallback) {
        if let Ok(mut guard) = self.on_event.write() {
            *guard = callback;
        }
    }

    /// Get a cloned event callback for use in async contexts.
    fn get_event_callback(&self) -> EventCallback {
        self.on_event
            .read()
            .map(|g| g.clone())
            .unwrap_or_else(|_| Arc::new(|_| {}))
    }

    /// Queue a download request.
    ///
    /// Returns the download ID and queue position.
    pub async fn queue_download(
        &self,
        repo_id: impl Into<String>,
        quantization: impl Into<String>,
    ) -> Result<(DownloadId, u32), DownloadError> {
        let repo_id = repo_id.into();
        let quantization = quantization.into();
        let id = DownloadId::new(&repo_id, Some(&quantization));

        // Check for duplicates
        let has_active = {
            let queue = self.queue.read().await;
            if queue.is_queued(&id) {
                return Err(DownloadError::already_queued(id.to_string()));
            }
            // Has active if we're tracking a cancellation token
            !self.cancel_tokens.read().await.is_empty()
        };

        // Resolve files from HuggingFace
        let resolution = resolve_quantization_files(&repo_id, &quantization)
            .await
            .map_err(|e| DownloadError::not_found(e.to_string()))?;

        // Store the request for later execution
        let request = self.build_request(&id, &repo_id, &resolution);

        // Add to queue
        let position = {
            let mut queue = self.queue.write().await;
            queue.queue(id.clone(), has_active)?
        };

        // Store request in a separate map for the executor
        {
            let mut requests = self.pending_requests.write().await;
            requests.insert(id.to_string(), request);
        }

        // Emit queue snapshot
        self.emit_queue_snapshot().await;

        Ok((id, position))
    }

    /// Start processing the queue.
    ///
    /// This processes downloads one at a time until the queue is empty
    /// or processing is stopped.
    pub async fn process_queue(&self) -> Result<(), DownloadError> {
        loop {
            // Get next download
            let queued = {
                let mut queue = self.queue.write().await;
                queue.dequeue()
            };

            let Some(queued) = queued else {
                break; // Queue empty
            };

            // Get the request
            let request = {
                let requests = self.pending_requests.read().await;
                requests.get(&queued.id.to_string()).cloned()
            };

            let Some(request) = request else {
                // Request was removed (cancelled?) - skip
                continue;
            };

            // Create cancellation token
            let cancel_token = CancellationToken::new();
            {
                let mut tokens = self.cancel_tokens.write().await;
                tokens.insert(queued.id.to_string(), cancel_token.clone());
            }

            // Track as current download (for queue snapshots)
            {
                let mut current = self.current_download.write().await;
                *current = Some(queued.clone());
            }

            // Emit queue snapshot (item now "current")
            self.emit_queue_snapshot().await;

            // Execute download
            let event_callback = self.get_event_callback();
            let result = self
                .executor
                .execute(&request, &event_callback, cancel_token)
                .await;

            // Remove cancellation token
            {
                let mut tokens = self.cancel_tokens.write().await;
                tokens.remove(&queued.id.to_string());
            }

            // Clear current download tracking
            {
                let mut current = self.current_download.write().await;
                *current = None;
            }

            // Remove request from pending
            {
                let mut requests = self.pending_requests.write().await;
                requests.remove(&queued.id.to_string());
            }

            // Handle result
            match result {
                Ok(ExecutionResult::Completed) => {
                    // Successfully completed - no queue update needed
                }
                Ok(ExecutionResult::Cancelled) => {
                    // Already handled by executor
                }
                Err(e) => {
                    let mut queue = self.queue.write().await;
                    let error_msg = e.to_string();
                    let failed =
                        crate::download::queue::FailedDownload::new(queued.clone(), &error_msg);
                    queue.mark_failed(failed);
                }
            }

            // Emit updated queue snapshot
            self.emit_queue_snapshot().await;
        }

        Ok(())
    }

    /// Cancel a specific download.
    pub async fn cancel(&self, id: &DownloadId) -> bool {
        // Check if it's the current download
        let tokens = self.cancel_tokens.read().await;
        if let Some(token) = tokens.get(&id.to_string()) {
            token.cancel();
            return true;
        }
        drop(tokens);

        // Otherwise remove from queue
        let mut queue = self.queue.write().await;
        queue.remove(id).is_ok()
    }

    /// Cancel all downloads.
    pub async fn cancel_all(&self) {
        // Cancel current
        let tokens = self.cancel_tokens.read().await;
        for token in tokens.values() {
            token.cancel();
        }
        drop(tokens);

        // Clear queue
        let mut queue = self.queue.write().await;
        queue.clear();
        drop(queue);

        // Clear pending requests
        let mut requests = self.pending_requests.write().await;
        requests.clear();
        drop(requests);

        self.emit_queue_snapshot().await;
    }

    /// Get current queue state.
    pub async fn get_queue_snapshot(&self) -> crate::download::queue::QueueSnapshot {
        let queue = self.queue.read().await;
        let current = self.current_download.read().await;
        let current_summary = current
            .as_ref()
            .map(|q| q.to_summary(1, DownloadStatus::Downloading));
        queue.snapshot(current_summary)
    }

    /// Retry a failed download.
    pub async fn retry(&self, id: &DownloadId) -> Result<u32, DownloadError> {
        let has_active = !self.cancel_tokens.read().await.is_empty();
        let mut queue = self.queue.write().await;
        let position = queue.retry_failed(id, has_active)?;
        drop(queue);

        self.emit_queue_snapshot().await;
        Ok(position)
    }

    // ========================================================================
    // Queue Management Methods
    // ========================================================================

    /// Set the maximum queue size.
    pub async fn set_max_queue_size(&self, size: u32) {
        let mut queue = self.queue.write().await;
        queue.set_max_size(size);
    }

    /// Get the maximum queue size.
    pub async fn get_max_queue_size(&self) -> u32 {
        let queue = self.queue.read().await;
        queue.max_size()
    }

    /// Remove an item from the pending queue.
    ///
    /// Cannot remove the currently downloading item (use cancel instead).
    pub async fn remove_from_queue(&self, id: &DownloadId) -> Result<(), DownloadError> {
        let mut queue = self.queue.write().await;
        queue.remove(id)?;
        drop(queue);

        // Also remove from pending requests
        let mut requests = self.pending_requests.write().await;
        requests.remove(&id.to_string());
        drop(requests);

        self.emit_queue_snapshot().await;
        Ok(())
    }

    /// Reorder a queued item (or shard group) to a new position.
    ///
    /// # Arguments
    ///
    /// * `id` - The download ID to move
    /// * `new_position` - The target position (1-based)
    ///
    /// # Returns
    ///
    /// The actual 1-based position where the item(s) were placed.
    pub async fn reorder_queue(
        &self,
        id: &DownloadId,
        new_position: u32,
    ) -> Result<u32, DownloadError> {
        let has_active = !self.cancel_tokens.read().await.is_empty();
        let mut queue = self.queue.write().await;
        let actual_position = queue.reorder(id, new_position, has_active)?;
        drop(queue);

        self.emit_queue_snapshot().await;
        Ok(actual_position)
    }

    /// Cancel an active download and remove all related shards from queue.
    ///
    /// If the active download belongs to a shard group, this will also
    /// remove all pending shards in that group.
    pub async fn cancel_shard_group(&self, group_id: &ShardGroupId) -> Result<(), DownloadError> {
        // Find any active download for this shard group
        let active_id = {
            let tokens = self.cancel_tokens.read().await;
            let queue = self.queue.read().await;

            // Check if any active download belongs to this group
            // by looking at pending items that share the group_id
            tokens
                .keys()
                .find_map(|key| {
                    let id: DownloadId = key.parse().ok()?;
                    if queue.is_queued(&id) {
                        None // Still in queue, not active
                    } else {
                        Some(id)
                    }
                })
                .or_else(|| {
                    // Also try parsing the first queued item's group
                    None
                })
        };

        // Cancel active download if found
        if let Some(id) = active_id {
            self.cancel(&id).await;
        }

        // Remove all pending shards in this group
        let mut queue = self.queue.write().await;
        queue.remove_group(group_id);
        drop(queue);

        // Clean up pending requests for this group
        let mut requests = self.pending_requests.write().await;
        let keys_to_remove: Vec<_> = requests
            .keys()
            .filter(|k| k.contains(&group_id.to_string()))
            .cloned()
            .collect();
        for key in keys_to_remove {
            requests.remove(&key);
        }
        drop(requests);

        self.emit_queue_snapshot().await;
        Ok(())
    }

    /// Clear all failed downloads from the list.
    pub async fn clear_failed(&self) {
        let mut queue = self.queue.write().await;
        queue.clear_failed();
        drop(queue);

        self.emit_queue_snapshot().await;
    }

    /// Queue a download with automatic shard detection.
    ///
    /// This is the preferred method for GUI downloads. It:
    /// 1. Queries HuggingFace to detect if the model is sharded
    /// 2. Creates individual queue items for each shard (with shared group_id)
    /// 3. Or creates a single queue item for non-sharded models
    ///
    /// # Arguments
    ///
    /// * `repo_id` - HuggingFace repository ID
    /// * `quantization` - Quantization type
    ///
    /// # Returns
    ///
    /// Returns `(DownloadId, queue_position, shard_count)` where shard_count is 1
    /// for non-sharded models.
    pub async fn queue_download_auto(
        &self,
        repo_id: impl Into<String>,
        quantization: impl Into<String>,
    ) -> Result<(DownloadId, u32, usize), DownloadError> {
        let repo_id = repo_id.into();
        let quantization = quantization.into();
        let id = DownloadId::new(&repo_id, Some(&quantization));

        // Check for duplicates
        let has_active = {
            let queue = self.queue.read().await;
            if queue.is_queued(&id) {
                return Err(DownloadError::already_queued(id.to_string()));
            }
            !self.cancel_tokens.read().await.is_empty()
        };

        // Resolve files from HuggingFace (this is "resolve → queue")
        let resolution = resolve_quantization_files(&repo_id, &quantization)
            .await
            .map_err(|e| DownloadError::not_found(e.to_string()))?;

        let shard_count = resolution.files.len();
        let is_sharded = resolution.is_sharded;

        // Build the request
        let request = self.build_request(&id, &repo_id, &resolution);

        // Add to queue (handle sharded vs non-sharded)
        let position = if is_sharded {
            let shard_files: Vec<(String, Option<u64>)> = resolution
                .files
                .iter()
                .map(|f| (f.path.clone(), f.size))
                .collect();
            let mut queue = self.queue.write().await;
            queue.queue_sharded(id.clone(), shard_files, has_active)?
        } else {
            let mut queue = self.queue.write().await;
            queue.queue(id.clone(), has_active)?
        };

        // Store request for executor
        {
            let mut requests = self.pending_requests.write().await;
            requests.insert(id.to_string(), request);
        }

        self.emit_queue_snapshot().await;

        Ok((id, position, shard_count))
    }

    /// Retry a failed download by ID.
    ///
    /// For sharded models, this re-resolves files from HuggingFace and
    /// re-queues appropriately.
    pub async fn retry_failed_download(
        &self,
        id: &DownloadId,
    ) -> Result<(u32, usize), DownloadError> {
        // Check if it's in failed list
        {
            let queue = self.queue.read().await;
            if queue.get_failed(id).is_none() {
                return Err(DownloadError::not_in_queue(id.to_string()));
            }
        }

        // Remove from failed list first
        {
            let mut queue = self.queue.write().await;
            // Remove from failed (we'll re-queue with fresh resolution)
            queue.clear_failed(); // TODO: More targeted removal
        }

        // Re-queue using auto-detect
        let quant = id.quantization().unwrap_or("Q4_K_M");
        let (_, position, shard_count) = self.queue_download_auto(id.model_id(), quant).await?;

        Ok((position, shard_count))
    }

    /// Check if a download is currently active (being downloaded).
    pub async fn is_downloading(&self, id: &DownloadId) -> bool {
        let tokens = self.cancel_tokens.read().await;
        tokens.contains_key(&id.to_string())
    }

    /// Get list of currently active download IDs.
    pub async fn active_downloads(&self) -> Vec<DownloadId> {
        let tokens = self.cancel_tokens.read().await;
        tokens.keys().filter_map(|k| k.parse().ok()).collect()
    }

    // ========================================================================
    // Process Management (delegation to DownloadProcessManager)
    // ========================================================================

    /// Cancel all active downloads and wait for processes to stop.
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
    pub async fn cancel_all_and_wait(&self, timeout: Duration) -> usize {
        // Cancel all tokens
        let count = {
            let tokens = self.cancel_tokens.read().await;
            let count = tokens.len();
            for token in tokens.values() {
                token.cancel();
            }
            count
        };

        if count > 0 {
            // Wait for processes to receive kill signal
            tokio::time::sleep(timeout).await;

            // Force kill any remaining processes
            self.process_manager.kill_all_sync();
        }

        // Clear queue and requests
        self.cancel_all().await;

        count
    }

    /// Get a clone of the PID storage for passing to download functions.
    ///
    /// This allows download functions to register their child process PIDs
    /// so they can be killed synchronously on app shutdown.
    pub fn child_pids(&self) -> PidStorage {
        self.process_manager.pid_storage()
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
        self.process_manager.kill_all_sync()
    }

    /// Build a download request from resolved files.
    fn build_request(
        &self,
        id: &DownloadId,
        repo_id: &str,
        resolution: &FileResolution,
    ) -> DownloadRequest {
        DownloadRequest::builder()
            .id(id.clone())
            .repo_id(repo_id)
            .quantization(resolution.quantization)
            .files(resolution.filenames())
            .destination(self.config.models_dir.join(sanitize_model_name(repo_id)))
            .token(self.config.hf_token.clone())
            .build()
    }

    /// Emit current queue state to frontend.
    async fn emit_queue_snapshot(&self) {
        let queue = self.queue.read().await;
        let current = self.current_download.read().await;
        let current_summary = current
            .as_ref()
            .map(|q| q.to_summary(1, DownloadStatus::Downloading));
        let snapshot = queue.snapshot(current_summary);
        let event = build_queue_snapshot(&snapshot);
        let callback = self.get_event_callback();
        callback(event);
    }
}

// ============================================================================
// Helpers
// ============================================================================

/// Sanitize model name for filesystem use.
fn sanitize_model_name(model_id: &str) -> String {
    model_id.replace(['/', '\\'], "_")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn noop_callback() -> EventCallback {
        Arc::new(|_event| {})
    }

    #[test]
    fn test_sanitize_model_name() {
        assert_eq!(sanitize_model_name("user/model"), "user_model");
        assert_eq!(sanitize_model_name("org/repo/model"), "org_repo_model");
    }

    #[tokio::test]
    async fn test_manager_creation() {
        let config = DownloadManagerConfig::default();
        let _manager = DownloadManager::new(config, noop_callback());
    }

    #[tokio::test]
    async fn test_empty_queue_snapshot() {
        let config = DownloadManagerConfig::default();
        let manager = DownloadManager::new(config, noop_callback());

        let snapshot = manager.get_queue_snapshot().await;
        assert!(snapshot.current.is_none());
        assert!(snapshot.pending.is_empty());
        assert!(snapshot.failed.is_empty());
    }
}
