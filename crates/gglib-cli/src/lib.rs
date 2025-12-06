//! CLI interface for gglib.
//!
//! This crate provides the command-line interface for gglib, handling
//! argument parsing, command dispatch, and user interaction.
//!
//! # Architecture
//!
//! The CLI is organized into distinct modules:
//! - `parser` - Main CLI struct with global options
//! - `commands` - Primary command enum
//! - `llama_commands` - llama.cpp management subcommands
//! - `config_commands` - Configuration management subcommands
//! - `assistant_ui_commands` - assistant-ui management subcommands
//! - `error` - CLI-specific error types with exit codes

#![deny(unsafe_code)]
#![deny(unused_crate_dependencies)]

// Silence unused dev-dependency warnings for planned test infrastructure
#[cfg(test)]
use tempfile as _;
#[cfg(test)]
use tokio_test as _;

// gglib-db will be used by command handlers as they are migrated
use gglib_db as _;

pub mod assistant_ui_commands;
pub mod commands;
pub mod config_commands;
pub mod error;
pub mod llama_commands;
pub mod parser;

// Re-export primary types for convenient access
pub use commands::Commands;
pub use config_commands::{ConfigCommand, ModelsDirCommand, SettingsCommand};
pub use error::CliError;
pub use llama_commands::LlamaCommand;
pub use parser::Cli;
