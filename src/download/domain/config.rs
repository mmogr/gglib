//! Configuration types for download operations.
//!
//! These types are shared across multiple download modules and represent
//! the stable configuration interface.

use std::path::{Path, PathBuf};

use tokio_util::sync::CancellationToken;

use crate::services::core::PidStorage;

use super::types::Quantization;

/// Configuration for a download operation.
///
/// This is the primary input type for initiating a download.
#[derive(Debug, Clone)]
pub struct DownloadConfig {
    /// Repository ID on HuggingFace (e.g., "unsloth/Llama-3-GGUF").
    pub repo_id: String,
    /// The quantization to download.
    pub quantization: Quantization,
    /// Destination directory for downloaded files.
    pub destination: PathBuf,
    /// Git revision/commit SHA (defaults to "main").
    pub revision: String,
    /// Force re-download even if file exists locally.
    pub force: bool,
    /// Add to local model database after download.
    pub add_to_db: bool,
    /// HuggingFace authentication token (for private repos).
    pub token: Option<String>,
}

impl DownloadConfig {
    /// Create a new download configuration.
    pub fn new(repo_id: impl Into<String>, quantization: Quantization, destination: PathBuf) -> Self {
        Self {
            repo_id: repo_id.into(),
            quantization,
            destination,
            revision: "main".to_string(),
            force: false,
            add_to_db: true,
            token: None,
        }
    }

    /// Set the revision/commit SHA.
    pub fn with_revision(mut self, revision: impl Into<String>) -> Self {
        self.revision = revision.into();
        self
    }

    /// Set the force flag.
    pub fn with_force(mut self, force: bool) -> Self {
        self.force = force;
        self
    }

    /// Set the add_to_db flag.
    pub fn with_add_to_db(mut self, add_to_db: bool) -> Self {
        self.add_to_db = add_to_db;
        self
    }

    /// Set the authentication token.
    pub fn with_token(mut self, token: Option<String>) -> Self {
        self.token = token;
        self
    }
}

/// Session-specific options shared across download operations.
///
/// Contains runtime state like cancellation tokens and progress callbacks.
#[derive(Default, Clone)]
pub struct SessionOptions {
    /// Cancellation token for external cancellation (GUI/service layer).
    pub cancel_token: Option<CancellationToken>,
    /// Optional PID storage for synchronous process termination on app shutdown.
    pub pid_storage: Option<PidStorage>,
    /// Key used to identify this download in the PID storage.
    pub pid_key: Option<String>,
}

impl SessionOptions {
    /// Create new session options with a cancellation token.
    pub fn with_cancel_token(cancel_token: CancellationToken) -> Self {
        Self {
            cancel_token: Some(cancel_token),
            ..Default::default()
        }
    }

    /// Set PID storage for process tracking.
    pub fn with_pid_storage(mut self, storage: PidStorage, key: String) -> Self {
        self.pid_storage = Some(storage);
        self.pid_key = Some(key);
        self
    }
}

/// Result of a successful download, containing paths and metadata.
#[derive(Debug, Clone)]
pub struct DownloadResult {
    /// Path to the primary downloaded file (first shard for sharded models).
    pub primary_path: PathBuf,
    /// All downloaded file paths (multiple for sharded models).
    pub all_paths: Vec<PathBuf>,
    /// The resolved quantization.
    pub quantization: Quantization,
    /// Repository ID.
    pub repo_id: String,
    /// Commit SHA at time of download.
    pub commit_sha: String,
    /// Whether this was a sharded download.
    pub is_sharded: bool,
    /// Total bytes downloaded.
    pub total_bytes: u64,
}

impl DownloadResult {
    /// Get the primary file path for database registration.
    ///
    /// For sharded models, this returns the first shard path
    /// (required by llama-server for loading split models).
    pub fn db_path(&self) -> &Path {
        &self.primary_path
    }
}
