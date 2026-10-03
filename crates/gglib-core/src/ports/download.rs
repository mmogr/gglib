//! Download port definitions (trait abstractions).
//!
//! This module contains trait definitions for download-related operations
//! that abstract away infrastructure concerns.

use async_trait::async_trait;
use serde::{Deserialize, Serialize};

use crate::download::{DownloadError, GgufFileRole, Quantization};

// ============================================================================
// Resolution Types
// ============================================================================

/// Result of resolving files for a quantization: the download group.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Resolution {
    /// The resolved quantization type.
    pub quantization: Quantization,
    /// The files to download. The weights come first, in shard order, and
    /// the projector fetched with them, when there is one, is last.
    pub files: Vec<ResolvedFile>,
    /// Whether the weights are sharded (multi-part). A projector is not a
    /// shard.
    pub is_sharded: bool,
}

impl Resolution {
    /// Get total size of every file, if all file sizes are known.
    pub fn total_size(&self) -> Option<u64> {
        let sizes: Option<Vec<u64>> = self.files.iter().map(|f| f.size).collect();
        sizes.map(|s| s.iter().sum())
    }

    /// Get the number of files, the projector included.
    pub const fn file_count(&self) -> usize {
        self.files.len()
    }

    /// The weights files, in shard order.
    pub fn weights(&self) -> impl Iterator<Item = &ResolvedFile> {
        self.files.iter().filter(|f| !f.role.is_projector())
    }

    /// The number of weights shards (1 for a single-file model).
    pub fn shard_count(&self) -> usize {
        self.weights().count()
    }

    /// The projector fetched with the weights, when the repository has one.
    pub fn projector(&self) -> Option<&ResolvedFile> {
        self.files.iter().find(|f| f.role.is_projector())
    }
}

/// A single resolved file.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ResolvedFile {
    /// Path within the repository.
    pub path: String,
    /// Size in bytes (if available from API).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub size: Option<u64>,
    /// Git LFS OID (SHA256 hash from `HuggingFace` tree API).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub oid: Option<String>,
    /// What the file is: weights, or the projector fetched with them.
    pub role: GgufFileRole,
}

impl ResolvedFile {
    /// Create a new resolved weights file.
    pub fn new(path: impl Into<String>) -> Self {
        Self {
            path: path.into(),
            size: None,
            oid: None,
            role: GgufFileRole::Weights,
        }
    }

    /// Create a new resolved weights file with size.
    pub fn with_size(path: impl Into<String>, size: u64) -> Self {
        Self {
            size: Some(size),
            ..Self::new(path)
        }
    }

    /// Create a new resolved weights file with size and OID.
    pub fn with_size_and_oid(path: impl Into<String>, size: u64, oid: Option<String>) -> Self {
        Self {
            oid,
            ..Self::with_size(path, size)
        }
    }

    /// Create a resolved projector file with size and OID.
    pub fn projector(path: impl Into<String>, size: u64, oid: Option<String>) -> Self {
        Self {
            role: GgufFileRole::Projector,
            ..Self::with_size_and_oid(path, size, oid)
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
    }

    #[test]
    fn test_resolved_file_creation() {
        let file = ResolvedFile::new("test.gguf");
        assert_eq!(file.path, "test.gguf");
        assert_eq!(file.size, None);

        let file_with_size = ResolvedFile::with_size("test.gguf", 1024);
        assert_eq!(file_with_size.size, Some(1024));
        assert_eq!(file_with_size.role, GgufFileRole::Weights);
    }

    /// Shards are counted among the weights; sizes and the file count cover
    /// the projector too.
    #[test]
    fn a_projector_is_a_file_of_the_group_and_not_a_shard() {
        let resolution = Resolution {
            quantization: Quantization::Q8_0,
            files: vec![
                ResolvedFile::with_size("model-Q8_0.gguf", 1000),
                ResolvedFile::projector("mmproj-F16.gguf", 300, None),
            ],
            is_sharded: false,
        };

        assert_eq!(resolution.shard_count(), 1);
        assert_eq!(resolution.file_count(), 2);
        assert_eq!(resolution.total_size(), Some(1300));
        assert_eq!(
            resolution
                .weights()
                .map(|f| f.path.as_str())
                .collect::<Vec<_>>(),
            ["model-Q8_0.gguf"]
        );
        assert_eq!(
            resolution.projector().map(|f| f.path.as_str()),
            Some("mmproj-F16.gguf")
        );
    }

    #[test]
    fn a_group_without_a_projector_has_none() {
        let resolution = Resolution {
            quantization: Quantization::Q8_0,
            files: vec![ResolvedFile::with_size("model-Q8_0.gguf", 1000)],
            is_sharded: false,
        };

        assert_eq!(resolution.projector(), None);
        assert_eq!(resolution.shard_count(), 1);
    }
}
