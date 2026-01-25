//! Download queue operations for GUI backend.

use std::sync::Arc;

use gglib_core::download::{DownloadId, QueueSnapshot};
use gglib_core::ports::{
    DownloadManagerPort, HfClientPort, HfSearchOptions, ToolSupportDetectorPort,
};

use crate::deps::GuiDeps;
use crate::error::GuiError;
use crate::types::{
    HfModelSummary, HfQuantization, HfQuantizationsResponse, HfSearchRequest, HfSearchResponse,
    HfSortField, HfToolSupportResponse,
};

/// Download and HuggingFace operations handler.
pub struct DownloadOps<'a> {
    downloads: &'a Arc<dyn DownloadManagerPort>,
    hf_client: &'a Arc<dyn HfClientPort>,
    tool_detector: &'a Arc<dyn ToolSupportDetectorPort>,
}

impl<'a> DownloadOps<'a> {
    pub fn new(deps: &'a GuiDeps) -> Self {
        Self {
            downloads: &deps.downloads,
            hf_client: &deps.hf,
            tool_detector: &deps.tool_detector,
        }
    }

    // =========================================================================
    // Download Queue Operations
    // =========================================================================

    /// Queue a model download from HuggingFace Hub.
    ///
    /// Uses smart quantization selection:
    /// - If quantization is provided, validates it exists
    /// - If none provided and 1 option exists, auto-picks it
    /// - If none provided and multiple exist, uses default preference order
    pub async fn queue_download(
        &self,
        model_id: String,
        quantization: Option<String>,
    ) -> Result<(usize, usize), GuiError> {
        // Use queue_smart which handles quantization selection in the domain layer
        Arc::clone(self.downloads)
            .queue_smart(model_id, quantization)
            .await
            .map_err(|e| GuiError::Internal(e.to_string()))
    }

    /// Cancel an in-flight download.
    pub async fn cancel_download(&self, model_id: &str) -> Result<(), GuiError> {
        let id: DownloadId = model_id
            .parse()
            .unwrap_or_else(|_| DownloadId::from_model(model_id));
        self.downloads
            .cancel_download(&id)
            .await
            .map_err(|_| GuiError::NotFound {
                entity: "download",
                id: model_id.to_string(),
            })
    }

    /// Get the current status of the download queue.
    pub async fn get_queue_snapshot(&self) -> QueueSnapshot {
        self.downloads
            .get_queue_snapshot()
            .await
            .unwrap_or_default()
    }

    /// Remove an item from the pending download queue.
    pub async fn remove_from_queue(&self, model_id: &str) -> Result<(), GuiError> {
        let id: DownloadId = model_id
            .parse()
            .unwrap_or_else(|_| DownloadId::from_model(model_id));
        self.downloads
            .remove_from_queue(&id)
            .await
            .map_err(GuiError::from)
    }

    /// Reorder a queued download to a new position.
    pub async fn reorder_queue(
        &self,
        model_id: &str,
        new_position: usize,
    ) -> Result<usize, GuiError> {
        let id: DownloadId = model_id
            .parse()
            .unwrap_or_else(|_| DownloadId::from_model(model_id));
        let actual_position = self
            .downloads
            .reorder_queue(&id, new_position as u32)
            .await
            .map_err(GuiError::from)?;
        Ok(actual_position as usize)
    }

    /// Reorder the download queue using a full ordering array.
    pub async fn reorder_queue_full(&self, ids: &[String]) -> Result<(), GuiError> {
        for (position, model_id) in ids.iter().enumerate() {
            let id: DownloadId = model_id
                .parse()
                .unwrap_or_else(|_| DownloadId::from_model(model_id));
            let _ = self
                .downloads
                .reorder_queue(&id, (position + 1) as u32)
                .await;
        }
        Ok(())
    }

    /// Cancel all shards in a shard group.
    pub async fn cancel_shard_group(&self, group_id: &str) -> Result<(), GuiError> {
        self.downloads
            .cancel_group(group_id)
            .await
            .map_err(GuiError::from)
    }

    /// Clear all failed downloads from the list.
    pub async fn clear_failed(&self) {
        let _ = self.downloads.clear_failed().await;
    }

    /// Cancel all active and queued downloads.
    pub async fn cancel_all(&self) {
        let _ = self.downloads.cancel_all().await;
    }

    // =========================================================================
    // HuggingFace Browser Operations
    // =========================================================================

    /// Search HuggingFace for GGUF text-generation models.
    pub async fn search_hf_models(
        &self,
        request: HfSearchRequest,
    ) -> Result<HfSearchResponse, GuiError> {
        let options = HfSearchOptions {
            query: request.query,
            min_params_b: request.min_params_b,
            max_params_b: request.max_params_b,
            page: request.page,
            limit: request.limit,
            sort_by: match request.sort_by {
                HfSortField::Downloads => "downloads".to_string(),
                HfSortField::Likes => "likes".to_string(),
                HfSortField::Created => "created".to_string(),
                HfSortField::Modified => "modified".to_string(),
                HfSortField::Alphabetical => "id".to_string(),
            },
            sort_ascending: request.sort_ascending,
        };

        let response = self
            .hf_client
            .search(&options)
            .await
            .map_err(|e| GuiError::Internal(format!("HF search failed: {e}")))?;

        Ok(HfSearchResponse {
            models: response
                .items
                .into_iter()
                .map(|m| HfModelSummary {
                    id: m.model_id,
                    name: m.name,
                    author: m.author,
                    downloads: m.downloads,
                    likes: m.likes,
                    last_modified: m.last_modified,
                    parameters_b: m.parameters_b,
                    description: m.description,
                    tags: vec![],
                })
                .collect(),
            has_more: response.has_more,
            page: response.page,
            total_count: None,
        })
    }

    /// Get available quantizations for a HuggingFace model.
    pub async fn get_model_quantizations(
        &self,
        model_id: &str,
    ) -> Result<HfQuantizationsResponse, GuiError> {
        let quants = self
            .hf_client
            .list_quantizations(model_id)
            .await
            .map_err(|e| GuiError::Internal(format!("Failed to get quantizations: {e}")))?;

        Ok(HfQuantizationsResponse {
            model_id: model_id.to_string(),
            quantizations: quants
                .into_iter()
                .map(|q| HfQuantization {
                    name: q.name.clone(),
                    file_path: q.file_paths.first().cloned().unwrap_or_default(),
                    size_bytes: q.total_size,
                    size_mb: q.total_size as f64 / 1_048_576.0,
                    is_sharded: q.shard_count > 1,
                    shard_count: if q.shard_count > 1 {
                        Some(q.shard_count as u32)
                    } else {
                        None
                    },
                })
                .collect(),
        })
    }

    /// Check if a HuggingFace model supports tool/function calling.
    pub async fn get_hf_tool_support(
        &self,
        model_id: &str,
    ) -> Result<HfToolSupportResponse, GuiError> {
        use gglib_core::ports::{ModelSource, ToolSupportDetectionInput};

        // Fetch model info from HuggingFace
        let model_info = self
            .hf_client
            .get_model_info(model_id)
            .await
            .map_err(|e| GuiError::Internal(format!("Failed to get model info: {e}")))?;

        // Detect tool support using chat template and tags
        let detection = self.tool_detector.detect(ToolSupportDetectionInput {
            model_id: &model_info.model_id,
            chat_template: model_info.chat_template.as_deref(),
            tags: &model_info.tags,
            source: ModelSource::HuggingFace,
        });

        Ok(HfToolSupportResponse::from(detection))
    }

    /// Get model summary by exact repo ID (direct API lookup).
    ///
    /// Unlike search, this fetches model info directly from the HuggingFace API
    /// using the exact repo ID (e.g., `unsloth/medgemma-4b-it-GGUF`).
    ///
    /// Returns an error if the model doesn't exist or has no GGUF files.
    pub async fn get_model_summary(&self, model_id: &str) -> Result<HfModelSummary, GuiError> {
        // Fetch model info directly by ID
        let info = self
            .hf_client
            .get_model_info(model_id)
            .await
            .map_err(|e| GuiError::NotFound {
                entity: "model",
                id: format!("{model_id}: {e}"),
            })?;

        // Check if the model has GGUF files by checking quantizations
        let quants = self
            .hf_client
            .list_quantizations(model_id)
            .await
            .map_err(|e| GuiError::Internal(format!("Failed to check quantizations: {e}")))?;

        if quants.is_empty() {
            return Err(GuiError::ValidationFailed(format!(
                "Model '{model_id}' exists but contains no GGUF files"
            )));
        }

        // Map HfRepoInfo to HfModelSummary
        Ok(HfModelSummary {
            id: info.model_id,
            name: info.name,
            author: info.author,
            downloads: info.downloads,
            likes: info.likes,
            last_modified: info.last_modified,
            parameters_b: info.parameters_b,
            description: info.description,
            tags: info.tags,
        })
    }
}
