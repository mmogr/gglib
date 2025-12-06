//! Core domain types for the download module.
//!
//! This is a shim that re-exports types from `gglib_core::download`.
//! Infrastructure types that require PathBuf stay here.

// Re-export pure domain types from gglib-core
pub use gglib_core::download::{DownloadId, Quantization, ShardInfo};

use serde::{Deserialize, Serialize};

// ============================================================================
// Infrastructure types (PathBuf, etc.) - stay in legacy
// ============================================================================

/// Request to start a download.
///
/// Contains all parameters needed to initiate a download from HuggingFace Hub.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct DownloadRequest {
    /// The download identifier.
    pub id: DownloadId,
    /// Repository ID on HuggingFace (e.g., "unsloth/Llama-3.2-GGUF").
    pub repo_id: String,
    /// The resolved quantization type.
    pub quantization: Quantization,
    /// Specific files to download.
    pub files: Vec<String>,
    /// Destination directory for downloaded files.
    pub destination: std::path::PathBuf,
    /// Revision/commit SHA (defaults to "main").
    pub revision: Option<String>,
    /// Force re-download even if file exists locally.
    pub force: bool,
    /// Add to local model database after download.
    pub add_to_db: bool,
    /// HuggingFace authentication token (for private repos).
    pub token: Option<String>,
}

impl DownloadRequest {
    /// Create a builder for constructing a download request.
    pub fn builder() -> DownloadRequestBuilder {
        DownloadRequestBuilder::default()
    }

    /// Create a simple download request (for testing).
    pub fn new(id: DownloadId) -> Self {
        Self {
            repo_id: id.model_id().to_string(),
            quantization: Quantization::Unknown,
            files: Vec::new(),
            destination: std::path::PathBuf::new(),
            revision: None,
            id,
            force: false,
            add_to_db: true,
            token: None,
        }
    }
}

/// Builder for DownloadRequest.
#[derive(Default)]
pub struct DownloadRequestBuilder {
    id: Option<DownloadId>,
    repo_id: Option<String>,
    quantization: Option<Quantization>,
    files: Vec<String>,
    destination: Option<std::path::PathBuf>,
    revision: Option<String>,
    force: bool,
    add_to_db: bool,
    token: Option<String>,
}

impl DownloadRequestBuilder {
    /// Set the download ID.
    pub fn id(mut self, id: DownloadId) -> Self {
        self.id = Some(id);
        self
    }

    /// Set the repository ID.
    pub fn repo_id(mut self, repo_id: impl Into<String>) -> Self {
        self.repo_id = Some(repo_id.into());
        self
    }

    /// Set the quantization type.
    pub fn quantization(mut self, quantization: Quantization) -> Self {
        self.quantization = Some(quantization);
        self
    }

    /// Set the files to download.
    pub fn files(mut self, files: Vec<String>) -> Self {
        self.files = files;
        self
    }

    /// Set the destination directory.
    pub fn destination(mut self, destination: impl Into<std::path::PathBuf>) -> Self {
        self.destination = Some(destination.into());
        self
    }

    /// Set the revision/commit.
    pub fn revision(mut self, revision: impl Into<String>) -> Self {
        self.revision = Some(revision.into());
        self
    }

    /// Set whether to force re-download.
    pub fn force(mut self, force: bool) -> Self {
        self.force = force;
        self
    }

    /// Set whether to add to database.
    pub fn add_to_db(mut self, add_to_db: bool) -> Self {
        self.add_to_db = add_to_db;
        self
    }

    /// Set the authentication token.
    pub fn token(mut self, token: Option<String>) -> Self {
        self.token = token;
        self
    }

    /// Build the request.
    pub fn build(self) -> DownloadRequest {
        let id = self.id.expect("id is required");
        DownloadRequest {
            repo_id: self.repo_id.unwrap_or_else(|| id.model_id().to_string()),
            quantization: self.quantization.unwrap_or(Quantization::Unknown),
            files: self.files,
            destination: self.destination.unwrap_or_default(),
            revision: self.revision,
            id,
            force: self.force,
            add_to_db: self.add_to_db,
            token: self.token,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_download_id_display() {
        let id = DownloadId::new("unsloth/Llama-3", Some("Q4_K_M"));
        assert_eq!(id.to_string(), "unsloth/Llama-3:Q4_K_M");

        let id_no_quant = DownloadId::from_model("owner/repo");
        assert_eq!(id_no_quant.to_string(), "owner/repo");
    }

    #[test]
    fn test_download_id_parse() {
        let id: DownloadId = "unsloth/Llama-3:Q4_K_M".parse().unwrap();
        assert_eq!(id.model_id(), "unsloth/Llama-3");
        assert_eq!(id.quantization(), Some("Q4_K_M"));

        let id_no_quant: DownloadId = "owner/repo".parse().unwrap();
        assert_eq!(id_no_quant.model_id(), "owner/repo");
        assert_eq!(id_no_quant.quantization(), None);
    }

    #[test]
    fn test_quantization_from_filename() {
        assert_eq!(
            Quantization::from_filename("model-Q4_K_M.gguf"),
            Quantization::Q4KM
        );
        assert_eq!(
            Quantization::from_filename("some-model-IQ3_XS-v2.gguf"),
            Quantization::Iq3Xs
        );
    }

    #[test]
    fn test_download_request_builder() {
        let id = DownloadId::new("test/model", Some("Q4_K_M"));
        let request = DownloadRequest::builder()
            .id(id.clone())
            .repo_id("test/model")
            .quantization(Quantization::Q4KM)
            .destination("/tmp/test")
            .force(true)
            .build();

        assert_eq!(request.id, id);
        assert_eq!(request.repo_id, "test/model");
        assert!(request.force);
    }
}
