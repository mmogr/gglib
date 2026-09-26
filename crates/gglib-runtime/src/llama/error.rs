//! Error type for the llama.cpp install prompt.
//!
//! `LlamaError` is the error [`InstallPrompt`] returns: the prompt is a trait,
//! and a trait in a library should not make its implementors depend on
//! `anyhow`. The install, build, download and update orchestration here
//! returns `anyhow::Result`.
//!
//! [`InstallPrompt`]: super::prompt::InstallPrompt

use thiserror::Error;

/// Errors that can occur while asking the user to confirm an install.
#[derive(Debug, Error)]
pub enum LlamaError {
    /// User confirmation was required but not available (non-interactive mode).
    #[error("User confirmation required: {message}")]
    PromptRequired { message: String },

    /// Reading the answer from the terminal failed.
    ///
    /// Constructed only by `?` inside `CliPrompt::confirm`, which exists under
    /// the `cli` feature — hence no explicit `LlamaError::IoError` anywhere.
    #[error("IO error: {0}")]
    IoError(#[from] std::io::Error),
}

impl LlamaError {
    /// Create a `PromptRequired` error with a message
    pub fn prompt_required(message: impl Into<String>) -> Self {
        Self::PromptRequired {
            message: message.into(),
        }
    }
}

/// Result type alias for llama operations
pub type LlamaResult<T> = Result<T, LlamaError>;
