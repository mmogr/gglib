//! Error type for the llama.cpp install prompt.
//!
//! This was once "a unified error type for all llama-related operations", with
//! variant groups for installation, download and build. That design lost to
//! `anyhow`: every orchestration module in this directory — `download`,
//! `build`, `install`, `deps`, `detect`, `update`, `uninstall`, `validate`,
//! `status`, `config`, `ensure` — returns `anyhow::Result`, and had for long
//! enough that thirteen of the fifteen variants were never constructed once.
//!
//! What is left is the one place that wants a *typed* error: [`InstallPrompt`]
//! is a trait, and a trait in a library should not make its implementors
//! depend on `anyhow`.
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
