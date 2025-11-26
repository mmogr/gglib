//! Download service for HuggingFace model downloads.
//!
//! This service provides managed downloads with progress tracking
//! and cancellation support.

use anyhow::Result;
use std::collections::HashMap;
use std::sync::Arc;
use thiserror::Error;
use tokio::sync::RwLock;
use tokio_util::sync::CancellationToken;

/// Errors related to download operations.
#[derive(Debug, Error)]
pub enum DownloadError {
    #[error("Download '{model_id}' was cancelled by the user")]
    Cancelled { model_id: String },

    #[error("A download for '{model_id}' is already running")]
    AlreadyRunning { model_id: String },

    #[error("No active download for '{model_id}'")]
    NotFound { model_id: String },
}

/// Service for managing HuggingFace model downloads.
///
/// Provides download management with:
/// - Progress tracking via callbacks
/// - Cancellation support
/// - Concurrent download tracking (prevents duplicate downloads)
pub struct DownloadService {
    active_downloads: Arc<RwLock<HashMap<String, CancellationToken>>>,
}

impl DownloadService {
    /// Create a new DownloadService.
    pub fn new() -> Self {
        Self {
            active_downloads: Arc::new(RwLock::new(HashMap::new())),
        }
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
}
