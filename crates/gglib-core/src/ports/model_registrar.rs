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
    /// Path to the primary downloaded file: the weights, or their first
    /// shard. Never the projector.
    pub primary_path: std::path::PathBuf,
    /// All downloaded file paths: the weights, then the projector.
    pub all_paths: Vec<std::path::PathBuf>,
    /// The projector downloaded with the weights, when the group had one.
    pub projector_path: Option<std::path::PathBuf>,
    /// The resolved quantization.
    pub quantization: Quantization,
    /// Repository ID (e.g., "unsloth/Llama-3-GGUF").
    pub repo_id: String,
    /// Commit SHA at time of download.
    pub commit_sha: String,
    /// Whether the weights were downloaded as several shards.
    pub is_sharded: bool,
    /// Ordered list of the weights shards for sharded models (None for single-file models).
    pub file_paths: Option<Vec<std::path::PathBuf>>,
    /// `HuggingFace` tags for the model.
    pub hf_tags: Vec<String>,
    /// File entries with OIDs from `HuggingFace` (for `model_files` table),
    /// one per file of the group, the projector included.
    pub hf_file_entries: Vec<ResolvedFile>,
}

/// A download registered in the library.
#[derive(Debug, Clone)]
pub struct RegisteredDownload {
    /// The model as stored.
    pub model: Model,
    /// Why the model was stored without the details its file's header holds:
    /// the GGUF reader's own words for refusing the file. `None` when the
    /// reader accepted it.
    pub metadata_refusal: Option<String>,
    /// Why the group's projector was not linked to the model: its header
    /// refused it, or the model keeps a link to another file. `None` when it
    /// is the model's projector, and when the group had no projector.
    pub projector_refusal: Option<String>,
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
/// let model = registrar.register_model(&download).await?.model;
/// ```
#[async_trait]
pub trait ModelRegistrarPort: Send + Sync {
    /// Register a downloaded model in the database.
    ///
    /// Parses GGUF metadata from the downloaded file and creates a database entry.
    /// A file the reader refuses is registered all the same, without the
    /// details its header would have given, and the answer carries the
    /// reader's words.
    /// For sharded models, the primary (first shard) path is used for registration.
    /// A projector downloaded with the weights is linked to the model when its
    /// header says it is one; when it does not, the model is registered
    /// without the link and the answer says why. A model already linked to
    /// another file keeps that link, and the answer says so.
    ///
    /// # Arguments
    ///
    /// * `download` - The completed download information
    ///
    /// # Returns
    ///
    /// Returns the created `Model`, why its metadata was not read and why its
    /// projector was not linked, on success.
    async fn register_model(
        &self,
        download: &CompletedDownload,
    ) -> Result<RegisteredDownload, RepositoryError>;
}
