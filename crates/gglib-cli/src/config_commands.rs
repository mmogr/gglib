//! Configuration management subcommands.
//!
//! This module defines the configuration-related commands for managing
//! models directory and application settings.

use clap::Subcommand;

/// Configuration management commands.
#[derive(Subcommand)]
pub enum ConfigCommand {
    /// View or set the default model (shorthand for settings get/set-default-model)
    Default {
        /// Model ID or name to set as default (omit to show current)
        identifier: Option<String>,
        /// Clear the current default model
        #[arg(long)]
        clear: bool,
    },
    /// View or change the models directory preference
    ModelsDir {
        #[command(subcommand)]
        command: ModelsDirCommand,
    },
    /// View or change application settings (context size, ports, queue size, etc.)
    Settings {
        #[command(subcommand)]
        command: SettingsCommand,
    },
}

/// Models directory command variants.
#[derive(Subcommand)]
pub enum ModelsDirCommand {
    /// Show the currently configured models directory
    Show,
    /// Prompt the user for a directory (Enter to keep default)
    Prompt,
    /// Set the directory explicitly (non-interactive)
    Set {
        /// Path to the directory where GGUF models should be stored
        path: String,
        /// Fail if the directory does not exist (default creates it)
        #[arg(long)]
        no_create: bool,
    },
}

/// Settings command variants.
#[derive(Subcommand)]
pub enum SettingsCommand {
    /// Show all current application settings
    Show,
    /// Update application settings
    Set {
        /// Default context size for models (512-1000000)
        #[arg(long)]
        default_context_size: Option<u64>,
        /// Port for the OpenAI-compatible proxy server (>= 1024)
        #[arg(long)]
        proxy_port: Option<u16>,
        /// Base port for llama-server instances (>= 1024)
        #[arg(long)]
        llama_base_port: Option<u16>,
        /// Maximum number of downloads that can be queued (1-50)
        #[arg(long)]
        max_download_queue_size: Option<u32>,
        /// Default download path for models
        #[arg(long)]
        default_download_path: Option<String>,
    },
    /// Reset all settings to defaults
    Reset {
        /// Skip confirmation prompt
        #[arg(short, long)]
        force: bool,
    },
    /// Set the default model for commands that support it
    SetDefaultModel {
        /// Model ID or name to set as default
        identifier: String,
    },
    /// Show the current default model
    GetDefaultModel,
}
