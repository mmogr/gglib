#![doc = include_str!("README.md")]
//!
//! This module defines commands for managing application settings,
//! models directory, llama.cpp toolchain, system
//! dependency checks, and resolved path inspection.

use clap::Subcommand;

mod settings_args;

pub use settings_args::SettingsSetArgs;

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
    /// Check system dependencies required for gglib
    CheckDeps {
        /// Superseded by `gglib config fast-downloads enable`.
        ///
        /// Kept working because it is what the older docs and release notes
        /// tell people to run.
        #[arg(long, hide = true)]
        setup_fast_downloads: bool,
    },
    /// Enable or inspect the optional hf_xet download accelerator
    FastDownloads {
        #[command(subcommand)]
        command: Option<FastDownloadsCommand>,
    },
    /// Show resolved paths for all gglib directories
    Paths,
}

/// Fast-download accelerator command variants.
///
/// The accelerator is a Python environment gglib builds and owns under its own
/// data directory. It does not require, and is not affected by, whichever
/// Python or environment manager the user works with elsewhere.
#[derive(Subcommand)]
pub enum FastDownloadsCommand {
    /// Show whether the accelerator is provisioned, and where (default)
    Status,
    /// Build the accelerator's environment
    Enable {
        /// Use this Python interpreter instead of searching for one
        #[arg(long, value_name = "PATH")]
        python: Option<String>,
    },
    /// Remove the accelerator's environment; downloads revert to native HTTP
    Disable,
    /// Offer to enable it, interactively. Skips silently without a terminal
    Prompt,
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
        /// Frequency penalty (−2.0–2.0); scales with how often a token
        /// already appeared, 0.0 disables (llama.cpp default 0.0)
        #[arg(long)]
        frequency_penalty: Option<f32>,
        /// DRY repetition penalty strength (0.0 disables)
        #[arg(long)]
        dry_multiplier: Option<f32>,
        /// DRY penalty base (llama.cpp default 1.75)
        #[arg(long)]
        dry_base: Option<f32>,
        /// Tokens of repeat DRY tolerates before penalising (default 2)
        #[arg(long)]
        dry_allowed_length: Option<i32>,
        /// DRY lookback window in tokens; 0 disables (llama.cpp default 64)
        #[arg(long)]
        dry_penalty_last_n: Option<i32>,
        /// Dynamic-temperature half-range; 0.0 disables (llama.cpp default 0.0)
        #[arg(long)]
        dynatemp_range: Option<f32>,
        /// Dynamic-temperature exponent; inert without a range (llama.cpp default 1.0)
        #[arg(long)]
        dynatemp_exponent: Option<f32>,
        /// Top-n-sigma logit truncation; -1.0 disables (llama.cpp default -1.0)
        #[arg(long)]
        top_n_sigma: Option<f32>,
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
    Set(Box<SettingsSetArgs>),
    /// Reset all settings to defaults
    Reset {
        /// Skip confirmation prompt
        #[arg(short, long)]
        force: bool,
    },
}
