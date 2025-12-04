//! HuggingFace file resolution for GGUF models.
//!
//! This module extracts the logic for resolving quantization-specific files
//! from HuggingFace repositories, including sharded files.

use std::collections::HashMap;

use reqwest;
use serde_json;
use thiserror::Error;

use crate::download::domain::types::Quantization;
use crate::services::huggingface::build_tree_url_simple;

// ============================================================================
// Error Types
// ============================================================================

/// Errors that can occur during file resolution.
#[derive(Debug, Error)]
pub enum FileResolutionError {
    #[error("HTTP request failed: {0}")]
    HttpError(#[from] reqwest::Error),

    #[error("Failed to parse API response: {0}")]
    ParseError(String),

    #[error("No files found for quantization '{0}'")]
    NoFilesFound(String),

    #[error("Repository not found or inaccessible")]
    RepoNotFound,
}

// ============================================================================
// Result Types
// ============================================================================

/// Result of resolving files for a quantization.
#[derive(Debug, Clone)]
pub struct FileResolution {
    /// The resolved quantization.
    pub quantization: Quantization,
    /// List of files to download (sorted for sharded files).
    pub files: Vec<ResolvedFile>,
    /// Whether this is a sharded download.
    pub is_sharded: bool,
}

/// A single resolved file.
#[derive(Debug, Clone)]
pub struct ResolvedFile {
    /// Path within the repository.
    pub path: String,
    /// Size in bytes (if available from API).
    pub size: Option<u64>,
}

impl FileResolution {
    /// Get filenames as a simple list.
    pub fn filenames(&self) -> Vec<String> {
        self.files.iter().map(|f| f.path.clone()).collect()
    }

    /// Get total size if all file sizes are known.
    pub fn total_size(&self) -> Option<u64> {
        let sizes: Option<Vec<u64>> = self.files.iter().map(|f| f.size).collect();
        sizes.map(|s| s.iter().sum())
    }
}

// ============================================================================
// Resolver
// ============================================================================

/// Resolves GGUF files from HuggingFace repositories.
pub struct QuantizationFileResolver {
    client: reqwest::Client,
}

impl QuantizationFileResolver {
    /// Create a new resolver.
    pub fn new() -> Self {
        Self {
            client: reqwest::Client::new(),
        }
    }

    /// Resolve files for a specific quantization.
    ///
    /// This method queries the HuggingFace API to find all GGUF files
    /// matching the requested quantization, handling both single files
    /// and sharded (multi-part) files.
    pub async fn resolve(
        &self,
        repo_id: &str,
        quantization: &str,
    ) -> Result<FileResolution, FileResolutionError> {
        let quant_upper = quantization.to_uppercase();

        // Query top-level directory
        let api_url = build_tree_url_simple(repo_id, None);
        let response = self.client.get(&api_url).send().await?;

        if !response.status().is_success() {
            if response.status() == reqwest::StatusCode::NOT_FOUND {
                return Err(FileResolutionError::RepoNotFound);
            }
            return Err(FileResolutionError::ParseError(format!(
                "API returned status {}",
                response.status()
            )));
        }

        let json_text = response.text().await?;
        let data: serde_json::Value = serde_json::from_str(&json_text)
            .map_err(|e| FileResolutionError::ParseError(e.to_string()))?;

        let files = data
            .as_array()
            .ok_or_else(|| FileResolutionError::ParseError("Expected array".to_string()))?;

        // Collect matching files
        let mut matching_files: Vec<ResolvedFile> = Vec::new();

        for file in files {
            let path = file.get("path").and_then(|v| v.as_str()).unwrap_or("");
            let entry_type = file.get("type").and_then(|v| v.as_str()).unwrap_or("file");
            let size = file.get("size").and_then(|v| v.as_u64());

            if entry_type == "file" && path.ends_with(".gguf") {
                let file_quant = Quantization::from_filename(path);
                if file_quant.to_string().to_uppercase() == quant_upper {
                    matching_files.push(ResolvedFile {
                        path: path.to_string(),
                        size,
                    });
                }
            }

            // Check subdirectories for sharded files
            if entry_type == "directory" && path.to_uppercase().contains(&quant_upper) {
                let sub_files = self.resolve_directory(repo_id, path, &quant_upper).await?;
                matching_files.extend(sub_files);
            }
        }

        if matching_files.is_empty() {
            return Err(FileResolutionError::NoFilesFound(quantization.to_string()));
        }

        // Sort for consistent ordering (important for sharded files)
        matching_files.sort_by(|a, b| a.path.cmp(&b.path));

        let parsed_quant = Quantization::from_filename(&matching_files[0].path);
        let is_sharded = matching_files.len() > 1;

        Ok(FileResolution {
            quantization: parsed_quant,
            files: matching_files,
            is_sharded,
        })
    }

    /// List all available quantizations in a repository.
    pub async fn list_available(
        &self,
        repo_id: &str,
    ) -> Result<Vec<Quantization>, FileResolutionError> {
        let api_url = build_tree_url_simple(repo_id, None);
        let response = self.client.get(&api_url).send().await?;

        if !response.status().is_success() {
            if response.status() == reqwest::StatusCode::NOT_FOUND {
                return Err(FileResolutionError::RepoNotFound);
            }
            return Err(FileResolutionError::ParseError(format!(
                "API returned status {}",
                response.status()
            )));
        }

        let json_text = response.text().await?;
        let data: serde_json::Value = serde_json::from_str(&json_text)
            .map_err(|e| FileResolutionError::ParseError(e.to_string()))?;

        let files = data
            .as_array()
            .ok_or_else(|| FileResolutionError::ParseError("Expected array".to_string()))?;

        let mut seen: HashMap<String, Quantization> = HashMap::new();

        for file in files {
            let path = file.get("path").and_then(|v| v.as_str()).unwrap_or("");
            let entry_type = file.get("type").and_then(|v| v.as_str()).unwrap_or("file");

            if entry_type == "file" && path.ends_with(".gguf") {
                let quant = Quantization::from_filename(path);
                if !quant.is_unknown() {
                    seen.insert(quant.to_string(), quant);
                }
            }

            // Also check subdirectory names
            if entry_type == "directory" {
                let quant = Quantization::from_filename(path);
                if !quant.is_unknown() {
                    seen.insert(quant.to_string(), quant);
                }
            }
        }

        let mut result: Vec<Quantization> = seen.into_values().collect();
        result.sort_by_key(|q| q.to_string());

        Ok(result)
    }

    /// Resolve files within a subdirectory.
    async fn resolve_directory(
        &self,
        repo_id: &str,
        dir_path: &str,
        quant_upper: &str,
    ) -> Result<Vec<ResolvedFile>, FileResolutionError> {
        let sub_url = build_tree_url_simple(repo_id, Some(dir_path));
        let response = self.client.get(&sub_url).send().await?;

        if !response.status().is_success() {
            return Ok(Vec::new());
        }

        let json_text = response.text().await?;
        let data: serde_json::Value = serde_json::from_str(&json_text)
            .map_err(|e| FileResolutionError::ParseError(e.to_string()))?;

        let files = match data.as_array() {
            Some(f) => f,
            None => return Ok(Vec::new()),
        };

        let mut result = Vec::new();

        for file in files {
            let path = file.get("path").and_then(|v| v.as_str()).unwrap_or("");
            let size = file.get("size").and_then(|v| v.as_u64());

            if path.ends_with(".gguf") {
                let file_quant = Quantization::from_filename(path);
                if file_quant.to_string().to_uppercase() == *quant_upper {
                    result.push(ResolvedFile {
                        path: path.to_string(),
                        size,
                    });
                }
            }
        }

        Ok(result)
    }
}

impl Default for QuantizationFileResolver {
    fn default() -> Self {
        Self::new()
    }
}

// ============================================================================
// Convenience Function
// ============================================================================

/// Resolve files for a quantization (convenience function).
pub async fn resolve_quantization_files(
    repo_id: &str,
    quantization: &str,
) -> Result<FileResolution, FileResolutionError> {
    let resolver = QuantizationFileResolver::new();
    resolver.resolve(repo_id, quantization).await
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_file_resolution_filenames() {
        let resolution = FileResolution {
            quantization: Quantization::Q4KM,
            files: vec![
                ResolvedFile {
                    path: "model-00001.gguf".to_string(),
                    size: Some(1000),
                },
                ResolvedFile {
                    path: "model-00002.gguf".to_string(),
                    size: Some(1000),
                },
            ],
            is_sharded: true,
        };

        assert_eq!(
            resolution.filenames(),
            vec!["model-00001.gguf", "model-00002.gguf"]
        );
        assert_eq!(resolution.total_size(), Some(2000));
    }

    #[test]
    fn test_file_resolution_total_size_unknown() {
        let resolution = FileResolution {
            quantization: Quantization::Q4KM,
            files: vec![ResolvedFile {
                path: "model.gguf".to_string(),
                size: None,
            }],
            is_sharded: false,
        };

        assert_eq!(resolution.total_size(), None);
    }
}
