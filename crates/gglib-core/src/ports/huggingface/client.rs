//! `HuggingFace` client port trait.

use super::error::HfPortResult;
use super::types::{HfFileInfo, HfQuantInfo, HfRepoInfo, HfSearchOptions, HfSearchResult};
use async_trait::async_trait;

/// Port trait for `HuggingFace` Hub operations.
///
/// This trait defines the interface that the core domain uses to interact
/// with `HuggingFace`. The implementation lives in `gglib-hf`.
///
/// # Design
///
/// - Uses core-owned DTOs, not `HuggingFace` API types
/// - Returns `HfPortError` for all failures
/// - Async methods for network operations
/// - No implementation details leak through this interface
#[async_trait]
pub trait HfClientPort: Send + Sync {
    /// Search for GGUF models on `HuggingFace`.
    async fn search(&self, options: &HfSearchOptions) -> HfPortResult<HfSearchResult>;

    /// List available quantizations for a model.
    ///
    /// # Arguments
    ///
    /// * `model_id` - Full model ID (e.g., `TheBloke/Llama-2-7B-GGUF`)
    async fn list_quantizations(&self, model_id: &str) -> HfPortResult<Vec<HfQuantInfo>>;

    /// List all GGUF files in a model repository.
    ///
    /// # Arguments
    ///
    /// * `model_id` - Full model ID
    async fn list_gguf_files(&self, model_id: &str) -> HfPortResult<Vec<HfFileInfo>>;

    /// Get files for a specific quantization.
    ///
    /// Returns file information including OIDs for all files in the quantization,
    /// sorted for correct shard ordering.
    ///
    /// # Arguments
    ///
    /// * `model_id` - Full model ID
    /// * `quantization` - Quantization name (e.g., `Q4_K_M`)
    async fn get_quantization_files(
        &self,
        model_id: &str,
        quantization: &str,
    ) -> HfPortResult<Vec<HfFileInfo>>;

    /// Get the current commit SHA for a model.
    ///
    /// Used for version tracking and update detection.
    async fn get_commit_sha(&self, model_id: &str) -> HfPortResult<String>;

    /// Get detailed information about a model.
    async fn get_model_info(&self, model_id: &str) -> HfPortResult<HfRepoInfo>;
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;

    // Verify the trait is object-safe
    fn _assert_object_safe(_: Arc<dyn HfClientPort>) {}
}
