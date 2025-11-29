//! HuggingFace API service for searching and browsing GGUF models.
//!
//! This service provides centralized access to the HuggingFace Hub API,
//! ensuring consistent URL construction, expand parameters, and response
//! parsing across both GUI and CLI interfaces.
//!
//! # Architecture
//!
//! The service follows the pattern established by other core services:
//! - Stateless struct that can be cloned and shared
//! - Pure functions without interactive prompts

// Allow collapsible_if because let chains (`if let ... && let ...`) are unstable
// in the CI's Rust version (1.86). Once let chains are stabilized, we can collapse
// these nested if statements.
#![allow(clippy::collapsible_if)]
//! - Returns domain types from `models/gui.rs`
//!
//! # Example
//!
//! ```rust,no_run
//! use gglib::services::core::HuggingFaceService;
//! use gglib::models::gui::HfSearchRequest;
//!
//! # async fn example() -> anyhow::Result<()> {
//! let service = HuggingFaceService::new();
//!
//! // Search for models (paginated, for GUI)
//! let request = HfSearchRequest {
//!     query: Some("llama".to_string()),
//!     ..Default::default()
//! };
//! let response = service.search_models_paginated(request).await?;
//! for model in response.models {
//!     println!("{}: {} likes", model.id, model.likes);
//! }
//!
//! // Get quantizations for a specific model
//! let quants = service.get_quantizations("TheBloke/Llama-2-7B-GGUF").await?;
//! for q in quants.quantizations {
//!     println!("{}: {:.1} MB", q.name, q.size_mb);
//! }
//! # Ok(())
//! # }
//! ```

use super::huggingface_models::HuggingFaceError;
use crate::commands::download::extract_quantization_from_filename;
use crate::models::gui::{
    HfModelSummary, HfQuantization, HfQuantizationsResponse, HfSearchRequest, HfSearchResponse,
    HfSortField,
};
use anyhow::Result;
use std::collections::HashMap;

/// Base URL for HuggingFace API.
///
/// Exposed publicly so other modules can use this constant instead of
/// hardcoding the URL (DRY principle).
pub const HF_API_BASE: &str = "https://huggingface.co/api/models";

/// Fields to explicitly expand in API requests.
///
/// When using `expand[]` parameters, HuggingFace API only returns
/// explicitly requested fields. We request all fields we need to
/// ensure consistent data across all API calls.
///
/// - `siblings`: File list for filtering models with actual GGUF files
/// - `gguf`: Parameter count (gguf.total)
/// - `likes`: Like count (NOT returned by default with expand params!)
/// - `downloads`: Download count (returned by default, but explicit for safety)
const EXPAND_FIELDS: &[&str] = &["siblings", "gguf", "likes", "downloads"];

/// Service for interacting with the HuggingFace Hub API.
///
/// Provides methods for searching models, fetching quantization info,
/// and other HuggingFace-related operations.
#[derive(Clone, Default)]
pub struct HuggingFaceService;

impl HuggingFaceService {
    /// Create a new HuggingFaceService instance.
    pub fn new() -> Self {
        Self
    }

    /// Build the expand parameters string for API URLs.
    ///
    /// Returns a string like `&expand[]=siblings&expand[]=gguf&expand[]=likes&expand[]=downloads`
    fn build_expand_params() -> String {
        EXPAND_FIELDS
            .iter()
            .map(|field| format!("expand[]={}", field))
            .collect::<Vec<_>>()
            .join("&")
    }

    /// Build a search URL with all required expand parameters.
    ///
    /// This is the core URL builder that ensures consistent API calls
    /// across all methods, preventing issues like missing likes data.
    fn build_search_url(
        fetch_limit: u32,
        page: u32,
        sort_by: &HfSortField,
        sort_ascending: bool,
    ) -> String {
        let direction = if sort_ascending { "1" } else { "-1" };
        format!(
            "{}?library=gguf&pipeline_tag=text-generation&{}&sort={}&direction={}&limit={}&p={}",
            HF_API_BASE,
            Self::build_expand_params(),
            sort_by.as_api_param(),
            direction,
            fetch_limit,
            page
        )
    }

    /// Parse a single model JSON object into an HfModelSummary.
    ///
    /// Returns None if the model doesn't have the required fields or
    /// doesn't contain actual GGUF files.
    fn parse_model_summary(model_json: &serde_json::Value) -> Option<HfModelSummary> {
        // Check if the model actually contains .gguf files
        let has_gguf_files = model_json
            .get("siblings")
            .and_then(|s| s.as_array())
            .map(|siblings| {
                siblings.iter().any(|file| {
                    file.get("rfilename")
                        .and_then(|f| f.as_str())
                        .map(|name| name.ends_with(".gguf"))
                        .unwrap_or(false)
                })
            })
            .unwrap_or(false);

        if !has_gguf_files {
            return None;
        }

        let id = model_json.get("id").and_then(|v| v.as_str())?.to_string();

        if id.is_empty() {
            return None;
        }

        // Extract author from id (format: "author/model-name")
        let author = id.split('/').next().map(|s| s.to_string());

        // Extract model name (last part of id)
        let name = id.split('/').next_back().unwrap_or(&id).to_string();

        // Extract parameter count from gguf.total
        let parameters_b = model_json
            .get("gguf")
            .and_then(|s| s.get("total"))
            .and_then(|t| t.as_u64())
            .map(|params| params as f64 / 1_000_000_000.0);

        let downloads = model_json
            .get("downloads")
            .and_then(|v| v.as_u64())
            .unwrap_or(0);

        let likes = model_json
            .get("likes")
            .and_then(|v| v.as_u64())
            .unwrap_or(0);

        let last_modified = model_json
            .get("lastModified")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string());

        let description = model_json
            .get("description")
            .and_then(|v| v.as_str())
            .map(|s| {
                // Truncate long descriptions
                if s.len() > 200 {
                    format!("{}...", &s[..197])
                } else {
                    s.to_string()
                }
            });

        let tags = model_json
            .get("tags")
            .and_then(|v| v.as_array())
            .map(|arr| {
                arr.iter()
                    .filter_map(|t| t.as_str().map(|s| s.to_string()))
                    .collect()
            })
            .unwrap_or_default();

        Some(HfModelSummary {
            id,
            name,
            author,
            downloads,
            likes,
            last_modified,
            parameters_b,
            description,
            tags,
        })
    }

    /// Search HuggingFace models with pagination and parameter filtering.
    ///
    /// This is the primary method for the GUI browser, returning structured
    /// results with pagination support.
    ///
    /// # Arguments
    ///
    /// * `request` - Search parameters including query, parameter filters, and pagination
    ///
    /// # Returns
    ///
    /// Returns a paginated response with matching models.
    pub async fn search_models_paginated(
        &self,
        request: HfSearchRequest,
    ) -> Result<HfSearchResponse> {
        // Fetch more models than requested since we filter out models without actual GGUF files.
        // The library=gguf filter returns models TAGGED with GGUF, but many are base models
        // that don't contain GGUF files themselves (only their derivatives do).
        let fetch_limit = 100;

        let mut url = Self::build_search_url(
            fetch_limit,
            request.page,
            &request.sort_by,
            request.sort_ascending,
        );

        // CRITICAL: Always add "GGUF" to the search to filter for repos that actually contain
        // GGUF files. The library=gguf tag returns base models like "meta-llama/Llama-3.1-8B"
        // that are tagged with GGUF because derivatives exist, but don't contain GGUF files.
        let search_query = match &request.query {
            Some(q) if !q.trim().is_empty() => {
                if q.to_lowercase().contains("gguf") {
                    q.trim().to_string()
                } else {
                    format!("{} GGUF", q.trim())
                }
            }
            _ => "GGUF".to_string(),
        };
        url.push_str(&format!("&search={}", urlencoding::encode(&search_query)));

        let response = reqwest::get(&url).await?;

        if !response.status().is_success() {
            return Err(HuggingFaceError::ApiRequestFailed {
                status: response.status().as_u16(),
                url: url.clone(),
            }
            .into());
        }

        // Check for pagination via Link header
        let has_more = response
            .headers()
            .get("Link")
            .and_then(|h| h.to_str().ok())
            .map(|link| link.contains("rel=\"next\""))
            .unwrap_or(false);

        let models_json: Vec<serde_json::Value> = response.json().await?;

        // Parse and filter models
        let mut models: Vec<HfModelSummary> = Vec::new();

        for model_json in models_json {
            let Some(model) = Self::parse_model_summary(&model_json) else {
                continue;
            };

            // Apply parameter filtering (client-side)
            if let Some(min) = request.min_params_b {
                match model.parameters_b {
                    Some(params) if params >= min => {}
                    _ => continue,
                }
            }

            if let Some(max) = request.max_params_b {
                match model.parameters_b {
                    Some(params) if params <= max => {}
                    _ => continue,
                }
            }

            // Limit results to requested amount
            if models.len() >= request.limit as usize {
                break;
            }

            models.push(model);
        }

        Ok(HfSearchResponse {
            models,
            has_more,
            page: request.page,
            total_count: None, // HuggingFace API doesn't provide total count
        })
    }

    /// Search HuggingFace models with simple parameters (for CLI).
    ///
    /// Returns raw JSON values for flexible CLI output formatting.
    ///
    /// # Arguments
    ///
    /// * `query` - Search query string
    /// * `limit` - Maximum number of results
    /// * `sort` - Sort field (downloads, likes, created, trending)
    ///
    /// # Returns
    ///
    /// Returns a vector of raw JSON model objects.
    pub async fn search_models(
        &self,
        query: &str,
        limit: u32,
        sort: &str,
    ) -> Result<Vec<serde_json::Value>> {
        let url = format!(
            "{}?search={}&limit={}&sort={}&direction=-1",
            HF_API_BASE,
            urlencoding::encode(query),
            limit,
            sort
        );

        let response = reqwest::get(&url).await?;

        if !response.status().is_success() {
            return Err(HuggingFaceError::ApiRequestFailed {
                status: response.status().as_u16(),
                url: url.clone(),
            }
            .into());
        }

        let models: Vec<serde_json::Value> = response.json().await?;
        Ok(models)
    }

    /// Get available quantizations for a model with detailed information.
    ///
    /// Returns structured quantization data including file sizes and shard info,
    /// suitable for GUI display.
    ///
    /// # Arguments
    ///
    /// * `model_id` - HuggingFace model ID (e.g., "TheBloke/Llama-2-7B-GGUF")
    ///
    /// # Returns
    ///
    /// Returns detailed quantization information for each variant.
    pub async fn get_quantizations(&self, model_id: &str) -> Result<HfQuantizationsResponse> {
        let api_url = format!("{}/{}/tree/main", HF_API_BASE, model_id);
        let mut quantizations: Vec<HfQuantization> = Vec::new();

        let response = reqwest::get(&api_url).await?;

        if !response.status().is_success() {
            return Err(HuggingFaceError::ApiRequestFailed {
                status: response.status().as_u16(),
                url: api_url,
            }
            .into());
        }

        let data: serde_json::Value = response.json().await?;

        if let Some(files) = data.as_array() {
            let mut quant_map: HashMap<String, HfQuantization> = HashMap::new();

            // 1) Direct GGUF files at repo root
            for file in files {
                if let (Some(path), Some(size)) = (
                    file.get("path").and_then(|v| v.as_str()),
                    file.get("size").and_then(|v| v.as_u64()),
                ) {
                    let entry_type = file.get("type").and_then(|v| v.as_str()).unwrap_or("file");

                    if entry_type == "file" && path.ends_with(".gguf") {
                        let quant_name = extract_quantization_from_filename(path).to_string();
                        if quant_name != "unknown" {
                            let size_mb = size as f64 / 1_048_576.0;
                            let is_shard =
                                path.contains("-00001-of-") || path.contains("-00002-of-");

                            if let Some(existing) = quant_map.get_mut(&quant_name) {
                                existing.size_bytes += size;
                                existing.size_mb += size_mb;
                                if let Some(ref mut count) = existing.shard_count {
                                    *count += 1;
                                }
                            } else {
                                quant_map.insert(
                                    quant_name.clone(),
                                    HfQuantization {
                                        name: quant_name,
                                        file_path: path.to_string(),
                                        size_bytes: size,
                                        size_mb,
                                        is_sharded: is_shard,
                                        shard_count: if is_shard { Some(1) } else { None },
                                    },
                                );
                            }
                        }
                    }
                }
            }

            // 2) Check subdirectories for sharded GGUF files
            for file in files {
                if let Some(dir_path) = file.get("path").and_then(|v| v.as_str()) {
                    let entry_type = file.get("type").and_then(|v| v.as_str()).unwrap_or("file");

                    if entry_type == "directory" {
                        let sub_api_url =
                            format!("{}/{}/tree/main/{}", HF_API_BASE, model_id, dir_path);

                        if let Ok(sub_response) = reqwest::get(&sub_api_url).await {
                            if sub_response.status().is_success() {
                                if let Ok(sub_data) = sub_response.json::<serde_json::Value>().await
                                {
                                    if let Some(sub_files) = sub_data.as_array() {
                                        let mut dir_total_size: u64 = 0;
                                        let mut dir_shard_count: u32 = 0;
                                        let mut dir_quant_name: Option<String> = None;
                                        let mut dir_first_file: Option<String> = None;

                                        for sub_file in sub_files {
                                            if let (Some(sub_path), Some(sub_size)) = (
                                                sub_file.get("path").and_then(|v| v.as_str()),
                                                sub_file.get("size").and_then(|v| v.as_u64()),
                                            ) {
                                                if sub_path.ends_with(".gguf") {
                                                    dir_total_size += sub_size;
                                                    dir_shard_count += 1;

                                                    if dir_quant_name.is_none() {
                                                        dir_quant_name = Some(
                                                            extract_quantization_from_filename(
                                                                sub_path,
                                                            )
                                                            .to_string(),
                                                        );
                                                        dir_first_file = Some(sub_path.to_string());
                                                    }
                                                }
                                            }
                                        }

                                        if let (Some(quant_name), Some(first_file)) =
                                            (dir_quant_name, dir_first_file)
                                        {
                                            if quant_name != "unknown"
                                                && !quant_map.contains_key(&quant_name)
                                            {
                                                quant_map.insert(
                                                    quant_name.clone(),
                                                    HfQuantization {
                                                        name: quant_name,
                                                        file_path: first_file,
                                                        size_bytes: dir_total_size,
                                                        size_mb: dir_total_size as f64
                                                            / 1_048_576.0,
                                                        is_sharded: dir_shard_count > 1,
                                                        shard_count: if dir_shard_count > 1 {
                                                            Some(dir_shard_count)
                                                        } else {
                                                            None
                                                        },
                                                    },
                                                );
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }

            quantizations = quant_map.into_values().collect();
            quantizations.sort_by(|a, b| a.name.cmp(&b.name));
        }

        Ok(HfQuantizationsResponse {
            model_id: model_id.to_string(),
            quantizations,
        })
    }

    /// Get available quantization names for a model (simple list for CLI).
    ///
    /// # Arguments
    ///
    /// * `model_id` - HuggingFace model ID
    ///
    /// # Returns
    ///
    /// Returns a sorted list of quantization names (e.g., ["Q4_K_M", "Q5_K_M", "Q8_0"]).
    pub async fn get_quantization_names(&self, model_id: &str) -> Result<Vec<String>> {
        let response = self.get_quantizations(model_id).await?;
        Ok(response.quantizations.into_iter().map(|q| q.name).collect())
    }

    // =========================================================================
    // Low-level API Helpers (for use by other modules to avoid DRY violations)
    // =========================================================================

    /// Build a URL for the model tree endpoint.
    ///
    /// Use this instead of hardcoding URLs to maintain consistency.
    ///
    /// # Arguments
    ///
    /// * `model_id` - HuggingFace model ID (e.g., "TheBloke/Llama-2-7B-GGUF")
    /// * `path` - Optional subdirectory path within the repo
    ///
    /// # Returns
    ///
    /// Returns the full API URL for the tree endpoint.
    pub fn build_tree_url(model_id: &str, path: Option<&str>) -> String {
        match path {
            Some(p) => format!("{}/{}/tree/main/{}", HF_API_BASE, model_id, p),
            None => format!("{}/{}/tree/main", HF_API_BASE, model_id),
        }
    }

    /// Build a URL for the model info endpoint.
    ///
    /// Use this instead of hardcoding URLs to maintain consistency.
    ///
    /// # Arguments
    ///
    /// * `model_id` - HuggingFace model ID
    ///
    /// # Returns
    ///
    /// Returns the full API URL for the model info endpoint.
    pub fn build_model_info_url(model_id: &str) -> String {
        format!("{}/{}", HF_API_BASE, model_id)
    }

    /// Fetch the file tree for a model repository.
    ///
    /// This is a low-level helper for fetching the file listing from HuggingFace.
    /// Prefer using `get_quantizations()` for most use cases.
    ///
    /// # Arguments
    ///
    /// * `model_id` - HuggingFace model ID
    /// * `path` - Optional subdirectory path
    ///
    /// # Returns
    ///
    /// Returns the raw JSON array of file entries.
    pub async fn fetch_tree(
        &self,
        model_id: &str,
        path: Option<&str>,
    ) -> Result<Vec<serde_json::Value>> {
        let url = Self::build_tree_url(model_id, path);
        let response = reqwest::get(&url).await?;

        if !response.status().is_success() {
            return Err(HuggingFaceError::ApiRequestFailed {
                status: response.status().as_u16(),
                url,
            }
            .into());
        }

        let data: serde_json::Value = response.json().await?;
        data.as_array().cloned().ok_or_else(|| {
            HuggingFaceError::InvalidResponse {
                message: "Expected array for tree response".to_string(),
            }
            .into()
        })
    }

    /// Fetch model info (commit SHA, etc.) from HuggingFace.
    ///
    /// # Arguments
    ///
    /// * `model_id` - HuggingFace model ID
    ///
    /// # Returns
    ///
    /// Returns the raw JSON model info.
    pub async fn fetch_model_info(&self, model_id: &str) -> Result<serde_json::Value> {
        let url = Self::build_model_info_url(model_id);
        let response = reqwest::get(&url).await?;

        if !response.status().is_success() {
            return Err(HuggingFaceError::ApiRequestFailed {
                status: response.status().as_u16(),
                url,
            }
            .into());
        }

        Ok(response.json().await?)
    }

    /// Get the commit SHA for a model repository.
    ///
    /// # Arguments
    ///
    /// * `model_id` - HuggingFace model ID
    ///
    /// # Returns
    ///
    /// Returns the commit SHA string, or "main" if not found.
    pub async fn get_commit_sha(&self, model_id: &str) -> Result<String> {
        let info = self.fetch_model_info(model_id).await?;
        Ok(info
            .get("sha")
            .and_then(|v| v.as_str())
            .unwrap_or("main")
            .to_string())
    }

    /// Find GGUF files for a specific quantization in a model repository.
    ///
    /// This handles both flat repos (files at root) and structured repos
    /// (files in quantization-named subdirectories).
    ///
    /// # Arguments
    ///
    /// * `model_id` - HuggingFace model ID
    /// * `quantization` - Quantization name to filter by (e.g., "Q4_K_M")
    ///
    /// # Returns
    ///
    /// Returns a vector of (filename, size_bytes) tuples for matching files.
    pub async fn find_gguf_files_for_quantization(
        &self,
        model_id: &str,
        quantization: &str,
    ) -> Result<Vec<(String, u64)>> {
        let quant_upper = quantization.to_uppercase();
        let files = self.fetch_tree(model_id, None).await?;

        let mut matching_files: Vec<(String, u64)> = Vec::new();

        // Check top-level files
        for file in &files {
            if let Some(filename) = file.get("path").and_then(|v| v.as_str()) {
                let entry_type = file.get("type").and_then(|v| v.as_str()).unwrap_or("file");

                // Direct GGUF files at repo root
                if entry_type == "file" && filename.ends_with(".gguf") {
                    let file_quant = extract_quantization_from_filename(filename);
                    if file_quant.to_uppercase() == quant_upper {
                        let file_size = file.get("size").and_then(|v| v.as_u64()).unwrap_or(0);
                        matching_files.push((filename.to_string(), file_size));
                    }
                }

                // Sharded GGUF files in per-quant directories
                if entry_type == "directory" && filename.to_uppercase().contains(&quant_upper) {
                    if let Ok(sub_files) = self.fetch_tree(model_id, Some(filename)).await {
                        for sub_file in sub_files {
                            if let Some(sub_path) = sub_file.get("path").and_then(|v| v.as_str()) {
                                if sub_path.ends_with(".gguf") {
                                    let sub_quant = extract_quantization_from_filename(sub_path);
                                    if sub_quant.to_uppercase() == quant_upper {
                                        let file_size = sub_file
                                            .get("size")
                                            .and_then(|v| v.as_u64())
                                            .unwrap_or(0);
                                        matching_files.push((sub_path.to_string(), file_size));
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }

        // Sort shard files by name to ensure correct order
        matching_files.sort_by(|a, b| a.0.cmp(&b.0));

        Ok(matching_files)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_build_expand_params_includes_all_fields() {
        let params = HuggingFaceService::build_expand_params();

        // Verify all required fields are present
        assert!(params.contains("expand[]=siblings"));
        assert!(params.contains("expand[]=gguf"));
        assert!(params.contains("expand[]=likes"));
        assert!(params.contains("expand[]=downloads"));
    }

    #[test]
    fn test_build_search_url_includes_expand_params() {
        let url = HuggingFaceService::build_search_url(100, 0, &HfSortField::Downloads, false);

        // Verify URL structure
        assert!(url.starts_with("https://huggingface.co/api/models"));
        assert!(url.contains("library=gguf"));
        assert!(url.contains("pipeline_tag=text-generation"));

        // Verify expand params (the fix for #39)
        assert!(url.contains("expand[]=likes"));
        assert!(url.contains("expand[]=downloads"));
        assert!(url.contains("expand[]=siblings"));
        assert!(url.contains("expand[]=gguf"));

        // Verify default sort params
        assert!(url.contains("sort=downloads"));
        assert!(url.contains("direction=-1"));

        // Verify pagination params
        assert!(url.contains("limit=100"));
        assert!(url.contains("p=0"));
    }

    #[test]
    fn test_build_search_url_with_different_sort_options() {
        // Test sorting by likes descending
        let url = HuggingFaceService::build_search_url(50, 1, &HfSortField::Likes, false);
        assert!(url.contains("sort=likes"));
        assert!(url.contains("direction=-1"));
        assert!(url.contains("limit=50"));
        assert!(url.contains("p=1"));

        // Test sorting by name ascending (alphabetical)
        let url = HuggingFaceService::build_search_url(30, 0, &HfSortField::Alphabetical, true);
        assert!(url.contains("sort=id"));
        assert!(url.contains("direction=1"));

        // Test sorting by modified date
        let url = HuggingFaceService::build_search_url(30, 0, &HfSortField::Modified, false);
        assert!(url.contains("sort=lastModified"));
        assert!(url.contains("direction=-1"));

        // Test sorting by created date
        let url = HuggingFaceService::build_search_url(30, 0, &HfSortField::Created, false);
        assert!(url.contains("sort=createdAt"));
        assert!(url.contains("direction=-1"));
    }

    #[test]
    fn test_parse_model_summary_with_likes() {
        let json = serde_json::json!({
            "id": "TheBloke/Llama-2-7B-GGUF",
            "downloads": 50000,
            "likes": 42,
            "lastModified": "2024-01-15T10:30:00Z",
            "siblings": [
                {"rfilename": "llama-2-7b.Q4_K_M.gguf"}
            ],
            "gguf": {
                "total": 7000000000_u64
            },
            "tags": ["llama", "gguf"]
        });

        let model = HuggingFaceService::parse_model_summary(&json).unwrap();

        assert_eq!(model.id, "TheBloke/Llama-2-7B-GGUF");
        assert_eq!(model.name, "Llama-2-7B-GGUF");
        assert_eq!(model.author, Some("TheBloke".to_string()));
        assert_eq!(model.downloads, 50000);
        assert_eq!(model.likes, 42); // The fix for #39
        assert!(model.parameters_b.is_some());
        assert!((model.parameters_b.unwrap() - 7.0).abs() < 0.1);
    }

    #[test]
    fn test_parse_model_summary_missing_likes_defaults_to_zero() {
        let json = serde_json::json!({
            "id": "SomeOrg/Model-GGUF",
            "downloads": 1000,
            // "likes" field is missing
            "siblings": [
                {"rfilename": "model.Q4_K_M.gguf"}
            ]
        });

        let model = HuggingFaceService::parse_model_summary(&json).unwrap();

        assert_eq!(model.likes, 0); // Should default to 0
        assert_eq!(model.downloads, 1000);
    }

    #[test]
    fn test_parse_model_summary_no_gguf_files_returns_none() {
        let json = serde_json::json!({
            "id": "meta-llama/Llama-3.1-8B",
            "downloads": 100000,
            "likes": 500,
            "siblings": [
                {"rfilename": "model.safetensors"},
                {"rfilename": "config.json"}
            ]
        });

        let model = HuggingFaceService::parse_model_summary(&json);
        assert!(model.is_none()); // No GGUF files, should be filtered out
    }

    #[test]
    fn test_parse_model_summary_missing_id_returns_none() {
        let json = serde_json::json!({
            "downloads": 1000,
            "likes": 10,
            "siblings": [
                {"rfilename": "model.Q4_K_M.gguf"}
            ]
        });

        let model = HuggingFaceService::parse_model_summary(&json);
        assert!(model.is_none());
    }

    #[test]
    fn test_service_is_clone() {
        let service = HuggingFaceService::new();
        let _cloned = service.clone();
    }

    #[test]
    fn test_service_is_default() {
        let service = HuggingFaceService;
        // Just verify it compiles and doesn't panic
        let _ = service;
    }
}
