//! Python runner trait and implementation.
//!
//! This trait abstracts Python subprocess execution for testing.

use std::path::PathBuf;

use async_trait::async_trait;

/// Context for running a Python download script.
#[derive(Debug, Clone)]
pub struct PythonContext {
    /// Repository ID to download from.
    pub repo_id: String,
    /// Files to download.
    pub files: Vec<String>,
    /// Destination directory.
    pub destination: PathBuf,
    /// HuggingFace token (if needed).
    pub token: Option<String>,
}

/// Result of a Python script execution.
#[derive(Debug)]
pub enum PythonResult {
    /// Download completed successfully.
    Completed {
        /// Paths to downloaded files.
        paths: Vec<PathBuf>,
        /// Total bytes downloaded.
        total_bytes: u64,
    },
    /// Download was cancelled.
    Cancelled,
    /// Download failed.
    Failed {
        /// Error message.
        error: String,
    },
}

/// Trait for running Python download scripts.
///
/// This trait is internal to `gglib-download` and allows mocking
/// Python execution in tests.
#[async_trait]
pub trait PythonRunner: Send + Sync {
    /// Run a Python download script.
    async fn run(&self, ctx: PythonContext) -> PythonResult;

    /// Check if Python environment is available.
    async fn is_available(&self) -> bool;
}

/// Stub implementation for initial crate setup.
/// TODO: Replace with real implementation from src/download/executor/python.rs
pub struct StubPythonRunner;

#[async_trait]
impl PythonRunner for StubPythonRunner {
    async fn run(&self, _ctx: PythonContext) -> PythonResult {
        PythonResult::Failed {
            error: "Python runner not implemented".to_string(),
        }
    }

    async fn is_available(&self) -> bool {
        false
    }
}
