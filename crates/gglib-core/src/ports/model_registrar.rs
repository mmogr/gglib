//! Model registrar port definition.
//!
//! This port defines the interface for registering downloaded models
//! in the database. It breaks the circular dependency between download
//! and core services by allowing the download crate to depend on a trait
//! rather than concrete `AppCore`.

use async_trait::async_trait;
use std::path::Path;

use super::RepositoryError;
use super::download::ResolvedFile;
use crate::domain::Model;
use crate::download::Quantization;

/// Information about a completed download for model registration.
///
/// This is a pure data transfer object containing all information
/// needed to register a model after download completes.
#[derive(Debug, Clone)]
pub struct CompletedDownload {
    /// Path to the primary downloaded file (first shard for sharded models).
    pub primary_path: std::path::PathBuf,
    /// All downloaded file paths (multiple for sharded models).
    pub all_paths: Vec<std::path::PathBuf>,
    /// The resolved quantization.
    pub quantization: Quantization,
    /// Repository ID (e.g., "unsloth/Llama-3-GGUF").
    pub repo_id: String,
    /// Commit SHA at time of download.
    pub commit_sha: String,
    /// Whether this was a sharded download.
    pub is_sharded: bool,
    /// Total bytes downloaded.
    pub total_bytes: u64,
    /// Ordered list of all file paths for sharded models (None for single-file models).
    pub file_paths: Option<Vec<std::path::PathBuf>>,
    /// `HuggingFace` tags for the model.
    pub hf_tags: Vec<String>,
    /// File entries with OIDs from HuggingFace (for model_files table).
    pub hf_file_entries: Vec<ResolvedFile>,
}

impl CompletedDownload {
    /// Get the primary file path for database registration.
    ///
    /// For sharded models, this returns the first shard path
    /// (required by llama-server for loading split models).
    pub fn db_path(&self) -> &Path {
        &self.primary_path
    }
}

/// Port for registering downloaded models in the database.
///
/// This trait is implemented by core services and injected into
/// the download manager, allowing model registration without
/// coupling to `AppCore` directly.
///
/// # Usage
///
/// ```ignore
/// let registrar: Arc<dyn ModelRegistrarPort> = /* ... */;
/// let download = CompletedDownload { ... };
/// let model = registrar.register_model(&download).await?;
/// ```
#[async_trait]
pub trait ModelRegistrarPort: Send + Sync {
    /// Register a downloaded model in the database.
    ///
    /// Parses GGUF metadata from the downloaded file and creates a database entry.
    /// For sharded models, the primary (first shard) path is used for registration.
    ///
    /// # Arguments
    ///
    /// * `download` - The completed download information
    ///
    /// # Returns
    ///
    /// Returns the created `Model` on success.
    async fn register_model(&self, download: &CompletedDownload) -> Result<Model, RepositoryError>;

    /// Register a model using raw path parameters.
    ///
    /// This is a simpler interface for cases where you have the file path
    /// but not the full download metadata.
    ///
    /// # Arguments
    ///
    /// * `repo_id` - `HuggingFace` repository ID
    /// * `commit_sha` - Git commit SHA
    /// * `file_path` - Path to the GGUF file
    /// * `quantization` - Quantization type as string
    ///
    /// # Returns
    ///
    /// Returns the created `Model` on success.
    async fn register_model_from_path(
        &self,
        repo_id: &str,
        commit_sha: &str,
        file_path: &Path,
        quantization: &str,
    ) -> Result<Model, RepositoryError>;
}
