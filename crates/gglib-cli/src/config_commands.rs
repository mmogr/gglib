//! Configuration, tooling, and system management subcommands.
//!
//! This module defines commands for managing application settings,
//! models directory, llama.cpp toolchain, assistant-ui, system
//! dependency checks, and resolved path inspection.

use clap::Subcommand;

use crate::assistant_ui_commands::AssistantUiCommand;
use crate::llama_commands::LlamaCommand;

/// Configuration and system management commands.
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
    /// Manage named sampling profiles, selectable as `<model>:<profile>`
    Profile {
        #[command(subcommand)]
        command: ProfileCommand,
    },
    /// Manage llama.cpp installation and updates
    Llama {
        #[command(subcommand)]
        command: LlamaCommand,
    },
    /// Manage assistant-ui installation and updates
    AssistantUi {
        #[command(subcommand)]
        command: AssistantUiCommand,
    },
    /// Check system dependencies required for gglib
    CheckDeps,
    /// Show resolved paths for all gglib directories
    Paths,
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

/// Inference profile command variants.
///
/// Profiles are global: one `coding` profile applies to every model, and a
/// client selects it per request by asking for `<model>:<profile>`.
#[derive(Subcommand)]
pub enum ProfileCommand {
    /// List all configured profiles
    List,
    /// Show one profile's full configuration
    Show {
        /// Profile name
        name: String,
    },
    /// Create or update a profile.
    ///
    /// Only the flags you pass are set; the rest stay unset and fall through
    /// to the model's own defaults. Updating an existing profile merges the
    /// flags you pass over what is stored — use `--unset` to clear one.
    Set {
        /// Profile name (lowercase letters, digits and '-')
        name: String,
        /// Human-readable description, shown in the model picker
        #[arg(long)]
        description: Option<String>,
        /// Sampling temperature (0.0–2.0)
        #[arg(long)]
        temperature: Option<f32>,
        /// Nucleus sampling top-p (0.0–1.0)
        #[arg(long)]
        top_p: Option<f32>,
        /// Top-k sampling limit
        #[arg(long)]
        top_k: Option<i32>,
        /// Maximum tokens to generate
        #[arg(long)]
        max_tokens: Option<u32>,
        /// Repetition penalty (typically 1.0–1.3)
        #[arg(long)]
        repeat_penalty: Option<f32>,
        /// Presence penalty (0.0–2.0)
        #[arg(long)]
        presence_penalty: Option<f32>,
        /// Min-p sampling threshold (0.0–1.0)
        #[arg(long)]
        min_p: Option<f32>,
        /// Clear a parameter so it falls back to the model's own default.
        /// Repeatable, e.g. `--unset top-k --unset min-p`.
        #[arg(long, value_name = "PARAM")]
        unset: Vec<String>,
        /// Advertise `<model>:<name>` in /v1/models so clients can pick it
        #[arg(long)]
        list_in_models: bool,
        /// Stop advertising this profile in /v1/models
        #[arg(long, conflicts_with = "list_in_models")]
        no_list_in_models: bool,
    },
    /// Delete a profile
    Rm {
        /// Profile name
        name: String,
    },
    /// Install the built-in starter profiles (coding, chat, creative)
    InstallTemplates {
        /// Overwrite profiles that already exist with the same name
        #[arg(long)]
        force: bool,
    },
}

/// Settings command variants.
#[derive(Subcommand)]
pub enum SettingsCommand {
    /// Show all current application settings
    Show,
    /// Update application settings
    Set {
        /// Default context size for models (512-1000000).
        /// Global fallback (level 3 of 4); per-model server_defaults and runtime flags take precedence.
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
        /// Maximum agent iterations for tool-calling loop (1-50)
        #[arg(long)]
        max_tool_iterations: Option<u32>,
        /// Maximum stagnation steps before stopping agent loop
        #[arg(long)]
        max_stagnation_steps: Option<u32>,
        /// Show memory fit indicators in HuggingFace browser
        #[arg(long)]
        show_memory_fit_indicators: Option<bool>,
    },
    /// Reset all settings to defaults
    Reset {
        /// Skip confirmation prompt
        #[arg(short, long)]
        force: bool,
    },
}
