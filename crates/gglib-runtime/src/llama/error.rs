//! Error types for llama.cpp management.
//!
//! This module provides a unified error type for all llama-related operations,
//! keeping error plumbing out of orchestration modules.

use std::path::PathBuf;
use thiserror::Error;

/// Errors that can occur during llama.cpp management operations.
#[derive(Debug, Error)]
pub enum LlamaError {
    // === Installation & Detection ===
    /// Llama binaries are not installed
    #[error("llama.cpp is not installed. Run 'gglib llama install' to install it.")]
    NotInstalled,

    /// Binary exists but is not functional
    #[error("llama.cpp binary is corrupted or not executable: {path}")]
    BinaryCorrupted { path: PathBuf },

    /// Required build dependencies are missing
    #[error("Missing build dependencies: {missing}")]
    MissingDependencies { missing: String },

    // === Download ===
    /// Pre-built binaries not available for this platform
    #[error("Pre-built binaries not available: {reason}")]
    PrebuiltNotAvailable { reason: String },

    /// Failed to fetch release information from GitHub
    #[error("Failed to fetch release from GitHub: {0}")]
    ReleaseFetchFailed(String),

    /// Failed to download file
    #[error("Download failed: {0}")]
    DownloadFailed(String),

    /// Failed to extract archive
    #[error("Failed to extract archive: {0}")]
    ExtractionFailed(String),

    // === Build ===
    /// `CMake` configuration failed
    #[error("CMake configuration failed: {0}")]
    CmakeFailed(String),

    /// Build/compilation failed
    #[error("Build failed: {0}")]
    BuildFailed(String),

    /// CUDA/compiler compatibility issue
    #[error("CUDA/compiler compatibility issue: {0}")]
    CudaCompatibility(String),

    // === Prompt ===
    /// User confirmation was required but not available (non-interactive mode)
    #[error("User confirmation required: {message}")]
    PromptRequired { message: String },

    /// User cancelled the operation
    #[error("Operation cancelled by user")]
    Cancelled,

    // === Path & IO ===
    /// Path resolution failed
    #[error("Path error: {0}")]
    PathError(#[from] gglib_core::paths::PathError),

    /// IO operation failed
    #[error("IO error: {0}")]
    IoError(#[from] std::io::Error),

    // === Other ===
    /// Generic error with context
    #[error("{0}")]
    Other(String),
}

impl LlamaError {
    /// Create a `PromptRequired` error with a message
    pub fn prompt_required(message: impl Into<String>) -> Self {
        Self::PromptRequired {
            message: message.into(),
        }
    }

    /// Create an Other error from any error type
    pub fn other(err: impl std::fmt::Display) -> Self {
        Self::Other(err.to_string())
    }
}

/// Result type alias for llama operations
pub type LlamaResult<T> = Result<T, LlamaError>;
