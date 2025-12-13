//! Internal API response types for `HuggingFace` Hub.
//!
//! These types are internal to `gglib-hf` and are not exposed to consumers.
//! External consumers should use the port DTOs defined in `gglib-core`.

// Some helper methods are not yet used but will be useful for future features
#![allow(dead_code)]

use serde::{Deserialize, Serialize};
use url::Url;

// ============================================================================
// Configuration (used internally, see config.rs for public config)
// ============================================================================

/// Internal configuration for the `HuggingFace` client.
#[derive(Debug, Clone)]
pub struct HfConfig {
    /// Base URL for the `HuggingFace` API (default: <https://huggingface.co/api/models>)
    pub base_url: Url,
    /// Optional authentication token for private models
    pub token: Option<String>,
    /// Maximum number of retry attempts for transient errors (default: 3)
    pub max_retries: u8,
    /// Base delay in milliseconds for exponential backoff (default: 500)
    pub retry_base_delay_ms: u64,
}

impl Default for HfConfig {
    fn default() -> Self {
        Self {
            base_url: Url::parse("https://huggingface.co/api/models")
                .expect("default HF API URL is valid"),
            token: None,
            max_retries: 3,
            retry_base_delay_ms: 500,
        }
    }
}

// ============================================================================
// Repository Reference
// ============================================================================

/// Reference to a `HuggingFace` repository.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct HfRepoRef {
    /// Repository owner (user or organization)
    pub owner: String,
    /// Repository name
    pub name: String,
}

impl HfRepoRef {
    /// Create a new repository reference.
    pub fn new(owner: impl Into<String>, name: impl Into<String>) -> Self {
        Self {
            owner: owner.into(),
            name: name.into(),
        }
    }

    /// Parse a repository reference from a model ID string.
    pub fn parse(model_id: &str) -> Option<Self> {
        let parts: Vec<&str> = model_id.splitn(2, '/').collect();
        if parts.len() == 2 && !parts[0].is_empty() && !parts[1].is_empty() {
            Some(Self {
                owner: parts[0].to_string(),
                name: parts[1].to_string(),
            })
        } else {
            None
        }
    }

    /// Get the full model ID (owner/name).
    pub fn id(&self) -> String {
        format!("{}/{}", self.owner, self.name)
    }
}

impl std::fmt::Display for HfRepoRef {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}/{}", self.owner, self.name)
    }
}

// ============================================================================
// File Entry
// ============================================================================

/// Type of entry in a repository tree.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum HfEntryType {
    /// Regular file
    File,
    /// Directory
    Directory,
}

/// Entry in a `HuggingFace` repository file tree.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HfFileEntry {
    /// Path relative to repository root
    pub path: String,
    /// Entry type (file or directory)
    pub entry_type: HfEntryType,
    /// File size in bytes (0 for directories)
    pub size: u64,
}

impl HfFileEntry {
    /// Check if this is a GGUF file.
    pub fn is_gguf(&self) -> bool {
        self.entry_type == HfEntryType::File
            && std::path::Path::new(&self.path)
                .extension()
                .is_some_and(|ext| ext.eq_ignore_ascii_case("gguf"))
    }

    /// Check if this is a directory.
    pub fn is_directory(&self) -> bool {
        self.entry_type == HfEntryType::Directory
    }

    /// Get the filename without path.
    pub fn filename(&self) -> &str {
        self.path.rsplit('/').next().unwrap_or(&self.path)
    }
}

// ============================================================================
// Quantization (internal)
// ============================================================================

/// Information about a quantization variant in a repository.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HfQuantization {
    /// Quantization name (e.g., `Q4_K_M`, `Q8_0`)
    pub name: String,
    /// Number of files (1 for single file, >1 for sharded)
    pub shard_count: usize,
    /// Paths to all files for this quantization
    pub paths: Vec<String>,
    /// Total size in bytes across all shards
    pub total_size: u64,
}

impl HfQuantization {
    /// Check if this quantization is sharded (multiple files).
    pub const fn is_sharded(&self) -> bool {
        self.shard_count > 1
    }

    /// Get size in megabytes.
    #[allow(clippy::cast_precision_loss)] // Precision loss acceptable for display purposes
    pub fn size_mb(&self) -> f64 {
        self.total_size as f64 / 1_048_576.0
    }

    /// Get the primary file path (first shard or single file).
    pub fn primary_path(&self) -> Option<&str> {
        self.paths.first().map(String::as_str)
    }
}

// ============================================================================
// Model Summary (API response)
// ============================================================================

/// Summary of a `HuggingFace` model from the search API.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HfModelSummary {
    /// Model ID (e.g., "TheBloke/Llama-2-7B-GGUF")
    pub id: String,
    /// Human-readable model name (derived from id)
    pub name: String,
    /// Author/organization (e.g., `TheBloke`)
    pub author: Option<String>,
    /// Total download count
    pub downloads: u64,
    /// Like count
    pub likes: u64,
    /// Last modified timestamp (ISO 8601)
    pub last_modified: Option<String>,
    /// Total parameter count in billions (from gguf.total)
    pub parameters_b: Option<f64>,
    /// Model description/README excerpt
    pub description: Option<String>,
    /// Model tags
    #[serde(default)]
    pub tags: Vec<String>,
}

impl HfModelSummary {
    /// Get a reference to this model's repository.
    pub fn repo_ref(&self) -> Option<HfRepoRef> {
        HfRepoRef::parse(&self.id)
    }
}

// ============================================================================
// Search Types
// ============================================================================

/// Sort field options for `HuggingFace` model search.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, Default, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum HfSortField {
    /// Sort by download count (default)
    #[default]
    Downloads,
    /// Sort by number of likes
    Likes,
    /// Sort by last modified date
    Modified,
    /// Sort by creation date
    Created,
    /// Sort alphabetically by name
    #[serde(rename = "id")]
    Alphabetical,
}

impl HfSortField {
    /// Get the API parameter value for this sort field.
    pub const fn as_api_param(self) -> &'static str {
        match self {
            Self::Downloads => "downloads",
            Self::Likes => "likes",
            Self::Modified => "lastModified",
            Self::Created => "createdAt",
            Self::Alphabetical => "id",
        }
    }
}

/// Query parameters for searching `HuggingFace` models.
#[derive(Debug, Clone, Default)]
pub struct HfSearchQuery {
    /// Search query (model name)
    pub query: Option<String>,
    /// Minimum parameters in billions
    pub min_params_b: Option<f64>,
    /// Maximum parameters in billions
    pub max_params_b: Option<f64>,
    /// Page number (0-indexed)
    pub page: u32,
    /// Results per page (default 30)
    pub limit: u32,
    /// Sort field (default: downloads)
    pub sort_by: HfSortField,
    /// Sort direction: true = ascending, false = descending
    pub sort_ascending: bool,
}

impl HfSearchQuery {
    /// Create a new search query with defaults.
    pub fn new() -> Self {
        Self {
            limit: 30,
            ..Default::default()
        }
    }

    /// Set the search query string.
    pub fn with_query(mut self, query: impl Into<String>) -> Self {
        self.query = Some(query.into());
        self
    }

    /// Set the page number.
    pub const fn with_page(mut self, page: u32) -> Self {
        self.page = page;
        self
    }

    /// Set the results limit.
    pub const fn with_limit(mut self, limit: u32) -> Self {
        self.limit = limit;
        self
    }

    /// Set the sort field and direction.
    pub const fn with_sort(mut self, field: HfSortField, ascending: bool) -> Self {
        self.sort_by = field;
        self.sort_ascending = ascending;
        self
    }

    /// Set parameter size filters.
    pub const fn with_params_filter(mut self, min: Option<f64>, max: Option<f64>) -> Self {
        self.min_params_b = min;
        self.max_params_b = max;
        self
    }
}

/// Response from `HuggingFace` model search.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HfSearchResponse {
    /// Models matching the search criteria
    pub items: Vec<HfModelSummary>,
    /// Whether more results are available
    pub has_more: bool,
    /// Current page number (0-indexed)
    pub page: u32,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_hf_config_default() {
        let config = HfConfig::default();
        assert_eq!(
            config.base_url.as_str(),
            "https://huggingface.co/api/models"
        );
        assert!(config.token.is_none());
        assert_eq!(config.max_retries, 3);
    }

    #[test]
    fn test_hf_repo_ref_parse() {
        let repo = HfRepoRef::parse("TheBloke/Llama-2-7B-GGUF").unwrap();
        assert_eq!(repo.owner, "TheBloke");
        assert_eq!(repo.name, "Llama-2-7B-GGUF");
        assert_eq!(repo.id(), "TheBloke/Llama-2-7B-GGUF");
    }

    #[test]
    fn test_hf_repo_ref_parse_invalid() {
        assert!(HfRepoRef::parse("no-slash").is_none());
        assert!(HfRepoRef::parse("/no-owner").is_none());
        assert!(HfRepoRef::parse("no-name/").is_none());
        assert!(HfRepoRef::parse("").is_none());
    }

    #[test]
    fn test_hf_file_entry_is_gguf() {
        let gguf = HfFileEntry {
            path: "model.Q4_K_M.gguf".to_string(),
            entry_type: HfEntryType::File,
            size: 1000,
        };
        assert!(gguf.is_gguf());

        let dir = HfFileEntry {
            path: "subdir".to_string(),
            entry_type: HfEntryType::Directory,
            size: 0,
        };
        assert!(!dir.is_gguf());

        let other = HfFileEntry {
            path: "README.md".to_string(),
            entry_type: HfEntryType::File,
            size: 100,
        };
        assert!(!other.is_gguf());
    }

    #[test]
    fn test_hf_quantization_is_sharded() {
        let single = HfQuantization {
            name: "Q4_K_M".to_string(),
            shard_count: 1,
            paths: vec!["model.Q4_K_M.gguf".to_string()],
            total_size: 4_000_000_000,
        };
        assert!(!single.is_sharded());

        let sharded = HfQuantization {
            name: "Q8_0".to_string(),
            shard_count: 3,
            paths: vec![
                "model-00001-of-00003.gguf".to_string(),
                "model-00002-of-00003.gguf".to_string(),
                "model-00003-of-00003.gguf".to_string(),
            ],
            total_size: 12_000_000_000,
        };
        assert!(sharded.is_sharded());
    }

    #[test]
    fn test_hf_search_query_builder() {
        let query = HfSearchQuery::new()
            .with_query("llama")
            .with_page(2)
            .with_limit(50)
            .with_sort(HfSortField::Likes, false);

        assert_eq!(query.query, Some("llama".to_string()));
        assert_eq!(query.page, 2);
        assert_eq!(query.limit, 50);
        assert_eq!(query.sort_by, HfSortField::Likes);
        assert!(!query.sort_ascending);
    }

    #[test]
    fn test_hf_sort_field_api_param() {
        assert_eq!(HfSortField::Downloads.as_api_param(), "downloads");
        assert_eq!(HfSortField::Likes.as_api_param(), "likes");
        assert_eq!(HfSortField::Modified.as_api_param(), "lastModified");
        assert_eq!(HfSortField::Created.as_api_param(), "createdAt");
        assert_eq!(HfSortField::Alphabetical.as_api_param(), "id");
    }
}
