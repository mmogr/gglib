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
use crate::http::HttpBackend;
use crate::models::{HfModelSummary, HfQuantization, HfRepoRef, HfSearchQuery, HfSortField};

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

/// Convert internal `HfModelSummary` to core `HfRepoInfo`.
fn to_repo_info(model: &HfModelSummary) -> HfRepoInfo {
    HfRepoInfo {
        model_id: model.id.clone(),
        name: model.name.clone(),
        author: model.author.clone(),
        downloads: model.downloads,
        likes: model.likes,
        parameters_b: model.parameters_b,
        description: model.description.clone(),
        last_modified: model.last_modified.clone(),
        chat_template: None, // Not available in search summary
        tags: model.tags.clone(),
    }
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

/// Convert core `HfSearchOptions` to internal `HfSearchQuery`.
fn to_search_query(options: &HfSearchOptions) -> HfSearchQuery {
    let sort_field = match options.sort_by.as_str() {
        "likes" => HfSortField::Likes,
        "modified" | "lastModified" => HfSortField::Modified,
        "created" | "createdAt" => HfSortField::Created,
        "id" | "alphabetical" => HfSortField::Alphabetical,
        _ => HfSortField::Downloads,
    };

    HfSearchQuery {
        query: options.query.clone(),
        min_params_b: options.min_params_b,
        max_params_b: options.max_params_b,
        limit: options.limit,
        page: options.page,
        sort_by: sort_field,
        sort_ascending: options.sort_ascending,
    }
}

// ============================================================================
// Port Implementation
// ============================================================================

#[async_trait]
impl<B: HttpBackend + Send + Sync> HfClientPort for HfClient<B> {
    async fn search(&self, options: &HfSearchOptions) -> HfPortResult<HfSearchResult> {
        let query = to_search_query(options);
        let response = self.search_models_page(&query).await.map_err(map_error)?;

        Ok(HfSearchResult {
            items: response.items.iter().map(to_repo_info).collect(),
            has_more: response.has_more,
            page: response.page,
        })
    }

    async fn list_quantizations(&self, model_id: &str) -> HfPortResult<Vec<HfQuantInfo>> {
        let repo = HfRepoRef::parse(model_id).ok_or_else(|| HfPortError::InvalidResponse {
            message: format!("Invalid model ID format: {model_id}"),
        })?;

        let quants = self.list_quantizations(&repo).await.map_err(map_error)?;

        Ok(quants.iter().map(to_quant_info).collect())
    }

    async fn list_gguf_files(&self, model_id: &str) -> HfPortResult<Vec<HfFileInfo>> {
        let repo = HfRepoRef::parse(model_id).ok_or_else(|| HfPortError::InvalidResponse {
            message: format!("Invalid model ID format: {model_id}"),
        })?;

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
        let repo = HfRepoRef::parse(model_id).ok_or_else(|| HfPortError::InvalidResponse {
            message: format!("Invalid model ID format: {model_id}"),
        })?;

        let files = self.find_quantization_files_with_sizes(&repo, quantization)
            .await
            .map_err(map_error)?;
        
        // Convert HfFileEntry to HfFileInfo with OID
        Ok(files.into_iter().map(|f| HfFileInfo {
            path: f.path,
            size: f.size,
            is_gguf: matches!(f.entry_type, crate::models::HfEntryType::File),
            oid: f.oid,
        }).collect())
    }

    async fn get_commit_sha(&self, model_id: &str) -> HfPortResult<String> {
        let repo = HfRepoRef::parse(model_id).ok_or_else(|| HfPortError::InvalidResponse {
            message: format!("Invalid model ID format: {model_id}"),
        })?;

        self.get_commit_sha(&repo).await.map_err(map_error)
    }

    async fn get_model_info(&self, model_id: &str) -> HfPortResult<HfRepoInfo> {
        let repo = HfRepoRef::parse(model_id).ok_or_else(|| HfPortError::InvalidResponse {
            message: format!("Invalid model ID format: {model_id}"),
        })?;

        // Fetch model info JSON
        let info = self.get_model_info(&repo).await.map_err(map_error)?;

        // Parse the JSON into a HfRepoInfo
        let model_id_str = info
            .get("id")
            .and_then(|v| v.as_str())
            .map_or_else(|| model_id.to_string(), String::from);

        let name = model_id_str
            .split('/')
            .next_back()
            .unwrap_or(&model_id_str)
            .to_string();

        let author = model_id_str.split('/').next().map(String::from);

        let downloads = info
            .get("downloads")
            .and_then(serde_json::Value::as_u64)
            .unwrap_or(0);

        let likes = info
            .get("likes")
            .and_then(serde_json::Value::as_u64)
            .unwrap_or(0);

        let parameters_b = info
            .get("safetensors")
            .and_then(|s| s.get("total"))
            .and_then(serde_json::Value::as_f64)
            .map(|p| p / 1_000_000_000.0)
            .or_else(|| {
                info.get("config")
                    .and_then(|c| c.get("num_parameters"))
                    .and_then(serde_json::Value::as_f64)
                    .map(|p| p / 1_000_000_000.0)
            });

        let description = info
            .get("cardData")
            .and_then(|c| c.get("model_summary"))
            .and_then(|v| v.as_str())
            .map(String::from);

        let last_modified = info
            .get("lastModified")
            .and_then(|v| v.as_str())
            .map(String::from);

        // Extract chat template - check gguf location first (for GGUF repos),
        // then fall back to config location (for safetensors repos)
        let chat_template = info
            .get("gguf")
            .and_then(|g| g.get("chat_template"))
            .and_then(|v| v.as_str())
            .map(String::from)
            .or_else(|| {
                info.get("config")
                    .and_then(|c| c.get("chat_template"))
                    .and_then(|v| v.as_str())
                    .map(String::from)
            });

        // Extract tags from model metadata
        let tags = info
            .get("tags")
            .and_then(|v| v.as_array())
            .map(|arr| {
                arr.iter()
                    .filter_map(|v| v.as_str().map(String::from))
                    .collect()
            })
            .unwrap_or_default();

        Ok(HfRepoInfo {
            model_id: model_id_str,
            name,
            author,
            downloads,
            likes,
            parameters_b,
            description,
            last_modified,
            chat_template,
            tags,
        })
    }
}

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

    #[test]
    fn test_to_search_query() {
        let options = HfSearchOptions {
            query: Some("llama".to_string()),
            limit: 50,
            page: 2,
            sort_by: "likes".to_string(),
            sort_ascending: true,
            min_params_b: Some(7.0),
            max_params_b: Some(13.0),
        };

        let query = to_search_query(&options);
        assert_eq!(query.query, Some("llama".to_string()));
        assert_eq!(query.limit, 50);
        assert_eq!(query.page, 2);
        assert_eq!(query.sort_by, HfSortField::Likes);
        assert!(query.sort_ascending);
        assert_eq!(query.min_params_b, Some(7.0));
        assert_eq!(query.max_params_b, Some(13.0));
    }
}
