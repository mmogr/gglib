//! Port trait implementation for `HfClient`.
//!
//! This module implements the core-owned `HfClientPort` trait for `HfClient`,
//! handling the conversion between internal `HuggingFace` types and core DTOs.

use async_trait::async_trait;
use gglib_core::ports::huggingface::{
    HfClientPort, HfFileInfo, HfPortError, HfPortResult, HfQuantInfo, HfRepoInfo, HfSearchOptions,
    HfSearchResult,
};

use crate::client::HfClient;
use crate::error::HfError;
use crate::file_roles::to_file_info;
use crate::http::HttpBackend;
use crate::models::{HfQuantization, HfRepoRef};
use crate::parsing::repo_info_from_json;

// ============================================================================
// Error Mapping
// ============================================================================

/// Convert internal `HfError` to core `HfPortError`.
fn map_error(err: HfError) -> HfPortError {
    match err {
        HfError::ApiRequestFailed { status, url } => {
            if status == 404 {
                // Extract model ID from URL if possible
                let model_id = extract_model_id_from_url(&url);
                HfPortError::ModelNotFound { model_id }
            } else if status == 401 || status == 403 {
                let model_id = extract_model_id_from_url(&url);
                HfPortError::AuthRequired { model_id }
            } else if status == 429 {
                HfPortError::RateLimited
            } else {
                HfPortError::Network {
                    message: format!("API request failed with status {status}: {url}"),
                }
            }
        }
        HfError::InvalidResponse { message } => HfPortError::InvalidResponse { message },
        HfError::ModelNotFound { model_id } => HfPortError::ModelNotFound { model_id },
        HfError::QuantizationNotFound {
            model_id,
            quantization,
        } => HfPortError::QuantizationNotFound {
            model_id,
            quantization,
        },
        HfError::Network(e) => HfPortError::Network {
            message: e.to_string(),
        },
        HfError::InvalidUrl(e) => HfPortError::Configuration {
            message: e.to_string(),
        },
        HfError::JsonParse(e) => HfPortError::InvalidResponse {
            message: e.to_string(),
        },
    }
}

/// Extract model ID from a `HuggingFace` API URL.
fn extract_model_id_from_url(url: &str) -> String {
    // URLs look like: https://huggingface.co/api/models/TheBloke/Llama-2-7B-GGUF/...
    if let Some(models_pos) = url.find("/api/models/") {
        let after_models = &url[models_pos + 12..];
        // Take owner/name part (up to next / or end)
        let parts: Vec<&str> = after_models.splitn(3, '/').collect();
        if parts.len() >= 2 {
            return format!("{}/{}", parts[0], parts[1]);
        }
    }
    url.to_string()
}

// ============================================================================
// Type Conversions
// ============================================================================

/// `model_id` as a repository reference, or the error every method here
/// answers a malformed ID with.
fn parse_repo(model_id: &str) -> HfPortResult<HfRepoRef> {
    HfRepoRef::parse(model_id).ok_or_else(|| HfPortError::InvalidResponse {
        message: format!("Invalid model ID format: {model_id}"),
    })
}

/// Convert internal `HfQuantization` to core `HfQuantInfo`.
fn to_quant_info(quant: &HfQuantization) -> HfQuantInfo {
    HfQuantInfo {
        name: quant.name.clone(),
        shard_count: quant.shard_count,
        total_size: quant.total_size,
        file_paths: quant.paths.clone(),
    }
}

// ============================================================================
// Port Implementation
// ============================================================================

#[async_trait]
impl<B: HttpBackend + Send + Sync> HfClientPort for HfClient<B> {
    async fn search(&self, options: &HfSearchOptions) -> HfPortResult<HfSearchResult> {
        self.search_models_page(options).await.map_err(map_error)
    }

    async fn list_quantizations(&self, model_id: &str) -> HfPortResult<Vec<HfQuantInfo>> {
        let repo = parse_repo(model_id)?;

        let quants = self.list_quantizations(&repo).await.map_err(map_error)?;

        Ok(quants.iter().map(to_quant_info).collect())
    }

    async fn list_gguf_files(&self, model_id: &str) -> HfPortResult<Vec<HfFileInfo>> {
        let repo = parse_repo(model_id)?;

        let files = self.list_all_gguf_files(&repo).await.map_err(map_error)?;

        Ok(files
            .iter()
            .map(|f| HfFileInfo {
                path: f.path.clone(),
                size: f.size,
                is_gguf: f.is_gguf(),
                oid: None, // list_gguf_files doesn't populate OIDs
            })
            .collect())
    }

    async fn get_quantization_files(
        &self,
        model_id: &str,
        quantization: &str,
    ) -> HfPortResult<Vec<HfFileInfo>> {
        let repo = parse_repo(model_id)?;

        let files = self
            .find_quantization_files_with_sizes(&repo, quantization)
            .await
            .map_err(map_error)?;
        Ok(files.into_iter().map(to_file_info).collect())
    }

    async fn list_projectors(&self, model_id: &str) -> HfPortResult<Vec<HfFileInfo>> {
        let repo = parse_repo(model_id)?;

        let files = self.list_projectors(&repo).await.map_err(map_error)?;
        Ok(files.into_iter().map(to_file_info).collect())
    }

    async fn get_commit_sha(&self, model_id: &str) -> HfPortResult<String> {
        let repo = parse_repo(model_id)?;

        self.get_commit_sha(&repo).await.map_err(map_error)
    }

    /// Fetch `generation_config.json` from the `resolve` endpoint.
    ///
    /// A repo without one answers 404, which is the ordinary case for a GGUF
    /// quant repo and is mapped to `Ok(None)` rather than an error — the file
    /// being absent is not a failure to fetch it. Every other failure stays an
    /// `Err` so the caller can tell "no config published" from "could not
    /// look", even though today it treats both as a reason to fall back.
    async fn fetch_generation_config(&self, model_id: &str) -> HfPortResult<Option<String>> {
        parse_repo(model_id)?;

        let raw = crate::url::build_file_url(model_id, "generation_config.json", None);
        let url = url::Url::parse(&raw).map_err(|e| HfPortError::Configuration {
            message: e.to_string(),
        })?;

        // Deserialized as a `Value` and re-serialized rather than fetched as
        // text, because `HttpBackend` speaks JSON — which this file is. The
        // domain parser takes a string so it can also report "this was not
        // JSON at all", a case that cannot arise on this path.
        match self.backend.get_json::<serde_json::Value>(&url).await {
            Ok(value) => Ok(Some(value.to_string())),
            Err(HfError::ApiRequestFailed { status: 404, .. }) => Ok(None),
            Err(e) => Err(map_error(e)),
        }
    }

    async fn get_model_info(&self, model_id: &str) -> HfPortResult<HfRepoInfo> {
        let repo = parse_repo(model_id)?;

        let info = self.get_model_info(&repo).await.map_err(map_error)?;

        repo_info_from_json(&info).ok_or_else(|| HfPortError::InvalidResponse {
            message: format!("the model info for {model_id} names no id"),
        })
    }

    async fn read_head(&self, model_id: &str, path: &str, max_bytes: u64) -> HfPortResult<Vec<u8>> {
        let repo = parse_repo(model_id)?;

        self.read_head(&repo, path, max_bytes)
            .await
            .map_err(map_error)
    }

    async fn file_at(&self, model_id: &str, path: &str) -> HfPortResult<Option<HfFileInfo>> {
        let repo = parse_repo(model_id)?;

        let file = self.file_at(&repo, path).await.map_err(map_error)?;
        Ok(file.map(to_file_info))
    }
}

#[cfg(test)]
#[path = "recorded_tests.rs"]
mod recorded_tests;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_extract_model_id_from_url() {
        let url = "https://huggingface.co/api/models/TheBloke/Llama-2-7B-GGUF/tree/main";
        assert_eq!(extract_model_id_from_url(url), "TheBloke/Llama-2-7B-GGUF");

        let url = "https://huggingface.co/api/models/org/model";
        assert_eq!(extract_model_id_from_url(url), "org/model");
    }

    #[test]
    fn test_map_error_404() {
        let err = HfError::ApiRequestFailed {
            status: 404,
            url: "https://huggingface.co/api/models/Test/Model".to_string(),
        };
        match map_error(err) {
            HfPortError::ModelNotFound { model_id } => {
                assert_eq!(model_id, "Test/Model");
            }
            _ => panic!("Expected ModelNotFound"),
        }
    }

    #[test]
    fn test_map_error_401() {
        let err = HfError::ApiRequestFailed {
            status: 401,
            url: "https://huggingface.co/api/models/Private/Model".to_string(),
        };
        match map_error(err) {
            HfPortError::AuthRequired { model_id } => {
                assert_eq!(model_id, "Private/Model");
            }
            _ => panic!("Expected AuthRequired"),
        }
    }

    #[test]
    fn test_map_error_429() {
        let err = HfError::ApiRequestFailed {
            status: 429,
            url: "https://example.com".to_string(),
        };
        match map_error(err) {
            HfPortError::RateLimited => {}
            _ => panic!("Expected RateLimited"),
        }
    }
}
