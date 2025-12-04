//! Download manager - thin orchestration facade.
//!
//! This module provides `DownloadManager`, a lightweight facade that coordinates
//! the queue, executor, and progress emission. Target complexity: ≤30.

use std::path::PathBuf;
use std::sync::Arc;

use tokio::sync::RwLock;
use tokio_util::sync::CancellationToken;

use crate::download::domain::errors::DownloadError;
use crate::download::domain::types::{DownloadId, DownloadRequest};
use crate::download::executor::{EventCallback, ExecutionResult, PythonDownloadExecutor};
use crate::download::huggingface::{resolve_quantization_files, FileResolution};
use crate::download::progress::build_queue_snapshot;
use crate::download::queue::DownloadQueue;
use crate::services::core::PidStorage;

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
    /// Event callback for frontend updates.
    on_event: EventCallback,
    /// Cancellation tokens for active downloads.
    cancel_tokens: Arc<RwLock<std::collections::HashMap<String, CancellationToken>>>,
}

impl DownloadManager {
    /// Create a new download manager.
    pub fn new(config: DownloadManagerConfig, on_event: EventCallback) -> Self {
        Self {
            queue: Arc::new(RwLock::new(DownloadQueue::new(config.max_queue_size as u32))),
            pending_requests: Arc::new(RwLock::new(std::collections::HashMap::new())),
            executor: PythonDownloadExecutor::new(),
            on_event,
            cancel_tokens: Arc::new(RwLock::new(std::collections::HashMap::new())),
            config,
        }
    }

    /// Create with PID storage for process tracking.
    pub fn with_pid_storage(
        config: DownloadManagerConfig,
        on_event: EventCallback,
        pid_storage: PidStorage,
    ) -> Self {
        Self {
            queue: Arc::new(RwLock::new(DownloadQueue::new(config.max_queue_size as u32))),
            pending_requests: Arc::new(RwLock::new(std::collections::HashMap::new())),
            executor: PythonDownloadExecutor::with_pid_storage(pid_storage),
            on_event,
            cancel_tokens: Arc::new(RwLock::new(std::collections::HashMap::new())),
            config,
        }
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

            // Emit queue snapshot (item now "current")
            self.emit_queue_snapshot().await;

            // Execute download
            let result = self
                .executor
                .execute(&request, &self.on_event, cancel_token)
                .await;

            // Remove cancellation token
            {
                let mut tokens = self.cancel_tokens.write().await;
                tokens.remove(&queued.id.to_string());
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
                    let failed = crate::download::queue::FailedDownload::new(queued.clone(), &e.to_string());
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
        queue.snapshot(None)
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
        let snapshot = queue.snapshot(None);
        let event = build_queue_snapshot(&snapshot);
        (self.on_event)(event);
    }
}

// ============================================================================
// Helpers
// ============================================================================

/// Sanitize model name for filesystem use.
fn sanitize_model_name(model_id: &str) -> String {
    model_id.replace('/', "_").replace('\\', "_")
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
