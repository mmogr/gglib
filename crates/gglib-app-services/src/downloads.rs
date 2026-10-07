//! Download queue operations for GUI backend.

use std::sync::Arc;

use gglib_core::download::{DownloadId, QueueSnapshot};
use gglib_core::ports::{
    DownloadManagerPort, HfClientPort, HfSearchOptions, ToolSupportDetectorPort,
};

use crate::error::GuiError;
use crate::hf_quantizations::quantizations_response;
use crate::types::{
    HfModelSummary, HfQuantizationsResponse, HfSearchRequest, HfSearchResponse,
    QueueDownloadResponse, ToolSupportResponse,
};

/// Dependencies for download and `HuggingFace` operations.
pub struct DownloadDeps {
    pub downloads: Arc<dyn DownloadManagerPort>,
    pub hf: Arc<dyn HfClientPort>,
    pub tool_detector: Arc<dyn ToolSupportDetectorPort>,
}

/// Download and `HuggingFace` operations handler.
pub struct DownloadOps {
    downloads: Arc<dyn DownloadManagerPort>,
    hf_client: Arc<dyn HfClientPort>,
    tool_detector: Arc<dyn ToolSupportDetectorPort>,
}

impl DownloadOps {
    pub fn new(deps: DownloadDeps) -> Self {
        Self {
            downloads: deps.downloads,
            hf_client: deps.hf,
            tool_detector: deps.tool_detector,
        }
    }

    // =========================================================================
    // Download Queue Operations
    // =========================================================================

    /// Queue a model download from `HuggingFace` Hub, and answer its ID.
    ///
    /// Uses smart quantization selection:
    /// - If quantization is provided, validates it exists
    /// - If none provided and 1 option exists, auto-picks it
    /// - If none provided and multiple exist, uses default preference order
    pub async fn queue_download(
        &self,
        model_id: String,
        quantization: Option<String>,
    ) -> Result<QueueDownloadResponse, GuiError> {
        // Use queue_smart which handles quantization selection in the domain layer
        let id = Arc::clone(&self.downloads)
            .queue_smart(model_id, quantization)
            .await
            .map_err(|e| GuiError::Internal(e.to_string()))?;
        Ok(QueueDownloadResponse { id: id.to_string() })
    }

    /// Cancel a download that is waiting or running, every file of it.
    pub async fn cancel_download(&self, model_id: &str) -> Result<(), GuiError> {
        let id = DownloadId::from(model_id);
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

    /// Cancel a waiting or running download, or drop the entry of an ended one.
    pub async fn remove_from_queue(&self, model_id: &str) -> Result<(), GuiError> {
        let id = DownloadId::from(model_id);
        self.downloads
            .remove_from_queue(&id)
            .await
            .map_err(GuiError::from)
    }

    /// Reorder a queued download to a new position.
    ///
    /// The position is the one a queue snapshot gives a download: 1 is the
    /// running download, when there is one, and the waiting ones follow.
    pub async fn reorder_queue(
        &self,
        model_id: &str,
        new_position: usize,
    ) -> Result<usize, GuiError> {
        let id = DownloadId::from(model_id);
        let actual_position = self
            .downloads
            .reorder_queue(&id, new_position as u32)
            .await
            .map_err(GuiError::from)?;
        Ok(actual_position as usize)
    }

    /// Reorder the download queue using a full ordering array.
    ///
    /// `ids` are the waiting downloads in the order wanted, one id each. Each
    /// is moved in turn to its place in the array, the first to position 1.
    /// Behind a running download the waiting places start at 2, so the order
    /// that results can differ from the one given. An id that is not waiting
    /// is skipped.
    pub async fn reorder_queue_full(&self, ids: &[String]) -> Result<(), GuiError> {
        for (position, model_id) in ids.iter().enumerate() {
            let id = DownloadId::from(model_id.as_str());
            let _ = self
                .downloads
                .reorder_queue(&id, (position + 1) as u32)
                .await;
        }
        Ok(())
    }

    /// Clear the record of how earlier downloads ended.
    pub async fn clear_finished(&self) {
        let _ = self.downloads.clear_finished().await;
    }

    /// Cancel all active and queued downloads.
    pub async fn cancel_all(&self) {
        let _ = self.downloads.cancel_all().await;
    }

    // =========================================================================
    // HuggingFace Browser Operations
    // =========================================================================

    /// Search `HuggingFace` for GGUF text-generation models: the browser's
    /// search, which is [`search_hf_models`] over this handler's Hub client.
    pub async fn search_hf_models(
        &self,
        request: HfSearchRequest,
    ) -> Result<HfSearchResponse, GuiError> {
        search_hf_models(self.hf_client.as_ref(), request).await
    }

    /// Get available quantizations for a `HuggingFace` model, each with the
    /// projector its download fetches.
    pub async fn get_model_quantizations(
        &self,
        model_id: &str,
    ) -> Result<HfQuantizationsResponse, GuiError> {
        let failed = |e| GuiError::Internal(format!("Failed to get quantizations: {e}"));
        let quants = self
            .hf_client
            .list_quantizations(model_id)
            .await
            .map_err(failed)?;
        let projectors = self
            .hf_client
            .list_projectors(model_id)
            .await
            .map_err(failed)?;

        Ok(quantizations_response(model_id, quants, &projectors))
    }

    /// Check if a `HuggingFace` model supports tool/function calling.
    pub async fn get_hf_tool_support(
        &self,
        model_id: &str,
    ) -> Result<ToolSupportResponse, GuiError> {
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

        Ok(ToolSupportResponse::from(detection))
    }

    /// Get model summary by exact repo ID (direct API lookup).
    ///
    /// Unlike search, this fetches model info directly from the `HuggingFace` API
    /// using the exact repo ID (e.g., `unsloth/medgemma-4b-it-GGUF`).
    ///
    /// Returns an error if the model doesn't exist or has no GGUF files.
    pub async fn get_model_summary(&self, model_id: &str) -> Result<HfModelSummary, GuiError> {
        // Fetch model info directly by ID
        let info =
            self.hf_client
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

        Ok(info.into())
    }
}

/// Search `hf` for GGUF text-generation models.
///
/// The one search every surface runs: the browser's, through
/// [`DownloadOps::search_hf_models`], and `gglib model search` and `browse`,
/// which hold the Hub client and no `DownloadOps`.
pub async fn search_hf_models(
    hf: &dyn HfClientPort,
    request: HfSearchRequest,
) -> Result<HfSearchResponse, GuiError> {
    let options = HfSearchOptions {
        query: request.query,
        min_params_b: request.min_params_b,
        max_params_b: request.max_params_b,
        page: request.page,
        limit: request.limit,
        sort_by: request.sort_by,
        sort_ascending: request.sort_ascending,
    };

    let response = hf
        .search(&options)
        .await
        .map_err(|e| GuiError::Internal(format!("HF search failed: {e}")))?;

    Ok(HfSearchResponse {
        models: response.items.into_iter().map(Into::into).collect(),
        has_more: response.has_more,
        page: response.page,
        total_count: None,
    })
}

#[cfg(test)]
#[path = "downloads_tests.rs"]
mod tests;

#[cfg(test)]
#[path = "downloads_search_tests.rs"]
mod search_tests;
