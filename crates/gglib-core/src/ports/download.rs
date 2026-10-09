//! Download port definitions (trait abstractions).
//!
//! This module contains trait definitions for download-related operations
//! that abstract away infrastructure concerns.

use async_trait::async_trait;
use serde::{Deserialize, Serialize};

use crate::domain::{ComponentRole, ImageFamily};
use crate::download::{DownloadError, GgufFileRole, Quantization};

// ============================================================================
// Resolution Types
// ============================================================================

/// Result of resolving files for a quantization: the download group.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Resolution {
    /// The resolved quantization type.
    pub quantization: Quantization,
    /// The files to download. The weights come first, in shard order, then
    /// the projector fetched with them, when there is one, and last an image
    /// model's companions, each from its own repository.
    pub files: Vec<ResolvedFile>,
    /// Whether the weights are sharded (multi-part). A projector or a
    /// companion is not a shard.
    pub is_sharded: bool,
    /// The image family the weights' head names, when they are an image
    /// model's; `None` for a chat model and whenever the head was not read.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub image_family: Option<ImageFamily>,
}

impl Resolution {
    /// Get total size of every file, if all file sizes are known.
    pub fn total_size(&self) -> Option<u64> {
        let sizes: Option<Vec<u64>> = self.files.iter().map(|f| f.size).collect();
        sizes.map(|s| s.iter().sum())
    }

    /// Get the number of files, the projector and companions included.
    pub const fn file_count(&self) -> usize {
        self.files.len()
    }

    /// The weights files, in shard order.
    pub fn weights(&self) -> impl Iterator<Item = &ResolvedFile> {
        self.files.iter().filter(|f| f.is_weights())
    }

    /// An image model's companions: the files its family draws with beside
    /// its weights, each fetched from its own repository.
    pub fn components(&self) -> impl Iterator<Item = &ResolvedFile> {
        self.files.iter().filter(|f| f.component.is_some())
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
    /// What the file is: weights, or the projector fetched with them. A
    /// companion keeps the default and is told apart by `component`.
    pub role: GgufFileRole,
    /// The repository the file is fetched from, when it is not the group's
    /// own: an image model's companion. `None` for the group's own files.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub repo: Option<String>,
    /// The role the file plays for an image model, when it is one of its
    /// companions. A companion is neither weights nor a projector.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub component: Option<ComponentRole>,
}

impl ResolvedFile {
    /// Create a new resolved weights file.
    pub fn new(path: impl Into<String>) -> Self {
        Self {
            path: path.into(),
            size: None,
            oid: None,
            role: GgufFileRole::Weights,
            repo: None,
            component: None,
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

    /// Create a resolved companion: the file at `path` in `repo` that an
    /// image model draws with in `component`.
    pub fn companion(
        component: ComponentRole,
        repo: impl Into<String>,
        path: impl Into<String>,
        size: u64,
        oid: Option<String>,
    ) -> Self {
        Self {
            repo: Some(repo.into()),
            component: Some(component),
            ..Self::with_size_and_oid(path, size, oid)
        }
    }

    /// Whether this is one of the model's weights files: neither its
    /// projector nor a companion.
    pub const fn is_weights(&self) -> bool {
        !self.role.is_projector() && self.component.is_none()
    }

    /// The repository the file is fetched from: its own, or `group_repo`.
    pub fn repo_or<'a>(&'a self, group_repo: &'a str) -> &'a str {
        self.repo.as_deref().unwrap_or(group_repo)
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
            image_family: None,
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
            image_family: None,
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

    /// A companion is a file of the group, in its size and its count, and
    /// neither a shard of the weights nor the projector.
    #[test]
    fn a_companion_is_a_file_of_the_group_and_not_a_shard() {
        let resolution = Resolution {
            quantization: Quantization::Q8_0,
            files: vec![
                ResolvedFile::with_size("flux1-schnell-q8_0.gguf", 1000),
                ResolvedFile::companion(
                    ComponentRole::Vae,
                    "unsloth/FLUX.1-schnell",
                    "ae.safetensors",
                    300,
                    None,
                ),
            ],
            is_sharded: false,
            image_family: Some(ImageFamily::Flux1),
        };

        assert_eq!(resolution.shard_count(), 1);
        assert_eq!(resolution.file_count(), 2);
        assert_eq!(resolution.total_size(), Some(1300));
        assert_eq!(resolution.projector(), None);
        let companions: Vec<_> = resolution.components().collect();
        assert_eq!(companions.len(), 1);
        assert_eq!(companions[0].component, Some(ComponentRole::Vae));
        assert_eq!(
            companions[0].repo_or("leejet/FLUX.1-schnell-gguf"),
            "unsloth/FLUX.1-schnell"
        );
        assert!(!companions[0].is_weights());
        assert_eq!(
            resolution.files[0].repo_or("leejet/FLUX.1-schnell-gguf"),
            "leejet/FLUX.1-schnell-gguf"
        );
    }

    #[test]
    fn a_group_without_a_projector_has_none() {
        let resolution = Resolution {
            quantization: Quantization::Q8_0,
            files: vec![ResolvedFile::with_size("model-Q8_0.gguf", 1000)],
            is_sharded: false,
            image_family: None,
        };

        assert_eq!(resolution.projector(), None);
        assert_eq!(resolution.shard_count(), 1);
    }
}
