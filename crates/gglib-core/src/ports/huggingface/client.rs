//! `HuggingFace` client port trait.

use super::error::HfPortResult;
use super::types::{HfFileInfo, HfQuantInfo, HfRepoInfo, HfSearchOptions, HfSearchResult};
use async_trait::async_trait;

/// How much of a weights file's head is read to learn what it is before it
/// is downloaded: 1 MiB.
///
/// An image model's GGUF header holds no metadata, only its tensor table:
/// the measured Flux.1 schnell `Q8_0` header ends at byte 53,912 and the
/// Qwen-Image 2.1 `Q8_0` header at 20,499. A chat model's metadata (its
/// tokenizer above all) runs past this, and its head is then simply not read
/// as an image model's.
pub const SNIFF_HEAD_BYTES: u64 = 1 << 20;

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
    /// A quantization is made of weights files alone: a projector in the
    /// repository is neither a quantization nor a shard of one.
    ///
    /// # Arguments
    ///
    /// * `model_id` - Full model ID (e.g., `TheBloke/Llama-2-7B-GGUF`)
    async fn list_quantizations(&self, model_id: &str) -> HfPortResult<Vec<HfQuantInfo>>;

    /// List the projector files in a model repository, by path, with OIDs.
    ///
    /// Empty for a repository that has none.
    async fn list_projectors(&self, model_id: &str) -> HfPortResult<Vec<HfFileInfo>>;

    /// List all GGUF files in a model repository.
    ///
    /// # Arguments
    ///
    /// * `model_id` - Full model ID
    async fn list_gguf_files(&self, model_id: &str) -> HfPortResult<Vec<HfFileInfo>>;

    /// Get the weights files for a specific quantization.
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
    /// Used for version tracking and update detection. Fails when the Hub
    /// names no commit.
    async fn get_commit_sha(&self, model_id: &str) -> HfPortResult<String>;

    /// Get detailed information about a model.
    async fn get_model_info(&self, model_id: &str) -> HfPortResult<HfRepoInfo>;

    /// Read the first bytes of the file at `path` in repository `model_id`,
    /// at most `max_bytes` of them.
    ///
    /// A ranged read of the address a download reads the whole file from, so
    /// a header can be read before the file is fetched. A file shorter than
    /// `max_bytes` answers all of itself.
    async fn read_head(&self, model_id: &str, path: &str, max_bytes: u64) -> HfPortResult<Vec<u8>>;

    /// The file at `path` in repository `model_id`, with its size and LFS
    /// OID, or `None` when the repository holds no file there.
    ///
    /// One file looked up by its path, for a file that is not a quantization
    /// of the repository, such as an image model's VAE or text encoder.
    async fn file_at(&self, model_id: &str, path: &str) -> HfPortResult<Option<HfFileInfo>>;

    /// Fetch a repository's `generation_config.json`, if it has one.
    ///
    /// `Ok(None)` means the repo has no such file — the ordinary answer for a
    /// GGUF quant repo, and not a fault. `Err` means the fetch itself failed:
    /// gated repo, no network, rate limit.
    ///
    /// # Why this has a default implementation
    ///
    /// It returns `Ok(None)`, which is semantically exact for a client that
    /// cannot fetch one. The caller
    /// ([`crate::services::fetch_published_sampling`]) treats every negative answer the
    /// same way — carry on and fall back to the tag guess — so a client that
    /// does not implement this degrades to gglib's pre-existing behaviour
    /// rather than failing an import.
    async fn fetch_generation_config(&self, model_id: &str) -> HfPortResult<Option<String>> {
        let _ = model_id;
        Ok(None)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;

    // Verify the trait is object-safe
    fn _assert_object_safe(_: Arc<dyn HfClientPort>) {}
}
