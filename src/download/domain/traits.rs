//! Domain traits for download operations.
//!
//! These traits define the contracts for resolution and execution,
//! allowing orchestration code to depend on interfaces rather than implementations.

use std::path::Path;

use async_trait::async_trait;
use tokio_util::sync::CancellationToken;

use super::errors::DownloadError;
use super::events::DownloadEvent;
use super::types::Quantization;

// ============================================================================
// Resolution Types
// ============================================================================

/// Result of resolving files for a quantization.
#[derive(Debug, Clone)]
pub struct Resolution {
    /// The resolved quantization type.
    pub quantization: Quantization,
    /// List of files to download (sorted for sharded files).
    pub files: Vec<ResolvedFile>,
    /// Whether this is a sharded (multi-part) download.
    pub is_sharded: bool,
}

impl Resolution {
    /// Get filenames as a simple list.
    pub fn filenames(&self) -> Vec<String> {
        self.files.iter().map(|f| f.path.clone()).collect()
    }

    /// Get total size if all file sizes are known.
    pub fn total_size(&self) -> Option<u64> {
        let sizes: Option<Vec<u64>> = self.files.iter().map(|f| f.size).collect();
        sizes.map(|s| s.iter().sum())
    }

    /// Get the first file path (used for database registration of sharded models).
    pub fn first_file(&self) -> Option<&str> {
        self.files.first().map(|f| f.path.as_str())
    }
}

/// A single resolved file.
#[derive(Debug, Clone)]
pub struct ResolvedFile {
    /// Path within the repository.
    pub path: String,
    /// Size in bytes (if available from API).
    pub size: Option<u64>,
}

// ============================================================================
// Resolver Trait
// ============================================================================

/// Trait for resolving quantization-specific files from a model repository.
///
/// Implementations handle the specifics of querying APIs (HuggingFace, etc.)
/// to find GGUF files matching a requested quantization.
#[async_trait]
pub trait QuantizationResolver: Send + Sync {
    /// Resolve files for a specific quantization.
    ///
    /// Returns a `Resolution` containing the list of files to download
    /// and metadata about the resolution.
    async fn resolve(
        &self,
        repo_id: &str,
        quantization: Quantization,
    ) -> Result<Resolution, DownloadError>;

    /// List all available quantizations in a repository.
    async fn list_available(&self, repo_id: &str) -> Result<Vec<Quantization>, DownloadError>;
}

// ============================================================================
// Executor Trait
// ============================================================================

/// Callback for receiving download events during execution.
pub type EventCallback = std::sync::Arc<dyn Fn(DownloadEvent) + Send + Sync>;

/// Result of a download execution.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ExecutionResult {
    /// Download completed successfully.
    Completed,
    /// Download was cancelled.
    Cancelled,
}

/// Parameters for executing a download.
#[derive(Clone)]
pub struct ExecuteParams<'a> {
    /// Repository identifier (e.g., "unsloth/Llama-3-GGUF").
    pub repo_id: &'a str,
    /// List of file paths to download.
    pub files: &'a [String],
    /// Local directory to download files into.
    pub destination: &'a Path,
    /// Git revision/commit SHA.
    pub revision: &'a str,
    /// Optional authentication token.
    pub token: Option<&'a str>,
    /// Whether to overwrite existing files.
    pub force: bool,
    /// Token for cancellation.
    pub cancel_token: CancellationToken,
}

/// Trait for executing file downloads.
///
/// Implementations handle the mechanics of downloading files from
/// a repository to a local destination.
#[async_trait]
pub trait DownloadExecutor: Send + Sync {
    /// Execute a download of the specified files.
    async fn execute(
        &self,
        params: ExecuteParams<'_>,
        on_event: &EventCallback,
    ) -> Result<ExecutionResult, DownloadError>;

    /// Prepare the execution environment (e.g., set up Python venv).
    ///
    /// Call this during app startup to avoid first-download latency.
    async fn prepare(&self) -> Result<(), DownloadError> {
        Ok(())
    }
}
