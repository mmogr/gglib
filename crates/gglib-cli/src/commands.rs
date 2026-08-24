//! Main commands enum and primary subcommands.
//!
//! This module defines the top-level commands for the CLI tool.
//! Model management lives under [`ModelCommand`] and configuration
//! under [`ConfigCommand`]; inference commands stay top-level for
//! ergonomic direct access.

use clap::Subcommand;
use clap_complete::Shell;

use crate::benchmark_commands::BenchmarkCommand;
use crate::config_commands::ConfigCommand;
use crate::mcp_commands::McpCommand;
use crate::model_commands::ModelCommand;
use crate::profile_args::ProfileArgs;
use crate::shared_args::{
    AccessArgs, CacheArgs, ContextArgs, MtpArgs, RetryArgs, SamplingArgs, ServeOptions,
};
pub(crate) use crate::subcommands::{ChatCommand, DaemonCommand, ProxyCommand};

/// Top-level commands for the GGUF library management tool.
#[derive(Subcommand)]
pub enum Commands {
    // ── Getting started ──────────────────────────────────────────────────
    /// Go from nothing to a working OpenAI-compatible endpoint in one command
    ///
    /// Detects your hardware, installs llama.cpp if it is missing, recommends
    /// and downloads a model that actually fits, starts the proxy, sends a
    /// real request through it to prove the endpoint works, and prints the
    /// configuration to paste into your client.
    ///
    /// Safe to re-run: anything already present is skipped, so a second run is
    /// just "start the proxy".
    ///
    /// Deliberately unconfigurable beyond the flags below — it binds loopback
    /// with gglib's defaults. Reach for `gglib proxy` when you want to choose
    /// the host, the upstream port, sampling or cache behaviour.
    #[command(display_order = 0)]
    Up {
        /// Download the recommended model without asking
        #[arg(long, short = 'y')]
        yes: bool,
        /// Load this model instead of the recommended (or most recent) one
        #[arg(long)]
        model: Option<String>,
        /// Port to bind the endpoint to
        #[arg(short, long, default_value = "8080")]
        port: u16,
    },

    // ── Management (these have subcommands — use `<command> --help`) ────
    /// Manage GGUF models (add, list, remove, download, verify, …)
    #[command(display_order = 1)]
    Model {
        #[command(subcommand)]
        command: ModelCommand,
    },

    /// Manage configuration, tooling, and system settings
    #[command(display_order = 2)]
    Config {
        #[command(subcommand)]
        command: ConfigCommand,
    },

    /// Manage MCP (Model Context Protocol) tool servers
    #[command(display_order = 3)]
    Mcp {
        #[command(subcommand)]
        command: McpCommand,
    },

    // ── Inference ────────────────────────────────────────────────────────
    /// Serve one pinned model on an OpenAI-compatible endpoint
    ///
    /// Runs the same proxy stack as `gglib proxy`, pinned to a single model:
    /// requests naming any other model are refused rather than swapped. That
    /// makes it the right choice for clients that cannot switch models via
    /// `/v1/models` — VS Code Copilot's BYOK endpoint, for one.
    ///
    /// Being the proxy, it also brings the dashboard, KV cache reuse,
    /// prompt-cache auto-sizing, request normalization and health monitoring.
    ///
    /// `--port` is the endpoint clients connect to; the daemon allocates the
    /// upstream llama-server port. `--host` defaults to loopback; widening it
    /// generates an API key and prints it, and any host clients reach the
    /// endpoint by must be named with `--allowed-host`.
    #[command(display_order = 10)]
    Serve {
        /// Name or ID of the model to serve, optionally with a ':profile' suffix
        identifier: String,
        #[command(flatten)]
        context: ContextArgs,
        #[command(flatten)]
        options: ServeOptions,
        #[command(flatten)]
        sampling: SamplingArgs,
        #[command(flatten)]
        profile: ProfileArgs,
        #[command(flatten)]
        mtp: MtpArgs,
        #[command(flatten)]
        cache: CacheArgs,
        #[command(flatten)]
        access: AccessArgs,
    },

    /// Chat with a model interactively, or manage chat history
    #[command(display_order = 11, subcommand_negates_reqs = true)]
    Chat {
        /// Name or ID of the model to chat with (optional when resuming with --continue)
        #[arg(default_value = "")]
        identifier: String,
        #[command(flatten)]
        context: ContextArgs,
        /// Set a system prompt for the conversation
        #[arg(long = "system-prompt", short = 's')]
        system_prompt: Option<String>,
        #[command(flatten)]
        sampling: SamplingArgs,
        #[command(flatten)]
        profile: ProfileArgs,
        #[command(flatten)]
        retry: RetryArgs,
        /// Disable tool access (plain LLM chat without filesystem or MCP tools)
        #[arg(long = "no-tools")]
        no_tools: bool,
        /// Reuse an already-running llama-server on this port (skips auto-start)
        #[arg(long)]
        port: Option<u16>,
        /// Maximum agent iterations before giving up
        /// [default: persisted setting, or 25 if unset]
        #[arg(long = "max-iterations")]
        max_iterations: Option<usize>,
        /// Tool allowlist; may be repeated or comma-separated.
        /// Omit to allow all tools. (e.g. "mcp_search,builtin_time")
        /// Note: the filter is evaluated once at session start. To change the
        /// available tools mid-session, exit and restart with a new --tools list.
        #[arg(long, value_delimiter = ',')]
        tools: Vec<String>,
        /// Per-tool execution timeout in milliseconds
        #[arg(long = "tool-timeout-ms")]
        tool_timeout_ms: Option<u64>,
        /// Maximum number of tools executed in parallel per iteration
        #[arg(long = "max-parallel")]
        max_parallel: Option<usize>,
        /// Model name forwarded to llama-server (uses server default when omitted)
        #[arg(long)]
        model: Option<String>,
        /// Resume a previous conversation by ID (use `gglib chat history` to find IDs)
        #[arg(long = "continue")]
        continue_id: Option<i64>,
        /// Observation-only tool name patterns for the dual-threshold loop guard.
        /// A tool whose name ends with or contains any pattern is classified as
        /// observation-only and subject to the higher --max-observation-steps limit.
        /// Omit to use the built-in defaults (snapshot, screenshot, read_page).
        /// Pass an empty string to disable observation classification entirely.
        #[arg(long = "observation-tool", value_delimiter = ',')]
        observation_tools: Vec<String>,
        /// Maximum times an observation-only batch may repeat before loop detection
        /// fires. Clamped to 100. Defaults to 10.
        #[arg(long = "max-observation-steps")]
        max_observation_steps: Option<usize>,
        /// Subcommand (e.g. `history`)
        #[command(subcommand)]
        command: Option<ChatCommand>,
    },

    /// Ask a question with optional context from stdin or file
    #[command(
        display_order = 12,
        alias = "q",
        after_help = "EXAMPLES:\n    gglib q \"What is Rust?\"\n    cat file.txt | gglib q \"Summarize this\"\n    gglib q --file README.md \"Explain this project\"\n    echo \"Paris, Tokyo\" | gglib q \"List these cities: {}\"\n    gglib q \"How is error handling done in this project?\"\n    cat file.rs | gglib q \"Explain this code in depth\""
    )]
    Question {
        /// Question to ask (use {} as placeholder for piped/file input)
        question: String,
        /// Model ID or name (uses default model if not specified)
        #[arg(short, long)]
        model: Option<String>,
        /// Read context from file instead of stdin
        #[arg(short, long)]
        file: Option<String>,
        #[command(flatten)]
        context: ContextArgs,
        /// Show the constructed prompt before sending
        #[arg(long)]
        verbose: bool,
        /// Cleaner output for scripting (no tool progress, no reasoning tokens)
        #[arg(long, short = 'Q')]
        quiet: bool,
        #[command(flatten)]
        sampling: SamplingArgs,
        #[command(flatten)]
        profile: ProfileArgs,
        /// Disable tool access (plain LLM question without filesystem or MCP tools)
        #[arg(long = "no-tools")]
        no_tools: bool,
        /// Port of a running llama-server to reuse (skips auto-start)
        #[arg(long)]
        port: Option<u16>,
        /// Maximum agent iterations
        /// [default: persisted setting, or 25 if unset]
        #[arg(long = "max-iterations")]
        max_iterations: Option<usize>,
        /// Tool allowlist (empty = all tools)
        #[arg(long, value_delimiter = ',')]
        tools: Vec<String>,
        /// Per-tool execution timeout in milliseconds
        #[arg(long = "tool-timeout-ms")]
        tool_timeout_ms: Option<u64>,
        /// Maximum number of tools executed in parallel per iteration
        #[arg(long = "max-parallel")]
        max_parallel: Option<usize>,
        /// Observation-only tool name patterns for the dual-threshold loop guard.
        /// A tool whose name ends with or contains any pattern is classified as
        /// observation-only and subject to the higher --max-observation-steps limit.
        /// Omit to use the built-in defaults (snapshot, screenshot, read_page).
        #[arg(long = "observation-tool", value_delimiter = ',')]
        observation_tools: Vec<String>,
        /// Maximum times an observation-only batch may repeat before loop detection
        /// fires. Clamped to 100. Defaults to 10.
        #[arg(long = "max-observation-steps")]
        max_observation_steps: Option<usize>,
    },
    /// Run benchmark comparisons and performance tests across local models
    ///
    /// Compare outputs side-by-side (same prompt through N models) or measure
    /// raw prompt-processing and token-generation throughput with llama-bench.
    #[command(display_order = 13)]
    Benchmark {
        #[command(subcommand)]
        command: BenchmarkCommand,
    },

    // ── Interfaces ──────────────────────────────────────────────────────
    /// Launch the Tauri desktop GUI
    #[command(display_order = 20)]
    Gui {
        /// Run in development mode with hot-reload (requires Node.js and npm)
        #[arg(long)]
        dev: bool,
    },

    /// Open the web dashboard (served by the gglib daemon)
    ///
    /// Ensures the daemon is running and prints the dashboard URL. The
    /// daemon serves the UI and the API from one fixed loopback port; to
    /// expose it on the network, run `gglib daemon run --share-lan` in the
    /// foreground instead.
    #[command(display_order = 21)]
    Web {
        /// Shorthand for `gglib daemon run --share-lan` (foreground)
        #[arg(long)]
        share_lan: bool,
    },

    /// Manage the gglib daemon — the process that owns llama-server
    #[command(display_order = 23)]
    Daemon {
        #[command(subcommand)]
        command: DaemonCommand,
    },

    /// Generate shell completion scripts (bash, zsh, fish, elvish, powershell)
    ///
    /// Prints a completion script to stdout. Pipe it into your shell's config:
    ///
    ///   gglib completions fish > ~/.config/fish/completions/gglib.fish
    ///   gglib completions bash > ~/.bash_completion
    ///   gglib completions zsh  > ~/.zsh/_gglib
    #[command(display_order = 30)]
    Completions {
        /// Shell to generate completions for
        #[arg(value_enum)]
        shell: Shell,
    },

    /// Start OpenAI-compatible proxy with MCP tool gateway
    ///
    /// Serves /v1 chat completions and /mcp (MCP Streamable HTTP) from a single port.
    /// Configure OpenWebUI with the /v1 base URL and connect MCP tools via /mcp.
    ///
    /// When a request arrives for a model that is not yet running, the proxy
    /// auto-starts a llama-server and automatically enables the appropriate
    /// flags based on the model's capability tags:
    ///
    /// - `"mtp"` tag  → MTP speculative decoding (--spec-type draft-mtp)
    /// - `"reasoning"` tag → reasoning format extraction (--reasoning-format)
    /// - `"agent"` tag → Jinja template support (--jinja)
    ///
    /// This is identical to the behaviour when starting a model from the GUI or
    /// CLI — all surfaces go through the same canonical config builder.
    ///
    /// The sampling flags apply as proxy-wide defaults: requests that don't
    /// specify a value use them, while per-request and per-model values still
    /// win. (On `gglib serve` the same flags ride on the pinned model's own
    /// launch options instead, since there is exactly one model in scope.)
    #[command(display_order = 22)]
    Proxy {
        /// Host to bind to
        #[arg(long, default_value = "127.0.0.1")]
        host: String,
        /// Port to bind the proxy to
        #[arg(short, long, default_value = "8080")]
        port: u16,
        /// Default context size when not specified by client.
        /// Falls back to the app settings `default_context_size`, then to the
        /// compiled default (4096) if unset. Must be a positive number — `max`
        /// is not supported here since no specific model is in scope for a
        /// standalone proxy.
        #[arg(long)]
        default_context: Option<String>,

        #[command(flatten)]
        sampling: SamplingArgs,
        #[command(flatten)]
        cache: CacheArgs,
        #[command(flatten)]
        access: AccessArgs,
        /// Subcommand (e.g. `dashboard`)
        #[command(subcommand)]
        command: Option<ProxyCommand>,
    },
}
