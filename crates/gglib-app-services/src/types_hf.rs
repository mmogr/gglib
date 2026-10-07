//! The `HuggingFace` browser's DTOs: search, quantization listing and
//! tool-support detection.
//!
//! A `#[path]` child of `types.rs`, and everything here is re-exported from
//! `types`.

use gglib_core::ports::HfRepoInfo;
use serde::{Deserialize, Serialize};

pub use gglib_core::ports::HfSortField;

/// Summary of a `HuggingFace` model from the search API.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "ts-bindings", derive(ts_rs::TS), ts(export))]
pub struct HfModelSummary {
    /// Model ID (e.g., "TheBloke/Llama-2-7B-GGUF")
    pub id: String,
    /// Human-readable model name (derived from id)
    pub name: String,
    /// Author/organization (e.g., "`TheBloke`")
    pub author: Option<String>,
    /// Total download count
    #[cfg_attr(feature = "ts-bindings", ts(type = "number"))]
    pub downloads: u64,
    /// Like count
    #[cfg_attr(feature = "ts-bindings", ts(type = "number"))]
    pub likes: u64,
    /// Last modified timestamp
    pub last_modified: Option<String>,
    /// Total parameter count in billions (the GGUF header's, when the Hub gives it)
    pub parameters_b: Option<f64>,
    /// Model description/README excerpt
    pub description: Option<String>,
    /// Model tags
    #[serde(default)]
    pub tags: Vec<String>,
}

/// What the browser shows of a repository, whether a search found it or it
/// was looked up by its ID.
impl From<HfRepoInfo> for HfModelSummary {
    fn from(info: HfRepoInfo) -> Self {
        Self {
            id: info.model_id,
            name: info.name,
            author: info.author,
            downloads: info.downloads,
            likes: info.likes,
            last_modified: info.last_modified,
            parameters_b: info.parameters_b,
            description: info.description,
            tags: info.tags,
        }
    }
}

/// Request for searching `HuggingFace` models.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "ts-bindings", derive(ts_rs::TS), ts(export))]
pub struct HfSearchRequest {
    pub query: Option<String>,
    pub min_params_b: Option<f64>,
    pub max_params_b: Option<f64>,
    pub page: u32,
    pub limit: u32,
    #[serde(default)]
    pub sort_by: HfSortField,
    #[serde(default)]
    pub sort_ascending: bool,
}

impl Default for HfSearchRequest {
    fn default() -> Self {
        Self {
            query: None,
            min_params_b: None,
            max_params_b: None,
            page: 0,
            limit: 30,
            sort_by: HfSortField::default(),
            sort_ascending: false,
        }
    }
}

/// Response from `HuggingFace` model search.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "ts-bindings", derive(ts_rs::TS), ts(export))]
pub struct HfSearchResponse {
    pub models: Vec<HfModelSummary>,
    pub has_more: bool,
    pub page: u32,
    #[cfg_attr(feature = "ts-bindings", ts(type = "number | null"))]
    pub total_count: Option<u64>,
}

/// Information about a specific quantization variant.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "ts-bindings", derive(ts_rs::TS), ts(export))]
pub struct HfQuantization {
    pub name: String,
    pub file_path: String,
    #[cfg_attr(feature = "ts-bindings", ts(type = "number"))]
    pub size_bytes: u64,
    pub size_mb: f64,
    pub is_sharded: bool,
    pub shard_count: Option<u32>,
    /// The projector a download of this quantization fetches with it, when
    /// the repository has one. `size_bytes` above is the weights alone.
    pub projector: Option<HfProjector>,
}

/// A projector file of a `HuggingFace` repository.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts-bindings", derive(ts_rs::TS), ts(export))]
pub struct HfProjector {
    pub file_path: String,
    #[cfg_attr(feature = "ts-bindings", ts(type = "number"))]
    pub size_bytes: u64,
}

/// Response containing available quantizations for a model.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "ts-bindings", derive(ts_rs::TS), ts(export))]
pub struct HfQuantizationsResponse {
    pub model_id: String,
    pub quantizations: Vec<HfQuantization>,
}

/// Response for tool/function calling support detection.
///
/// Used for both `HuggingFace` model metadata and local running server queries.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "ts-bindings", derive(ts_rs::TS), ts(export))]
pub struct ToolSupportResponse {
    pub supports_tool_calls: bool,
    pub confidence: f32,
    pub detected_format: Option<String>,
}

impl From<gglib_core::ports::ToolSupportDetection> for ToolSupportResponse {
    fn from(detection: gglib_core::ports::ToolSupportDetection) -> Self {
        Self {
            supports_tool_calls: detection.supports_tool_calling,
            confidence: detection.confidence,
            detected_format: detection.detected_format.map(|f| f.to_string()),
        }
    }
}
