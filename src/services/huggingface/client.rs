//! HuggingFace client for searching models and fetching metadata.
//!
//! This module provides the main client interface for interacting with
//! the HuggingFace Hub API. The client is generic over an HTTP backend,
//! allowing for easy testing with fake implementations.
//!
//! # Example
//!
//! ```rust,no_run
//! use gglib::services::huggingface::{DefaultHuggingfaceClient, HfConfig, HfSearchQuery};
//!
//! # async fn example() -> anyhow::Result<()> {
//! let client = DefaultHuggingfaceClient::new(HfConfig::default());
//!
//! // Search for models
//! let query = HfSearchQuery::new().with_query("llama");
//! let response = client.search_models_page(&query).await?;
//! for model in response.items {
//!     println!("{}: {} downloads", model.id, model.downloads);
//! }
//!
//! // Get quantizations for a specific model
//! let repo = "TheBloke/Llama-2-7B-GGUF".parse().unwrap();
//! let quants = client.list_quantizations(&repo).await?;
//! for q in quants {
//!     println!("{}: {:.1} MB", q.name, q.size_mb());
//! }
//! # Ok(())
//! # }
//! ```

use super::error::{HfError, HfResult};
use super::http_backend::{HttpBackend, ReqwestBackend};
use super::models::{
    HfConfig, HfFileEntry, HfModelSummary, HfQuantization, HfRepoRef, HfSearchQuery,
    HfSearchResponse, HfToolSupportResponse,
};
use super::parsing::{
    aggregate_quantizations, filter_files_by_quantization, parse_search_response,
    parse_tree_entries,
};
use super::url_builder::{build_model_info_url, build_search_url, build_tree_url};
use crate::utils::gguf_parser::detect_tool_support;
use std::collections::HashMap;

// ============================================================================
// Type Aliases
// ============================================================================

/// Default HuggingFace client using the reqwest HTTP backend.
pub type DefaultHuggingfaceClient = HuggingfaceClient<ReqwestBackend>;

// ============================================================================
// Client
// ============================================================================

/// Client for interacting with the HuggingFace Hub API.
///
/// This client is generic over an HTTP backend, allowing for easy testing.
/// Use `DefaultHuggingfaceClient` for production code, or inject a fake
/// backend for testing.
pub struct HuggingfaceClient<B: HttpBackend> {
    backend: B,
    config: HfConfig,
}

impl DefaultHuggingfaceClient {
    /// Create a new client with the default reqwest backend.
    ///
    /// # Arguments
    ///
    /// * `config` - Client configuration
    pub fn new(config: HfConfig) -> Self {
        let backend = ReqwestBackend::new(&config);
        Self { backend, config }
    }

    /// Create a new client with default configuration.
    pub fn default_client() -> Self {
        Self::new(HfConfig::default())
    }
}

impl<B: HttpBackend> HuggingfaceClient<B> {
    /// Create a new client with a custom backend.
    ///
    /// Use this for testing with a fake backend.
    ///
    /// # Arguments
    ///
    /// * `config` - Client configuration
    /// * `backend` - HTTP backend implementation
    pub fn with_backend(config: HfConfig, backend: B) -> Self {
        Self { backend, config }
    }

    /// Get the client configuration.
    pub fn config(&self) -> &HfConfig {
        &self.config
    }

    // ========================================================================
    // Search Methods
    // ========================================================================

    /// Search for models with pagination.
    ///
    /// Returns a single page of results with pagination info.
    ///
    /// # Arguments
    ///
    /// * `query` - Search query parameters
    ///
    /// # Returns
    ///
    /// Returns a paginated response with matching models.
    pub async fn search_models_page(&self, query: &HfSearchQuery) -> HfResult<HfSearchResponse> {
        // Fetch more models than requested since we filter out models without GGUF files
        let fetch_query = HfSearchQuery {
            limit: 100, // Fetch more to ensure we get enough after filtering
            ..query.clone()
        };

        let url = build_search_url(&self.config, &fetch_query);
        let (json_array, has_more): (Vec<serde_json::Value>, bool) =
            self.backend.get_json_paginated(&url).await?;

        let mut response = parse_search_response(&json_array, has_more, query.page);

        // Apply parameter filtering (client-side)
        response.items = response
            .items
            .into_iter()
            .filter(|model| {
                // Min params filter
                if let Some(min) = query.min_params_b {
                    match model.parameters_b {
                        Some(params) if params >= min => {}
                        _ => return false,
                    }
                }

                // Max params filter
                if let Some(max) = query.max_params_b {
                    match model.parameters_b {
                        Some(params) if params <= max => {}
                        _ => return false,
                    }
                }

                true
            })
            .take(query.limit as usize)
            .collect();

        Ok(response)
    }

    /// Search for all models matching a query.
    ///
    /// Fetches all pages and returns a combined list.
    /// Use with caution for broad queries as this may make many API calls.
    ///
    /// # Arguments
    ///
    /// * `query` - Search query parameters (page is ignored)
    ///
    /// # Returns
    ///
    /// Returns all matching models.
    pub async fn search_all_models(&self, query: HfSearchQuery) -> HfResult<Vec<HfModelSummary>> {
        let mut results = Vec::new();
        let mut current_query = HfSearchQuery { page: 0, ..query };

        loop {
            let response = self.search_models_page(&current_query).await?;
            results.extend(response.items);

            if !response.has_more {
                break;
            }

            current_query.page += 1;

            // Safety limit to prevent infinite loops
            if current_query.page > 100 {
                break;
            }
        }

        Ok(results)
    }

    /// Search for models (simple interface for CLI).
    ///
    /// Returns raw JSON values for flexible output formatting.
    ///
    /// # Arguments
    ///
    /// * `search_query` - Search query string
    /// * `limit` - Maximum number of results
    /// * `sort` - Sort field (downloads, likes, created, trending)
    ///
    /// # Returns
    ///
    /// Returns a vector of raw JSON model objects.
    pub async fn search_models_raw(
        &self,
        search_query: &str,
        limit: u32,
        sort: &str,
    ) -> HfResult<Vec<serde_json::Value>> {
        use super::models::HfSortField;

        let sort_field = match sort {
            "likes" => HfSortField::Likes,
            "created" | "createdAt" => HfSortField::Created,
            "modified" | "lastModified" => HfSortField::Modified,
            "id" | "alphabetical" => HfSortField::Alphabetical,
            _ => HfSortField::Downloads,
        };

        let query = HfSearchQuery::new()
            .with_query(search_query)
            .with_limit(limit)
            .with_sort(sort_field, false);

        let url = build_search_url(&self.config, &query);
        let result: Vec<serde_json::Value> = self.backend.get_json(&url).await?;
        Ok(result)
    }

    // ========================================================================
    // File Tree Methods
    // ========================================================================

    /// List files in a model repository.
    ///
    /// # Arguments
    ///
    /// * `repo` - Repository reference
    /// * `path` - Optional subdirectory path
    ///
    /// # Returns
    ///
    /// Returns a list of file entries.
    pub async fn list_model_files(
        &self,
        repo: &HfRepoRef,
        path: Option<&str>,
    ) -> HfResult<Vec<HfFileEntry>> {
        let url = build_tree_url(&self.config, repo, path);
        let json: serde_json::Value = self.backend.get_json(&url).await?;
        parse_tree_entries(&json)
    }

    /// List all GGUF files in a repository (including subdirectories).
    ///
    /// This recursively scans subdirectories to find all GGUF files,
    /// which is necessary for repositories that organize files by quantization.
    ///
    /// # Arguments
    ///
    /// * `repo` - Repository reference
    ///
    /// # Returns
    ///
    /// Returns all GGUF file entries from the repository.
    pub async fn list_all_gguf_files(&self, repo: &HfRepoRef) -> HfResult<Vec<HfFileEntry>> {
        let mut all_files = Vec::new();

        // Get root files
        let root_files = self.list_model_files(repo, None).await?;

        for file in &root_files {
            if file.is_gguf() {
                all_files.push(file.clone());
            } else if file.is_directory() {
                // Check subdirectory for GGUF files
                if let Ok(sub_files) = self.list_model_files(repo, Some(&file.path)).await {
                    for sub_file in sub_files {
                        if sub_file.is_gguf() {
                            all_files.push(sub_file);
                        }
                    }
                }
            }
        }

        Ok(all_files)
    }

    // ========================================================================
    // Quantization Methods
    // ========================================================================

    /// List available quantizations for a model.
    ///
    /// Scans the repository for GGUF files and groups them by quantization type.
    ///
    /// # Arguments
    ///
    /// * `repo` - Repository reference
    ///
    /// # Returns
    ///
    /// Returns a list of available quantizations with file info.
    pub async fn list_quantizations(&self, repo: &HfRepoRef) -> HfResult<Vec<HfQuantization>> {
        let files = self.list_all_gguf_files(repo).await?;
        Ok(aggregate_quantizations(&files))
    }

    /// Find GGUF files for a specific quantization.
    ///
    /// # Arguments
    ///
    /// * `repo` - Repository reference
    /// * `quantization` - Quantization name (e.g., "Q4_K_M")
    ///
    /// # Returns
    ///
    /// Returns file entries matching the quantization, or an error if not found.
    pub async fn find_quantization_files(
        &self,
        repo: &HfRepoRef,
        quantization: &str,
    ) -> HfResult<Vec<HfFileEntry>> {
        let files = self.list_all_gguf_files(repo).await?;
        let matching = filter_files_by_quantization(&files, quantization);

        if matching.is_empty() {
            return Err(HfError::QuantizationNotFound {
                model_id: repo.id(),
                quantization: quantization.to_string(),
            });
        }

        Ok(matching)
    }

    /// Find GGUF files for a specific quantization, returning (path, size) tuples.
    ///
    /// This is a convenience method that returns the format commonly needed
    /// for download operations.
    ///
    /// # Arguments
    ///
    /// * `repo` - Repository reference
    /// * `quantization` - Quantization name (e.g., "Q4_K_M")
    ///
    /// # Returns
    ///
    /// Returns a sorted vector of (path, size_bytes) tuples for matching files.
    pub async fn find_quantization_files_with_sizes(
        &self,
        repo: &HfRepoRef,
        quantization: &str,
    ) -> HfResult<Vec<(String, u64)>> {
        let files = self.find_quantization_files(repo, quantization).await?;
        let mut result: Vec<(String, u64)> = files.into_iter().map(|f| (f.path, f.size)).collect();

        // Sort by path to ensure correct shard order
        result.sort_by(|a, b| a.0.cmp(&b.0));

        Ok(result)
    }

    // ========================================================================
    // Model Info Methods
    // ========================================================================

    /// Fetch model info (commit SHA, metadata, etc.).
    ///
    /// # Arguments
    ///
    /// * `repo` - Repository reference
    ///
    /// # Returns
    ///
    /// Returns raw JSON model info.
    pub async fn get_model_info(&self, repo: &HfRepoRef) -> HfResult<serde_json::Value> {
        let url = build_model_info_url(&self.config, repo);
        self.backend.get_json(&url).await
    }

    /// Get the commit SHA for a model repository.
    ///
    /// # Arguments
    ///
    /// * `repo` - Repository reference
    ///
    /// # Returns
    ///
    /// Returns the commit SHA string, or "main" if not found.
    pub async fn get_commit_sha(&self, repo: &HfRepoRef) -> HfResult<String> {
        let info = self.get_model_info(repo).await?;
        Ok(info
            .get("sha")
            .and_then(|v| v.as_str())
            .unwrap_or("main")
            .to_string())
    }

    /// Check if a model supports tool/function calling.
    ///
    /// Fetches the model's GGUF metadata from the API and analyzes
    /// the chat template for tool support.
    ///
    /// # Arguments
    ///
    /// * `repo` - Repository reference
    ///
    /// # Returns
    ///
    /// Returns tool support detection results.
    pub async fn get_tool_support(&self, repo: &HfRepoRef) -> HfResult<HfToolSupportResponse> {
        let model_info = self.get_model_info(repo).await?;

        // Extract chat_template from gguf metadata
        let chat_template = model_info
            .get("gguf")
            .and_then(|gguf| gguf.get("chat_template"))
            .and_then(|v| v.as_str())
            .map(|s| s.to_string());

        // Build metadata HashMap for detect_tool_support
        let mut metadata = HashMap::new();
        if let Some(template) = chat_template {
            metadata.insert("tokenizer.chat_template".to_string(), template);
        }

        // Add model name for name-based detection fallback
        if let Some(name) = model_info.get("id").and_then(|v| v.as_str()) {
            metadata.insert("general.name".to_string(), name.to_string());
        }

        // Use unified detection logic
        let detection = detect_tool_support(&metadata);

        Ok(HfToolSupportResponse {
            supports_tool_calling: detection.supports_tool_calling,
            confidence: detection.confidence,
            detected_format: detection.detected_format,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::super::http_backend::testing::{CannedResponse, FakeBackend};
    use super::*;
    use serde_json::json;

    fn test_config() -> HfConfig {
        HfConfig::default()
    }

    fn fake_model_json(id: &str, downloads: u64) -> serde_json::Value {
        json!({
            "id": id,
            "downloads": downloads,
            "likes": 10,
            "siblings": [{"rfilename": "model.gguf"}]
        })
    }

    #[tokio::test]
    async fn test_search_models_page() {
        let backend = FakeBackend::new().with_response(
            "huggingface.co",
            CannedResponse {
                json: json!([
                    fake_model_json("Org/Model1-GGUF", 1000),
                    fake_model_json("Org/Model2-GGUF", 2000),
                ]),
                has_more: true,
            },
        );

        let client = HuggingfaceClient::with_backend(test_config(), backend);
        let query = HfSearchQuery::new().with_query("llama");

        let response = client.search_models_page(&query).await.unwrap();

        assert_eq!(response.items.len(), 2);
        assert!(response.has_more);
        assert_eq!(response.items[0].id, "Org/Model1-GGUF");
    }

    #[tokio::test]
    async fn test_search_models_page_filters_by_params() {
        let backend = FakeBackend::new().with_response(
            "huggingface.co",
            CannedResponse {
                json: json!([
                    {
                        "id": "Org/Small-GGUF",
                        "downloads": 1000,
                        "siblings": [{"rfilename": "model.gguf"}],
                        "gguf": {"total": 1_000_000_000_u64}  // 1B params
                    },
                    {
                        "id": "Org/Large-GGUF",
                        "downloads": 2000,
                        "siblings": [{"rfilename": "model.gguf"}],
                        "gguf": {"total": 70_000_000_000_u64}  // 70B params
                    },
                ]),
                has_more: false,
            },
        );

        let client = HuggingfaceClient::with_backend(test_config(), backend);

        // Filter for models between 5B and 100B params
        let query = HfSearchQuery::new().with_params_filter(Some(5.0), Some(100.0));

        let response = client.search_models_page(&query).await.unwrap();

        assert_eq!(response.items.len(), 1);
        assert_eq!(response.items[0].id, "Org/Large-GGUF");
    }

    #[tokio::test]
    async fn test_list_model_files() {
        let backend = FakeBackend::new().with_response(
            "tree/main",
            CannedResponse {
                json: json!([
                    {"path": "README.md", "type": "file", "size": 1000},
                    {"path": "model.Q4_K_M.gguf", "type": "file", "size": 4000000000_u64},
                    {"path": "Q8_0", "type": "directory", "size": 0}
                ]),
                has_more: false,
            },
        );

        let client = HuggingfaceClient::with_backend(test_config(), backend);
        let repo = HfRepoRef::new("TheBloke", "Llama-2-7B-GGUF");

        let files = client.list_model_files(&repo, None).await.unwrap();

        assert_eq!(files.len(), 3);
        assert!(files[1].is_gguf());
        assert!(files[2].is_directory());
    }

    #[tokio::test]
    async fn test_list_quantizations() {
        let backend = FakeBackend::new().with_response(
            "tree/main",
            CannedResponse {
                json: json!([
                    {"path": "model-Q4_K_M.gguf", "type": "file", "size": 4000000000_u64},
                    {"path": "model-Q8_0.gguf", "type": "file", "size": 8000000000_u64},
                ]),
                has_more: false,
            },
        );

        let client = HuggingfaceClient::with_backend(test_config(), backend);
        let repo = HfRepoRef::new("TheBloke", "Llama-2-7B-GGUF");

        let quants = client.list_quantizations(&repo).await.unwrap();

        assert_eq!(quants.len(), 2);
        // Sorted alphabetically
        assert_eq!(quants[0].name, "Q4_K_M");
        assert_eq!(quants[1].name, "Q8_0");
    }

    #[tokio::test]
    async fn test_find_quantization_files() {
        let backend = FakeBackend::new().with_response(
            "tree/main",
            CannedResponse {
                json: json!([
                    {"path": "model-Q4_K_M.gguf", "type": "file", "size": 4000000000_u64},
                    {"path": "model-Q8_0.gguf", "type": "file", "size": 8000000000_u64},
                ]),
                has_more: false,
            },
        );

        let client = HuggingfaceClient::with_backend(test_config(), backend);
        let repo = HfRepoRef::new("TheBloke", "Llama-2-7B-GGUF");

        let files = client
            .find_quantization_files(&repo, "Q4_K_M")
            .await
            .unwrap();

        assert_eq!(files.len(), 1);
        assert_eq!(files[0].path, "model-Q4_K_M.gguf");
    }

    #[tokio::test]
    async fn test_find_quantization_files_not_found() {
        let backend = FakeBackend::new().with_response(
            "tree/main",
            CannedResponse {
                json: json!([
                    {"path": "model-Q4_K_M.gguf", "type": "file", "size": 4000000000_u64},
                ]),
                has_more: false,
            },
        );

        let client = HuggingfaceClient::with_backend(test_config(), backend);
        let repo = HfRepoRef::new("TheBloke", "Llama-2-7B-GGUF");

        let result = client.find_quantization_files(&repo, "Q99_Z").await;

        assert!(matches!(result, Err(HfError::QuantizationNotFound { .. })));
    }

    #[tokio::test]
    async fn test_get_commit_sha() {
        let backend = FakeBackend::new().with_response(
            "Llama-2-7B-GGUF",
            CannedResponse {
                json: json!({
                    "id": "TheBloke/Llama-2-7B-GGUF",
                    "sha": "abc123def456"
                }),
                has_more: false,
            },
        );

        let client = HuggingfaceClient::with_backend(test_config(), backend);
        let repo = HfRepoRef::new("TheBloke", "Llama-2-7B-GGUF");

        let sha = client.get_commit_sha(&repo).await.unwrap();

        assert_eq!(sha, "abc123def456");
    }

    #[tokio::test]
    async fn test_get_commit_sha_missing_defaults_to_main() {
        let backend = FakeBackend::new().with_response(
            "Llama-2-7B-GGUF",
            CannedResponse {
                json: json!({"id": "TheBloke/Llama-2-7B-GGUF"}),
                has_more: false,
            },
        );

        let client = HuggingfaceClient::with_backend(test_config(), backend);
        let repo = HfRepoRef::new("TheBloke", "Llama-2-7B-GGUF");

        let sha = client.get_commit_sha(&repo).await.unwrap();

        assert_eq!(sha, "main");
    }
}
