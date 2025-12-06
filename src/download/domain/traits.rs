//! Domain traits for download operations.
//!
//! This module re-exports pure traits from gglib-core and keeps
//! infrastructure-dependent traits (DownloadExecutor) locally.

use std::path::Path;

use async_trait::async_trait;
use tokio_util::sync::CancellationToken;

use super::errors::DownloadError;
use super::events::DownloadEvent;

// Re-export pure resolution types and trait from gglib-core
pub use gglib_core::ports::{QuantizationResolver, ResolvedFile, Resolution};

// ============================================================================
// Executor Trait (infrastructure - stays here due to CancellationToken, Path)
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
