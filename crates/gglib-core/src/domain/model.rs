//! Model domain types.
//!
//! These types represent models in the system, independent of any
//! infrastructure concerns (database, filesystem, etc.).

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::PathBuf;

use super::capabilities::ModelCapabilities;

// ─────────────────────────────────────────────────────────────────────────────
// Filter/Aggregate Types
// ─────────────────────────────────────────────────────────────────────────────

/// Filter options for the model library UI.
///
/// Contains aggregate data about available models for building
/// dynamic filter controls.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelFilterOptions {
    /// All distinct quantization types present in the library.
    pub quantizations: Vec<String>,
    /// Minimum and maximum parameter counts (in billions).
    pub param_range: Option<RangeValues>,
    /// Minimum and maximum context lengths.
    pub context_range: Option<RangeValues>,
}

/// A range of numeric values with min and max.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RangeValues {
    pub min: f64,
    pub max: f64,
}

// ─────────────────────────────────────────────────────────────────────────────
// Model Types
// ─────────────────────────────────────────────────────────────────────────────

/// A model that exists in the system with a database ID.
///
/// This represents a persisted model with all its metadata.
/// Use `NewModel` for models that haven't been persisted yet.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Model {
    /// Database ID of the model (always present for persisted models).
    pub id: i64,
    /// Human-readable name for the model.
    pub name: String,
    /// Absolute path to the GGUF file on the filesystem.
    pub file_path: PathBuf,
    /// Number of parameters in the model (in billions).
    pub param_count_b: f64,
    /// Model architecture (e.g., "llama", "mistral", "falcon").
    pub architecture: Option<String>,
    /// Quantization type (e.g., "`Q4_0`", "`Q8_0`", "`F16`", "`F32`").
    pub quantization: Option<String>,
    /// Maximum context length the model supports.
    pub context_length: Option<u64>,
    /// Additional metadata key-value pairs from the GGUF file.
    pub metadata: HashMap<String, String>,
    /// UTC timestamp of when the model was added to the database.
    pub added_at: DateTime<Utc>,
    /// `HuggingFace` repository ID (e.g., "`TheBloke/Llama-2-7B-GGUF`").
    pub hf_repo_id: Option<String>,
    /// Git commit SHA from `HuggingFace` Hub.
    pub hf_commit_sha: Option<String>,
    /// Original filename on `HuggingFace` Hub.
    pub hf_filename: Option<String>,
    /// Timestamp of when this model was downloaded from `HuggingFace`.
    pub download_date: Option<DateTime<Utc>>,
    /// Last time we checked for updates on `HuggingFace`.
    pub last_update_check: Option<DateTime<Utc>>,
    /// User-defined tags for organizing models.
    pub tags: Vec<String>,
    /// Model capabilities inferred from chat template analysis.
    #[serde(default)]
    pub capabilities: ModelCapabilities,
}

/// A model to be inserted into the system (no ID yet).
///
/// This represents a model that hasn't been persisted to the database.
/// After insertion, the repository returns a `Model` with the assigned ID.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NewModel {
    /// Human-readable name for the model.
    pub name: String,
    /// Absolute path to the GGUF file on the filesystem.
    pub file_path: PathBuf,
    /// Number of parameters in the model (in billions).
    pub param_count_b: f64,
    /// Model architecture (e.g., "llama", "mistral", "falcon").
    pub architecture: Option<String>,
    /// Quantization type (e.g., "`Q4_0`", "`Q8_0`", "`F16`", "`F32`").
    pub quantization: Option<String>,
    /// Maximum context length the model supports.
    pub context_length: Option<u64>,
    /// Additional metadata key-value pairs from the GGUF file.
    pub metadata: HashMap<String, String>,
    /// UTC timestamp of when the model was added to the database.
    pub added_at: DateTime<Utc>,
    /// `HuggingFace` repository ID (e.g., "`TheBloke/Llama-2-7B-GGUF`").
    pub hf_repo_id: Option<String>,
    /// Git commit SHA from `HuggingFace` Hub.
    pub hf_commit_sha: Option<String>,
    /// Original filename on `HuggingFace` Hub.
    pub hf_filename: Option<String>,
    /// Timestamp of when this model was downloaded from `HuggingFace`.
    pub download_date: Option<DateTime<Utc>>,
    /// Last time we checked for updates on `HuggingFace`.
    pub last_update_check: Option<DateTime<Utc>>,
    /// User-defined tags for organizing models.
    pub tags: Vec<String>,
    /// Ordered list of all file paths for sharded models (None for single-file models).
    pub file_paths: Option<Vec<PathBuf>>,
    /// Model capabilities inferred from chat template analysis.
    #[serde(default)]
    pub capabilities: ModelCapabilities,
}

impl NewModel {
    /// Create a new model with minimal required fields.
    ///
    /// Other fields are set to `None` or empty defaults.
    #[must_use]
    pub fn new(
        name: String,
        file_path: PathBuf,
        param_count_b: f64,
        added_at: DateTime<Utc>,
    ) -> Self {
        Self {
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
            file_paths: None,
            capabilities: ModelCapabilities::default(),
        }
    }
}

impl Model {
    /// Convert this model to a `NewModel` (drops the ID).
    ///
    /// Useful when you need to clone a model's data without the ID.
    #[must_use]
    pub fn to_new_model(&self) -> NewModel {
        NewModel {
            name: self.name.clone(),
            file_path: self.file_path.clone(),
            param_count_b: self.param_count_b,
            architecture: self.architecture.clone(),
            quantization: self.quantization.clone(),
            context_length: self.context_length,
            metadata: self.metadata.clone(),
            added_at: self.added_at,
            hf_repo_id: self.hf_repo_id.clone(),
            hf_commit_sha: self.hf_commit_sha.clone(),
            hf_filename: self.hf_filename.clone(),
            download_date: self.download_date,
            last_update_check: self.last_update_check,
            tags: self.tags.clone(),
            file_paths: None, // Not preserved in conversion
            capabilities: self.capabilities,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;

    #[test]
    fn test_new_model_creation() {
        let model = NewModel::new(
            "Test Model".to_string(),
            PathBuf::from("/path/to/model.gguf"),
            7.0,
            Utc::now(),
        );

        assert_eq!(model.name, "Test Model");
        assert!((model.param_count_b - 7.0).abs() < f64::EPSILON);
        assert!(model.architecture.is_none());
        assert!(model.tags.is_empty());
    }

    #[test]
    fn test_model_to_new_model() {
        let model = Model {
            id: 42,
            name: "Persisted Model".to_string(),
            file_path: PathBuf::from("/path/to/model.gguf"),
            param_count_b: 13.0,
            architecture: Some("llama".to_string()),
            quantization: Some("Q4_0".to_string()),
            context_length: Some(4096),
            metadata: HashMap::new(),
            added_at: Utc::now(),
            hf_repo_id: Some("TheBloke/Model-GGUF".to_string()),
            hf_commit_sha: None,
            hf_filename: None,
            download_date: None,
            last_update_check: None,
            tags: vec!["chat".to_string()],
            capabilities: ModelCapabilities::default(),
        };

        let new_model = model.to_new_model();
        assert_eq!(new_model.name, "Persisted Model");
        assert_eq!(new_model.architecture, Some("llama".to_string()));
        assert_eq!(new_model.tags, vec!["chat".to_string()]);
    }
}
