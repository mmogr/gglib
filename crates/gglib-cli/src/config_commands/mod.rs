#![doc = include_str!("README.md")]

use clap::Subcommand;

mod secret_arg;
mod setting_key;

pub(crate) use secret_arg::SecretArg;
#[cfg(test)]
pub(crate) use setting_key::Bound;
pub(crate) use setting_key::SettingKey;
pub(crate) use setting_key::Takes;

use crate::llama_commands::LlamaCommand;
use crate::sd_commands::SdCommand;

/// Configuration and system management commands.
#[derive(Subcommand)]
pub enum ConfigCommand {
    /// Show every setting and its value
    Show,
    /// Set one setting, by the key `show` prints
    ///
    /// The value is read by the key's type: a whole number, `true` or
    /// `false`, text, a comma-separated list for `proxy-allowed-hosts`, or a
    /// model's id or name for `default-model-id` and
    /// `default-image-model-id`. A value the setting does not allow is
    /// refused and nothing is stored.
    Set {
        /// The setting, as `show` prints it
        key: SettingKey,
        /// Its new value
        #[arg(allow_hyphen_values = true)]
        value: String,
    },
    /// Clear one setting, returning it to unset
    ///
    /// Unset is not the same as zero or a default: for `default-context-size`
    /// it is what lets each launch size the context to this machine, and
    /// `set` can only ever write a value. For `models-dir` it removes the
    /// stored folder, so the default applies.
    Unset {
        /// The setting, as `show` prints it
        key: SettingKey,
    },
    /// Reset every preference to its default
    ///
    /// Keeps the machine this one joined, the devices it admits, whether and
    /// how it serves remote access, and the proxy's API key.
    Reset {
        /// Answer yes to the confirmation, so nothing is asked
        #[arg(short = 'y', long)]
        yes: bool,
    },
    /// Print a stored secret, alone on stdout
    ///
    /// The one place a stored secret is printed: `show` says only whether it
    /// is held. Stdout carries the value and nothing else, so
    /// `$(gglib config reveal proxy-api-key)` is exactly the key.
    Reveal {
        /// The secret, as `show` names it
        key: SecretArg,
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
    /// Manage stable-diffusion.cpp, the runtime that serves image models
    Sd {
        #[command(subcommand)]
        command: SdCommand,
    },
    /// Check system dependencies required for gglib
    CheckDeps,
    /// Enable or inspect the optional `hf_xet` download accelerator
    FastDownloads {
        #[command(subcommand)]
        command: Option<FastDownloadsCommand>,
    },
    /// Show resolved paths for all gglib directories, and every gglib on PATH and the desktop app
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
        /// Reasoning effort: minimal, low, medium, high, xhigh, max.
        ///
        /// Applies only to models whose chat template declares that it reads
        /// the variable. There is no `none` — set
        /// `--reasoning-budget-tokens 0` to stop thinking.
        #[arg(long, value_parser = crate::reasoning_args::parse_effort)]
        reasoning_effort: Option<gglib_core::domain::ReasoningEffort>,
        /// Thinking-token ceiling; -1 defers to the launch default, 0 stops
        /// thinking. Enforced by llama.cpp on every model.
        // `allow_hyphen_values` so the documented `-1` parses; see `SamplingArgs`.
        #[arg(long, allow_hyphen_values = true, value_parser = crate::reasoning_args::parse_budget)]
        reasoning_budget_tokens: Option<i32>,
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
    Remove {
        /// Profile name
        name: String,
    },
    /// Install the built-in starter profiles.
    ///
    /// Three shape the distribution (coding, chat, creative) and six are the
    /// rungs of the reasoning ladder (minimal … max); `low`, `high` and `max`
    /// are the ones advertised in /v1/models.
    InstallTemplates {
        /// Overwrite profiles that already exist with the same name
        #[arg(long)]
        force: bool,
    },
}
