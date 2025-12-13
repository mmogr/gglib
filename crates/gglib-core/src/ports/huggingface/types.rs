//! Core-owned DTOs for `HuggingFace` operations.
//!
//! These types cross the boundary between `gglib-hf` and consumers.
//! They contain only the data needed by the core domain, not internal
//! `HuggingFace` API details.

use serde::{Deserialize, Serialize};

/// Information about a `HuggingFace` repository/model.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HfRepoInfo {
    /// Full model ID (e.g., "TheBloke/Llama-2-7B-GGUF")
    pub model_id: String,
    /// Model name (derived from ID)
    pub name: String,
    /// Author/organization
    pub author: Option<String>,
    /// Total download count
    pub downloads: u64,
    /// Like count
    pub likes: u64,
    /// Parameter count in billions (if known)
    pub parameters_b: Option<f64>,
    /// Short description
    pub description: Option<String>,
    /// Last modified timestamp (ISO 8601)
    pub last_modified: Option<String>,
    /// Chat template (from `config.chat_template` or `cardData`)
    pub chat_template: Option<String>,
    /// Model tags
    #[serde(default)]
    pub tags: Vec<String>,
}

/// Information about a file in a `HuggingFace` repository.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HfFileInfo {
    /// Path relative to repository root
    pub path: String,
    /// File size in bytes
    pub size: u64,
    /// Whether this is a GGUF file
    pub is_gguf: bool,
}

/// Information about a quantization variant.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HfQuantInfo {
    /// Quantization name (e.g., `Q4_K_M`)
    pub name: String,
    /// Number of files (1 for single, >1 for sharded)
    pub shard_count: usize,
    /// Total size across all shards
    pub total_size: u64,
    /// Paths to all files for this quantization
    pub file_paths: Vec<String>,
}

impl HfQuantInfo {
    /// Check if this quantization is sharded.
    #[must_use]
    pub const fn is_sharded(&self) -> bool {
        self.shard_count > 1
    }

    /// Get size in megabytes.
    #[must_use]
    #[allow(clippy::cast_precision_loss)]
    pub fn size_mb(&self) -> f64 {
        self.total_size as f64 / 1_048_576.0
    }

    /// Get size in gigabytes.
    #[must_use]
    #[allow(clippy::cast_precision_loss)]
    pub fn size_gb(&self) -> f64 {
        self.total_size as f64 / 1_073_741_824.0
    }
}

/// Options for searching `HuggingFace` models.
#[derive(Debug, Clone, Default)]
pub struct HfSearchOptions {
    /// Search query string
    pub query: Option<String>,
    /// Minimum parameter count in billions
    pub min_params_b: Option<f64>,
    /// Maximum parameter count in billions
    pub max_params_b: Option<f64>,
    /// Maximum number of results
    pub limit: u32,
    /// Page number (0-indexed)
    pub page: u32,
    /// Sort field: "downloads", "likes", "modified", "created"
    pub sort_by: String,
    /// Sort ascending (false = descending)
    pub sort_ascending: bool,
}

impl HfSearchOptions {
    /// Create new search options with defaults.
    #[must_use]
    pub fn new() -> Self {
        Self {
            limit: 30,
            sort_by: "downloads".to_string(),
            sort_ascending: false,
            ..Default::default()
        }
    }

    /// Set the search query.
    #[must_use]
    pub fn with_query(mut self, query: impl Into<String>) -> Self {
        self.query = Some(query.into());
        self
    }

    /// Set the result limit.
    #[must_use]
    pub const fn with_limit(mut self, limit: u32) -> Self {
        self.limit = limit;
        self
    }

    /// Set the page number.
    #[must_use]
    pub const fn with_page(mut self, page: u32) -> Self {
        self.page = page;
        self
    }

    /// Set parameter size filters.
    #[must_use]
    pub const fn with_params_filter(mut self, min: Option<f64>, max: Option<f64>) -> Self {
        self.min_params_b = min;
        self.max_params_b = max;
        self
    }

    /// Set sort options.
    #[must_use]
    pub fn with_sort(mut self, field: impl Into<String>, ascending: bool) -> Self {
        self.sort_by = field.into();
        self.sort_ascending = ascending;
        self
    }
}

/// Result of a `HuggingFace` model search.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HfSearchResult {
    /// Models matching the search
    pub items: Vec<HfRepoInfo>,
    /// Whether more results are available
    pub has_more: bool,
    /// Current page number
    pub page: u32,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_search_options_builder() {
        let opts = HfSearchOptions::new()
            .with_query("llama")
            .with_limit(50)
            .with_page(2)
            .with_params_filter(Some(7.0), Some(70.0))
            .with_sort("likes", true);

        assert_eq!(opts.query, Some("llama".to_string()));
        assert_eq!(opts.limit, 50);
        assert_eq!(opts.page, 2);
        assert_eq!(opts.min_params_b, Some(7.0));
        assert_eq!(opts.max_params_b, Some(70.0));
        assert_eq!(opts.sort_by, "likes");
        assert!(opts.sort_ascending);
    }

    #[test]
    fn test_quant_info_helpers() {
        let single = HfQuantInfo {
            name: "Q4_K_M".to_string(),
            shard_count: 1,
            total_size: 4_000_000_000,
            file_paths: vec!["model.gguf".to_string()],
        };
        assert!(!single.is_sharded());
        assert!((single.size_gb() - 3.72).abs() < 0.1);

        let sharded = HfQuantInfo {
            name: "Q8_0".to_string(),
            shard_count: 3,
            total_size: 12_000_000_000,
            file_paths: vec![
                "a.gguf".to_string(),
                "b.gguf".to_string(),
                "c.gguf".to_string(),
            ],
        };
        assert!(sharded.is_sharded());
    }
}
