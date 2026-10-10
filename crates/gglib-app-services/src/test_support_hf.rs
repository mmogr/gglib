//! The `HuggingFace` client stub for gglib-app-services unit tests.

use async_trait::async_trait;
use gglib_core::ports::{
    HfClientPort, HfFileInfo, HfPortError, HfQuantInfo, HfRepoInfo, HfSearchOptions, HfSearchResult,
};

/// Stub `HfClientPort` that returns minimal valid responses: one repository
/// with a `Q4_K_M` quantization and one projector of 7 bytes.
pub(crate) struct MockHfClient;

#[async_trait]
impl HfClientPort for MockHfClient {
    async fn search(&self, _options: &HfSearchOptions) -> Result<HfSearchResult, HfPortError> {
        Ok(HfSearchResult {
            items: vec![],
            has_more: false,
            page: 0,
        })
    }

    async fn list_quantizations(&self, _model_id: &str) -> Result<Vec<HfQuantInfo>, HfPortError> {
        Ok(vec![HfQuantInfo {
            name: "Q4_K_M".to_string(),
            shard_count: 1,
            total_size: 0,
            file_paths: vec!["model.gguf".to_string()],
        }])
    }

    async fn list_gguf_files(&self, _model_id: &str) -> Result<Vec<HfFileInfo>, HfPortError> {
        Ok(vec![])
    }

    async fn list_projectors(&self, _model_id: &str) -> Result<Vec<HfFileInfo>, HfPortError> {
        Ok(vec![HfFileInfo {
            path: "mmproj-F16.gguf".to_string(),
            size: 7,
            is_gguf: true,
            oid: None,
        }])
    }

    async fn get_quantization_files(
        &self,
        _model_id: &str,
        _quantization: &str,
    ) -> Result<Vec<HfFileInfo>, HfPortError> {
        Ok(vec![])
    }

    async fn get_commit_sha(&self, _model_id: &str) -> Result<String, HfPortError> {
        Ok("abc123".to_string())
    }

    async fn get_model_info(&self, model_id: &str) -> Result<HfRepoInfo, HfPortError> {
        Ok(HfRepoInfo {
            model_id: model_id.to_string(),
            name: model_id.to_string(),
            author: None,
            downloads: 0,
            likes: 0,
            parameters_b: None,
            description: None,
            last_modified: None,
            chat_template: None,
            tags: vec![],
        })
    }
    /// No head is held: a listing that reads one to know an image model
    /// finds none, and lists a chat model's quantizations.
    async fn read_head(
        &self,
        model_id: &str,
        path: &str,
        _max_bytes: u64,
    ) -> Result<Vec<u8>, HfPortError> {
        Err(HfPortError::FileNotFound {
            model_id: model_id.to_owned(),
            path: path.to_owned(),
        })
    }
    async fn file_at(
        &self,
        _model_id: &str,
        _path: &str,
    ) -> Result<Option<HfFileInfo>, HfPortError> {
        Ok(None)
    }
}
