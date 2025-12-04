#![doc = include_str!(concat!(env!("OUT_DIR"), "/models_docs.md"))]

pub mod gui;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::PathBuf;

/// Represents a GGUF (GPT-Generated Unified Format) model managed by the library.
///
/// This struct contains essential metadata extracted from GGUF files, focusing
/// on the most important information for model management.
///
/// # Fields
///
/// * `name` - A human-readable name for the model (from metadata or filename)
/// * `file_path` - The absolute path to the GGUF file on the filesystem
/// * `param_count_b` - The number of parameters in the model (in billions)
/// * `architecture` - Model architecture (e.g., "llama", "mistral")
/// * `quantization` - Quantization type (e.g., "Q4_0", "F16")
/// * `context_length` - Maximum context length the model supports
/// * `metadata` - Additional metadata key-value pairs from the GGUF file
/// * `added_at` - UTC timestamp of when the model was added to the database
///
/// # Examples
///
/// ```rust
/// use std::path::PathBuf;
/// use chrono::Utc;
/// use gglib::Gguf;
/// use std::collections::HashMap;
///
/// let model = Gguf {
///     id: None,
///     name: "Llama 2 7B Chat".to_string(),
///     file_path: PathBuf::from("/models/llama-2-7b-chat.gguf"),
///     param_count_b: 7.0,
///     architecture: Some("llama".to_string()),
///     quantization: Some("Q4_0".to_string()),
///     context_length: Some(4096),
///     metadata: HashMap::new(),
///     added_at: Utc::now(),
///     hf_repo_id: None,
///     hf_commit_sha: None,
///     hf_filename: None,
///     download_date: None,
///     last_update_check: None,
///     tags: Vec::new(),
/// };
/// ```
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Gguf {
    /// Database ID of the model
    pub id: Option<u32>,
    /// Human-readable name for the model (from metadata or filename)
    pub name: String,
    /// Absolute path to the GGUF file on the filesystem
    pub file_path: PathBuf,
    /// Number of parameters in the model (in billions)
    pub param_count_b: f64,
    /// Model architecture (e.g., "llama", "mistral", "falcon")
    pub architecture: Option<String>,
    /// Quantization type (e.g., "Q4_0", "Q8_0", "F16", "F32")
    pub quantization: Option<String>,
    /// Maximum context length the model supports
    pub context_length: Option<u64>,
    /// Additional metadata key-value pairs from the GGUF file
    pub metadata: HashMap<String, String>,
    /// UTC timestamp of when the model was added to the database
    pub added_at: DateTime<Utc>,
    /// HuggingFace repository ID (e.g., "microsoft/DialoGPT-medium")
    pub hf_repo_id: Option<String>,
    /// Git commit SHA from HuggingFace Hub
    pub hf_commit_sha: Option<String>,
    /// Original filename on HuggingFace Hub
    pub hf_filename: Option<String>,
    /// Timestamp of when this model was downloaded from HuggingFace
    pub download_date: Option<DateTime<Utc>>,
    /// Last time we checked for updates on HuggingFace
    pub last_update_check: Option<DateTime<Utc>>,
    /// User-defined tags for organizing models
    pub tags: Vec<String>,
}

impl Gguf {
    /// Create a new Gguf instance with minimal required fields
    /// Other fields are set to None/defaults
    pub fn new(
        name: String,
        file_path: PathBuf,
        param_count_b: f64,
        added_at: DateTime<Utc>,
    ) -> Self {
        Self {
            id: None,
            name,
            file_path,
            param_count_b,
            architecture: None,
            quantization: None,
            context_length: None,
            metadata: HashMap::new(),
            added_at,
            hf_repo_id: None,
            hf_commit_sha: None,
            hf_filename: None,
            download_date: None,
            last_update_check: None,
            tags: Vec::new(),
        }
    }
}

/// Information about available GGUF files for a HuggingFace model
#[derive(Debug, Clone)]
pub struct HfModelInfo {
    /// Repository ID (e.g., "microsoft/DialoGPT-medium")
    pub repo_id: String,
    /// Latest commit SHA
    pub commit_sha: String,
    /// Available GGUF files with their quantization types
    pub gguf_files: Vec<HfGgufFile>,
}

/// Information about a specific GGUF file on HuggingFace
#[derive(Debug, Clone)]
pub struct HfGgufFile {
    /// Filename on HuggingFace
    pub filename: String,
    /// Detected quantization type from filename
    pub quantization: Option<String>,
    /// File size in bytes
    pub size: Option<u64>,
    /// Whether this is part of a multi-file model (e.g., "00001-of-00002")
    pub is_split: bool,
    /// Part number if this is a split file
    pub part_info: Option<(u32, u32)>, // (current_part, total_parts)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::gguf::GgufMetadata;
    use chrono::Utc;
    use std::path::PathBuf;

    #[test]
    fn test_gguf_creation() {
        let mut metadata = HashMap::new();
        metadata.insert("test_key".to_string(), "test_value".to_string());

        let model = Gguf {
            id: Some(1),
            name: "Test Model".to_string(),
            file_path: PathBuf::from("/test/path/model.gguf"),
            param_count_b: 7.0,
            architecture: Some("llama".to_string()),
            quantization: Some("Q4_0".to_string()),
            context_length: Some(4096),
            metadata: metadata.clone(),
            added_at: Utc::now(),
            hf_repo_id: None,
            hf_commit_sha: None,
            hf_filename: None,
            download_date: None,
            last_update_check: None,
            tags: Vec::new(),
        };

        assert_eq!(model.name, "Test Model");
        assert_eq!(model.param_count_b, 7.0);
        assert_eq!(model.architecture, Some("llama".to_string()));
        assert_eq!(model.quantization, Some("Q4_0".to_string()));
        assert_eq!(model.context_length, Some(4096));
        assert_eq!(model.metadata, metadata);
    }

    #[test]
    fn test_gguf_metadata_creation() {
        let mut metadata = HashMap::new();
        metadata.insert("general.name".to_string(), "Test".to_string());

        let gguf_metadata = GgufMetadata {
            name: Some("Test Model".to_string()),
            architecture: Some("llama".to_string()),
            param_count_b: Some(7.0),
            quantization: Some("Q4_0".to_string()),
            context_length: Some(4096),
            metadata,
        };

        assert_eq!(gguf_metadata.name, Some("Test Model".to_string()));
        assert_eq!(gguf_metadata.architecture, Some("llama".to_string()));
        assert_eq!(gguf_metadata.param_count_b, Some(7.0));
        assert_eq!(gguf_metadata.quantization, Some("Q4_0".to_string()));
        assert_eq!(gguf_metadata.context_length, Some(4096));
    }

    #[test]
    fn test_gguf_with_optional_fields() {
        let model = Gguf {
            id: None,
            name: "Minimal Model".to_string(),
            file_path: PathBuf::from("/test/minimal.gguf"),
            param_count_b: 1.3,
            architecture: None,
            quantization: None,
            context_length: None,
            metadata: HashMap::new(),
            added_at: Utc::now(),
            hf_repo_id: None,
            hf_commit_sha: None,
            hf_filename: None,
            download_date: None,
            last_update_check: None,
            tags: Vec::new(),
        };

        assert_eq!(model.name, "Minimal Model");
        assert_eq!(model.param_count_b, 1.3);
        assert_eq!(model.architecture, None);
        assert_eq!(model.quantization, None);
        assert_eq!(model.context_length, None);
        assert!(model.metadata.is_empty());
    }
}
