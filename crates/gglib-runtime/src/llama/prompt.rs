//! User prompt abstraction for llama.cpp installation operations.
//!
//! This module provides a trait-based prompt system that allows CLI and GUI
//! adapters to handle user confirmations without coupling to specific I/O.
//!
//! # Feature Flags
//!
//! - `cli`: Enables `CliPrompt` which uses stdin/stdout for interactive prompts.
//!   Without this feature, only `NonInteractivePrompt` is available.
//!
//! # Design
//!
//! The default `NonInteractivePrompt` returns `Err(LlamaError::PromptRequired)`
//! rather than silently auto-confirming. This is intentional:
//! - Safe for GUI/HTTP contexts where silent confirmation would be surprising
//! - Forces callers to explicitly handle the prompt requirement
//! - Prevents accidental installations in automated contexts

use super::error::{LlamaError, LlamaResult};

/// Trait for handling user prompts during installation operations.
///
/// Implementors can show dialogs, prompt on stdin, or return errors
/// for non-interactive contexts.
pub trait InstallPrompt: Send + Sync {
    /// Ask the user to confirm an action.
    ///
    /// # Arguments
    /// * `message` - The question to ask (e.g., "Install llama.cpp now?")
    /// * `default` - The default answer if user just presses Enter
    ///
    /// # Returns
    /// - `Ok(true)` if user confirmed
    /// - `Ok(false)` if user declined
    /// - `Err(LlamaError::PromptRequired)` if prompting is not supported
    fn confirm(&self, message: &str, default: bool) -> LlamaResult<bool>;

    /// Display an informational message to the user.
    fn info(&self, message: &str);

    /// Display a warning message to the user.
    fn warn(&self, message: &str);
}

/// Non-interactive prompt that returns errors instead of prompting.
///
/// Use this in contexts where user interaction is not available (HTTP APIs,
/// background services, tests). All confirmation requests return
/// `Err(LlamaError::PromptRequired)` with the original message.
#[derive(Debug, Default, Clone, Copy)]
pub struct NonInteractivePrompt;

impl InstallPrompt for NonInteractivePrompt {
    fn confirm(&self, message: &str, _default: bool) -> LlamaResult<bool> {
        Err(LlamaError::prompt_required(message))
    }

    fn info(&self, _message: &str) {
        // Silent in non-interactive mode
    }

    fn warn(&self, _message: &str) {
        // Silent in non-interactive mode
    }
}

/// Auto-confirm prompt that always returns true.
///
/// Use this for automated contexts where you want to proceed without
/// user interaction (e.g., CI/CD, scripted installations with --yes flag).
///
/// **Warning**: This will auto-confirm all prompts. Use with caution.
#[derive(Debug, Default, Clone, Copy)]
pub struct AutoConfirmPrompt;

impl InstallPrompt for AutoConfirmPrompt {
    fn confirm(&self, _message: &str, _default: bool) -> LlamaResult<bool> {
        Ok(true)
    }

    fn info(&self, message: &str) {
        println!("{}", message);
    }

    fn warn(&self, message: &str) {
        eprintln!("Warning: {}", message);
    }
}

/// CLI prompt using stdin/stdout for interactive confirmation.
///
/// This is only available with the `cli` feature flag.
#[cfg(feature = "cli")]
pub mod cli_prompt {
    use super::*;
    use std::io::{self, BufRead, Write};

    /// CLI prompt that reads from stdin.
    #[derive(Debug, Default)]
    pub struct CliPrompt;

    impl CliPrompt {
        /// Create a new CLI prompt.
        pub fn new() -> Self {
            Self
        }
    }

    impl InstallPrompt for CliPrompt {
        fn confirm(&self, message: &str, default: bool) -> LlamaResult<bool> {
            let prompt_suffix = if default { "[Y/n]" } else { "[y/N]" };
            print!("{} {}: ", message, prompt_suffix);
            io::stdout().flush()?;

            let stdin = io::stdin();
            let mut input = String::new();
            stdin.lock().read_line(&mut input)?;

            let trimmed = input.trim().to_lowercase();
            if trimmed.is_empty() {
                Ok(default)
            } else if trimmed == "y" || trimmed == "yes" {
                Ok(true)
            } else if trimmed == "n" || trimmed == "no" {
                Ok(false)
            } else {
                // Treat unknown input as default
                Ok(default)
            }
        }

        fn info(&self, message: &str) {
            println!("{}", message);
        }

        fn warn(&self, message: &str) {
            eprintln!("⚠️  {}", message);
        }
    }
}

#[cfg(feature = "cli")]
pub use cli_prompt::CliPrompt;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_non_interactive_returns_error() {
        let prompt = NonInteractivePrompt;
        let result = prompt.confirm("Install?", true);
        assert!(result.is_err());

        match result {
            Err(LlamaError::PromptRequired { message }) => {
                assert_eq!(message, "Install?");
            }
            _ => panic!("Expected PromptRequired error"),
        }
    }

    #[test]
    fn test_auto_confirm_returns_true() {
        let prompt = AutoConfirmPrompt;
        assert!(prompt.confirm("Install?", false).unwrap());
        assert!(prompt.confirm("Delete?", true).unwrap());
    }
}
