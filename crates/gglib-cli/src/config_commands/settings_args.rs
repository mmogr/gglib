//! The argument list for `gglib config settings set`.
//!
//! Split out of `config_commands.rs` because it is a third of the file on its
//! own and grows with every setting — one flag, one doc comment, one `#[arg]`
//! each. Keeping it here means adding a setting touches a file about settings
//! rather than the file that enumerates every config subcommand.

use clap::Args;

/// Every field `gglib config settings set` can write.
///
/// A named struct rather than inline variant fields so the handler can take
/// one argument instead of destructuring eighteen, and so the list of what is
/// settable lives in one place — `scripts/check_settings_surfaces.sh` reads it
/// to prove no `Settings` field is stranded without a surface.
///
/// Boxed at the call site: clap variants are sized by their largest, and a
/// dozen `Option<String>`s would make every `SettingsCommand` that big.
#[derive(Args)]
pub struct SettingsSetArgs {
    /// Default context size for models (512-1000000).
    /// Global fallback (level 3 of 4); per-model server_defaults and runtime flags take precedence.
    #[arg(long)]
    pub default_context_size: Option<u64>,
    /// Port for the OpenAI-compatible proxy server (>= 1024)
    #[arg(long)]
    pub proxy_port: Option<u16>,
    /// Base port for llama-server instances (>= 1024)
    #[arg(long)]
    pub llama_base_port: Option<u16>,
    /// Maximum number of downloads that can be queued (1-50)
    #[arg(long)]
    pub max_download_queue_size: Option<u32>,
    /// Default download path for models
    #[arg(long)]
    pub default_download_path: Option<String>,
    /// Maximum agent iterations for tool-calling loop (1-50)
    #[arg(long)]
    pub max_tool_iterations: Option<u32>,
    /// Maximum stagnation steps before stopping agent loop
    #[arg(long)]
    pub max_stagnation_steps: Option<u32>,
    /// Show memory fit indicators in HuggingFace browser
    #[arg(long)]
    pub show_memory_fit_indicators: Option<bool>,
    /// Host address `gglib web` binds to (an IP, e.g. 127.0.0.1 or 0.0.0.0).
    /// The `--host` flag overrides this for a single run.
    #[arg(long)]
    pub bind_host: Option<String>,
    /// Expose `gglib web` on all LAN interfaces and broadcast over mDNS.
    /// WARNING: makes GGLib visible to every device on your network.
    #[arg(long)]
    pub share_lan: Option<bool>,
    /// Bearer token the proxy requires on /v1/* and /mcp. Clients send it
    /// as `Authorization: Bearer <key>`; /health stays open. Clear it to
    /// go back to an unauthenticated endpoint. The proxy sets this itself
    /// the first time it binds a non-loopback host.
    #[arg(long)]
    pub proxy_api_key: Option<String>,
    /// Honour a client's own sampling parameters (temperature, top_p,
    /// top_k, presence_penalty, repeat_penalty, min_p). Defaults to
    /// false: most clients (e.g. VS Code Copilot) send fixed sampling
    /// values with no user-facing control behind them, so this server's
    /// own per-model and global defaults apply instead. `max_tokens` is
    /// always honoured regardless of this setting.
    #[arg(long)]
    pub trust_client_sampling: Option<bool>,
    /// Run the proxy's turn-level loop/stagnation guard on
    /// /v1/chat/completions. Enabled by default: a conversation whose
    /// replayed history repeats the same tool-call batch or assistant
    /// response beyond the agent-path thresholds is rejected with a
    /// clean 400 before any model work. Set false only for a client
    /// that legitimately replays identical batches.
    #[arg(long)]
    pub proxy_loop_detection: Option<bool>,
    /// Cap the temperature on agentic turns. Enabled by default: a
    /// request carrying tools may emit structured output, so its
    /// temperature is capped — but only over a value nobody chose (an
    /// auto-detected recipe or the floor). Anything you set stands.
    #[arg(long)]
    pub agentic_sampling: Option<bool>,
    /// Re-issue a malformed tool call with `tool_choice: "required"`.
    /// Enabled by default: a call that fails schema validation is
    /// attempted once more with llama.cpp's own grammar made non-lazy,
    /// which repairs most packaging failures instead of forwarding a
    /// broken turn. Set false when measuring a model's raw behaviour.
    #[arg(long)]
    pub tool_call_repair: Option<bool>,
    /// Start the OpenAI-compatible proxy as soon as the desktop app
    /// launches, instead of waiting for it to be switched on. Combined
    /// with --start-at-login and --close-to-tray this keeps the endpoint
    /// permanently available with no terminal held open. Desktop app only;
    /// `gglib proxy` and `gglib serve` remain explicit foreground commands.
    #[arg(long)]
    pub proxy_autostart: Option<bool>,
    /// Closing the desktop app's window hides it to the system tray
    /// instead of quitting, leaving the proxy serving. Quitting is then an
    /// explicit action from the tray menu.
    #[arg(long)]
    pub close_to_tray: Option<bool>,
    /// Register the desktop app to launch on login (macOS login item,
    /// Windows Run key, XDG autostart entry on Linux). Applied
    /// immediately, so the stored value and the OS state stay in step.
    #[arg(long)]
    pub start_at_login: Option<bool>,
}
