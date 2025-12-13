//! Model catalog port for listing and resolving models.
//!
//! This port defines the interface for querying the model catalog.
//! It provides domain-level model information without exposing
//! database or storage implementation details.

use async_trait::async_trait;
use std::fmt;
use std::path::PathBuf;
use thiserror::Error;

/// Domain model summary for catalog operations (listing).
///
/// This is a domain type (not an `OpenAI` API type). The proxy layer
/// is responsible for mapping this to OpenAI-compatible formats.
///
/// Note: Does NOT include `file_path` to avoid leaking filesystem details
/// in catalog/listing operations.
#[derive(Debug, Clone)]
pub struct ModelSummary {
    /// Database ID of the model.
    pub id: u32,
    /// Model name (used as identifier).
    pub name: String,
    /// Tags/labels associated with the model.
    pub tags: Vec<String>,
    /// Parameter count as string (e.g., "7B", "13B", "70B").
    pub param_count: String,
    /// Quantization type (e.g., "`Q4_K_M`", "`Q8_0`").
    pub quantization: Option<String>,
    /// Model architecture (e.g., "llama", "mistral", "qwen2").
    pub architecture: Option<String>,
    /// Unix timestamp when the model was added.
    pub created_at: i64,
    /// File size in bytes.
    pub file_size: u64,
}

/// Launch specification for running a model.
///
/// Contains all information needed to actually launch a model,
/// including the file path. Separate from `ModelSummary` to avoid
/// leaking filesystem details in catalog operations.
#[derive(Debug, Clone)]
pub struct ModelLaunchSpec {
    /// Database ID of the model.
    pub id: u32,
    /// Model name.
    pub name: String,
    /// Absolute path to the GGUF file.
    pub file_path: PathBuf,
    /// Tags/labels associated with the model.
    pub tags: Vec<String>,
    /// Model architecture (for runtime configuration).
    pub architecture: Option<String>,
    /// Maximum context length the model supports.
    pub context_length: Option<u64>,
}

impl ModelSummary {
    /// Create a description string for this model.
    #[must_use]
    pub fn description(&self) -> String {
        let arch = self.architecture.as_deref().unwrap_or("unknown");
        let quant = self.quantization.as_deref().unwrap_or("unknown");
        format!("{} - {} parameters, {}", arch, self.param_count, quant)
    }
}

/// Errors that can occur during catalog operations.
#[derive(Debug, Error)]
pub enum CatalogError {
    /// Failed to query the catalog.
    #[error("Failed to query catalog: {0}")]
    QueryFailed(String),

    /// Internal error during catalog operations.
    #[error("Internal error: {0}")]
    Internal(String),
}

/// Port for querying the model catalog.
///
/// This interface provides read-only access to the model catalog
/// for listing and resolving models. It does not handle model
/// registration or deletion.
#[async_trait]
pub trait ModelCatalogPort: Send + Sync + fmt::Debug {
    /// List all models in the catalog.
    ///
    /// Returns a list of model summaries ordered by name.
    ///
    /// # Errors
    ///
    /// Returns `CatalogError` if the catalog cannot be queried.
    async fn list_models(&self) -> Result<Vec<ModelSummary>, CatalogError>;

    /// Resolve a model by name or alias.
    ///
    /// This method performs model resolution:
    /// 1. Exact name match
    /// 2. Case-insensitive name match
    /// 3. Fuzzy/partial match (implementation-defined)
    ///
    /// Returns `None` if no matching model is found.
    ///
    /// # Arguments
    ///
    /// * `name` - Model name or alias to resolve
    ///
    /// # Errors
    ///
    /// Returns `CatalogError` if the catalog cannot be queried.
    async fn resolve_model(&self, name: &str) -> Result<Option<ModelSummary>, CatalogError>;

    /// Resolve a model for launching.
    ///
    /// Returns full launch specification including file path.
    /// Use this when you need to actually run a model, not just list it.
    ///
    /// # Arguments
    ///
    /// * `name` - Model name or alias to resolve
    ///
    /// # Errors
    ///
    /// Returns `CatalogError` if the catalog cannot be queried.
    async fn resolve_for_launch(&self, name: &str)
    -> Result<Option<ModelLaunchSpec>, CatalogError>;
}
