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
        /// DRY repetition penalty strength (0.0 disables)
        #[arg(long)]
        dry_multiplier: Option<f32>,
        /// DRY penalty base (llama.cpp default 1.75)
        #[arg(long)]
        dry_base: Option<f32>,
        /// Tokens of repeat DRY tolerates before penalising (default 2)
        #[arg(long)]
        dry_allowed_length: Option<i32>,
        /// DRY lookback window in tokens; -1 = whole context
        #[arg(long)]
        dry_penalty_last_n: Option<i32>,
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
        /// Host address `gglib web` binds to (an IP, e.g. 127.0.0.1 or 0.0.0.0).
        /// The `--host` flag overrides this for a single run.
        #[arg(long)]
        bind_host: Option<String>,
        /// Expose `gglib web` on all LAN interfaces and broadcast over mDNS.
        /// WARNING: makes GGLib visible to every device on your network.
        #[arg(long)]
        share_lan: Option<bool>,
        /// Bearer token the proxy requires on /v1/* and /mcp. Clients send it
        /// as `Authorization: Bearer <key>`; /health stays open. Clear it to
        /// go back to an unauthenticated endpoint. The proxy sets this itself
        /// the first time it binds a non-loopback host.
        #[arg(long)]
        proxy_api_key: Option<String>,
        /// Honour a client's own sampling parameters (temperature, top_p,
        /// top_k, presence_penalty, repeat_penalty, min_p). Defaults to
        /// false: most clients (e.g. VS Code Copilot) send fixed sampling
        /// values with no user-facing control behind them, so this server's
        /// own per-model and global defaults apply instead. `max_tokens` is
        /// always honoured regardless of this setting.
        #[arg(long)]
        trust_client_sampling: Option<bool>,
        /// Run the proxy's turn-level loop/stagnation guard on
        /// /v1/chat/completions. Enabled by default: a conversation whose
        /// replayed history repeats the same tool-call batch or assistant
        /// response beyond the agent-path thresholds is rejected with a
        /// clean 400 before any model work. Set false only for a client
        /// that legitimately replays identical batches.
        #[arg(long)]
        proxy_loop_detection: Option<bool>,
        /// Sample tool-emission turns against a tighter floor. Enabled by
        /// default: a request carrying tools decodes near-deterministically
        /// with DRY off, because its output is structured and a malformed
        /// tool call is the hardest failure to recover from. Only ever
        /// supplies a floor, so any layer that names a parameter still wins.
        #[arg(long)]
        tool_call_floor: Option<bool>,
        /// Start the OpenAI-compatible proxy as soon as the desktop app
        /// launches, instead of waiting for it to be switched on. Combined
        /// with --start-at-login and --close-to-tray this keeps the endpoint
        /// permanently available with no terminal held open. Desktop app only;
        /// `gglib proxy` and `gglib serve` remain explicit foreground commands.
        #[arg(long)]
        proxy_autostart: Option<bool>,
        /// Closing the desktop app's window hides it to the system tray
        /// instead of quitting, leaving the proxy serving. Quitting is then an
        /// explicit action from the tray menu.
        #[arg(long)]
        close_to_tray: Option<bool>,
        /// Register the desktop app to launch on login (macOS login item,
        /// Windows Run key, XDG autostart entry on Linux). Applied
        /// immediately, so the stored value and the OS state stay in step.
        #[arg(long)]
        start_at_login: Option<bool>,
    },
    /// Reset all settings to defaults
    Reset {
        /// Skip confirmation prompt
        #[arg(short, long)]
        force: bool,
    },
}
