#![doc = include_str!(concat!(env!("OUT_DIR"), "/README_GENERATED.md"))]
#![deny(unsafe_code)]
#![deny(unused_crate_dependencies)]

// Silence unused dev-dependency warnings for planned test infrastructure
#[cfg(test)]
use tempfile as _;
#[cfg(test)]
use tokio_test as _;

// Dependencies used by handlers module (will be used as handlers are migrated)
use anyhow as _;
use dotenvy as _;
use hf_hub as _;
use tokio as _;
use tracing as _;
use tracing_subscriber as _;

// gglib-runtime used for process runner in bootstrap
use gglib_runtime as _;

// gglib-axum used for web command in main.rs
use gglib_axum as _;

pub mod assistant_ui_commands;
pub mod bootstrap;
pub mod commands;
pub mod config_commands;
pub mod error;
pub mod handlers;
pub mod llama_commands;
pub mod parser;
pub mod presentation;
pub mod utils;

// Re-export primary types for convenient access
pub use assistant_ui_commands::AssistantUiCommand;
pub use bootstrap::{CliConfig, CliContext, bootstrap};
pub use commands::Commands;
pub use config_commands::{ConfigCommand, ModelsDirCommand, SettingsCommand};
pub use error::CliError;
pub use llama_commands::LlamaCommand;
pub use parser::Cli;
