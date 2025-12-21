//! Download port definitions (trait abstractions).
//!
//! This module contains trait definitions for download-related operations
//! that abstract away infrastructure concerns.

use async_trait::async_trait;
use serde::{Deserialize, Serialize};

use crate::download::{DownloadError, Quantization};

// ============================================================================
// Resolution Types
// ============================================================================

/// Result of resolving files for a quantization.
#[derive(Debug, Clone, Serialize, Deserialize)]
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

    /// Get the number of files.
    pub const fn file_count(&self) -> usize {
        self.files.len()
    }
}

/// A single resolved file.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResolvedFile {
    /// Path within the repository.
    pub path: String,
    /// Size in bytes (if available from API).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub size: Option<u64>,
}

impl ResolvedFile {
    /// Create a new resolved file.
    pub fn new(path: impl Into<String>) -> Self {
        Self {
            path: path.into(),
            size: None,
        }
    }

    /// Create a new resolved file with size.
    pub fn with_size(path: impl Into<String>, size: u64) -> Self {
        Self {
            path: path.into(),
            size: Some(size),
        }
    }
}

// ============================================================================
// Resolver Trait
// ============================================================================

/// Trait for resolving quantization-specific files from a model repository.
///
/// Implementations handle the specifics of querying APIs (`HuggingFace`, etc.)
/// to find GGUF files matching a requested quantization.
///
/// # Usage
///
/// ```ignore
/// let resolver: Arc<dyn QuantizationResolver> = /* ... */;
/// let resolution = resolver.resolve("unsloth/Llama-3-GGUF", Quantization::Q4KM).await?;
/// println!("Found {} files", resolution.file_count());
/// ```
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_resolution_methods() {
        let resolution = Resolution {
            quantization: Quantization::Q4KM,
            files: vec![
                ResolvedFile::with_size("model.gguf", 1000),
                ResolvedFile::with_size("model-00001-of-00002.gguf", 500),
            ],
            is_sharded: true,
        };

        assert_eq!(resolution.file_count(), 2);
        assert_eq!(resolution.total_size(), Some(1500));
        assert_eq!(resolution.first_file(), Some("model.gguf"));
        assert_eq!(
            resolution.filenames(),
            vec!["model.gguf", "model-00001-of-00002.gguf"]
        );
    }

    #[test]
    fn test_resolved_file_creation() {
        let file = ResolvedFile::new("test.gguf");
        assert_eq!(file.path, "test.gguf");
        assert_eq!(file.size, None);

        let file_with_size = ResolvedFile::with_size("test.gguf", 1024);
        assert_eq!(file_with_size.size, Some(1024));
    }
}
