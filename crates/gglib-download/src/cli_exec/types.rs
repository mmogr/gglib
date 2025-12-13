//! CLI download execution types.
//!
//! These types define the interface between CLI handlers and the download
//! execution layer. They are intentionally simple and free of dependencies
//! on `AppCore`, clap, or other adapter-specific types.

use std::path::PathBuf;

/// Request to download a model from `HuggingFace`.
#[derive(Debug, Clone)]
pub struct CliDownloadRequest {
    /// `HuggingFace` model ID (e.g., "unsloth/Llama-3-GGUF").
    pub model_id: String,
    /// Specific quantization to download (e.g., "`Q4_K_M`").
    pub quantization: Option<String>,
    /// Directory where models are stored.
    pub models_dir: PathBuf,
    /// Force re-download even if file exists.
    pub force: bool,
    /// `HuggingFace` API token (for private repos).
    pub token: Option<String>,
}

impl CliDownloadRequest {
    /// Create a new download request.
    pub fn new(model_id: impl Into<String>, models_dir: PathBuf) -> Self {
        Self {
            model_id: model_id.into(),
            quantization: None,
            models_dir,
            force: false,
            token: None,
        }
    }

    /// Set the quantization to download.
    #[must_use]
    pub fn with_quantization(mut self, quant: impl Into<String>) -> Self {
        self.quantization = Some(quant.into());
        self
    }

    /// Set whether to force re-download.
    #[must_use]
    pub const fn with_force(mut self, force: bool) -> Self {
        self.force = force;
        self
    }

    /// Set the `HuggingFace` token.
    #[must_use]
    pub fn with_token(mut self, token: Option<String>) -> Self {
        self.token = token;
        self
    }
}

/// Result of a successful download.
#[derive(Debug, Clone)]
pub struct CliDownloadResult {
    /// All downloaded file paths (for sharded models, multiple files).
    pub downloaded_paths: Vec<PathBuf>,
    /// Primary model file path (first shard or single file).
    pub primary_path: PathBuf,
    /// Resolved quantization name.
    pub quantization: String,
    /// `HuggingFace` repository ID.
    pub repo_id: String,
    /// Git commit SHA of the downloaded version.
    pub commit_sha: String,
}

/// Request to update a downloaded model.
#[derive(Debug, Clone)]
pub struct CliUpdateRequest {
    /// Path to the existing model file.
    pub model_path: PathBuf,
    /// `HuggingFace` repository ID.
    pub repo_id: String,
    /// Quantization to download.
    pub quantization: String,
    /// Directory where models are stored.
    pub models_dir: PathBuf,
    /// `HuggingFace` API token.
    pub token: Option<String>,
}

/// Information about available model updates.
#[derive(Debug, Clone)]
pub struct UpdateCheckResult {
    /// Whether an update is available.
    pub has_update: bool,
    /// Current local commit SHA (if known).
    pub current_sha: Option<String>,
    /// Latest remote commit SHA.
    pub latest_sha: String,
}
