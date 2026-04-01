//! Main commands enum and primary subcommands.
//!
//! This module defines the top-level commands for the CLI tool.
//! Model management lives under [`ModelCommand`] and configuration
//! under [`ConfigCommand`]; inference commands stay top-level for
//! ergonomic direct access.

use clap::Subcommand;

use crate::config_commands::ConfigCommand;
use crate::mcp_commands::McpCommand;
use crate::model_commands::ModelCommand;
use crate::shared_args::{ContextArgs, SamplingArgs};

/// Top-level commands for the GGUF library management tool.
#[derive(Subcommand)]
pub enum Commands {
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
    /// Serve a GGUF model with llama-server
    #[command(display_order = 10)]
    Serve {
        /// ID of the model to serve
        id: u32,
        #[command(flatten)]
        context: ContextArgs,
        /// Force-enable Jinja template parsing for chat templates
        #[arg(long)]
        jinja: bool,
        /// Port to serve on
        #[arg(short, long, default_value = "8080")]
        port: u16,
        #[command(flatten)]
        sampling: SamplingArgs,
    },

    /// Chat with a model directly via llama-cli
    #[command(display_order = 11)]
    Chat {
        /// Name or ID of the model to chat with
        identifier: String,
        #[command(flatten)]
        context: ContextArgs,
        /// Override the chat template name bundled with llama-cli
        #[arg(long = "chat-template")]
        chat_template: Option<String>,
        /// Provide a custom chat template file path
        #[arg(long = "chat-template-file")]
        chat_template_file: Option<String>,
        /// Force-enable Jinja template parsing for custom templates
        #[arg(long)]
        jinja: bool,
        /// Set a system prompt for the conversation
        #[arg(long = "system-prompt", short = 's')]
        system_prompt: Option<String>,
        /// Allow multi-line user input without escaping newlines
        #[arg(long = "multiline-input")]
        multiline_input: bool,
        /// Use simplified IO mode (better for piping/limited terminals)
        #[arg(long = "simple-io")]
        simple_io: bool,
        #[command(flatten)]
        sampling: SamplingArgs,
        /// Enable agentic mode: drives the backend agentic loop instead of llama-cli
        #[arg(long)]
        agent: bool,
        /// Reuse an already-running llama-server on this port (skips auto-start)
        #[arg(long)]
        port: Option<u16>,
        /// Maximum agent iterations before giving up (agentic mode only)
        #[arg(long = "max-iterations", default_value = "25")]
        max_iterations: usize,
        /// Tool allowlist exposed to the model; may be repeated or comma-separated.
        /// Omit to allow all tools. (agentic mode only, e.g. "mcp_search,builtin_time")
        /// Note: the filter is evaluated once at session start. To change the
        /// available tools mid-session, exit and restart with a new --tools list.
        #[arg(long, value_delimiter = ',')]
        tools: Vec<String>,
        /// Per-tool execution timeout in milliseconds (agentic mode only)
        #[arg(long = "tool-timeout-ms")]
        tool_timeout_ms: Option<u64>,
        /// Maximum number of tools executed in parallel per iteration (agentic mode only)
        #[arg(long = "max-parallel")]
        max_parallel: Option<usize>,
        /// Model name forwarded to llama-server (agentic mode only; uses server default when omitted)
        #[arg(long)]
        model: Option<String>,
    },

    /// Ask a question with optional context from stdin or file
    #[command(
        display_order = 12,
        alias = "q",
        after_help = "EXAMPLES:\n    gglib q \"What is Rust?\"\n    cat file.txt | gglib q \"Summarize this\"\n    gglib q --file README.md \"Explain this project\"\n    echo \"Paris, Tokyo\" | gglib q \"List these cities: {}\""
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
        /// Cleaner output for scripting (no prompt echo, no timings)
        #[arg(long, short = 'Q')]
        quiet: bool,
        #[command(flatten)]
        sampling: SamplingArgs,
    },

    // ── Interfaces ──────────────────────────────────────────────────────
    /// Launch the Tauri desktop GUI
    #[command(display_order = 20)]
    Gui {
        /// Run in development mode with hot-reload (requires Node.js and npm)
        #[arg(long)]
        dev: bool,
    },

    /// Start the web-based GUI server
    #[command(display_order = 21)]
    Web {
        /// Port to serve the web GUI on
        #[arg(short, long, env = "VITE_GGLIB_WEB_PORT", default_value = "9887")]
        port: u16,
        /// Base port for llama-server instances (Note: Port 5000 conflicts with macOS AirPlay)
        #[arg(long, default_value = "9000")]
        base_port: u16,
        /// Serve API endpoints only (do not serve static UI assets)
        ///
        /// By default, `gglib web` will auto-detect a built frontend (e.g. `./web_ui`) and
        /// serve it with SPA fallback. Use this flag when running the React dev server (Vite)
        /// separately.
        #[arg(long)]
        api_only: bool,
        /// Path to the directory containing built frontend assets (e.g., ./web_ui/dist)
        #[arg(long)]
        static_dir: Option<std::path::PathBuf>,
    },

    /// Start OpenAI-compatible proxy with MCP tool gateway
    ///
    /// Serves /v1 chat completions and /mcp (MCP Streamable HTTP) from a single port.
    /// Configure OpenWebUI with the /v1 base URL and connect MCP tools via /mcp.
    #[command(display_order = 22)]
    Proxy {
        /// Host to bind to
        #[arg(long, default_value = "127.0.0.1")]
        host: String,
        /// Port to bind the proxy to
        #[arg(short, long, default_value = "8080")]
        port: u16,
        /// Starting port for llama-server instances (5500+ to avoid macOS AirPlay on 5000)
        #[arg(long, default_value = "5500")]
        llama_port: u16,
        /// Default context size when not specified by client
        #[arg(long, default_value = "4096")]
        default_context: u64,
    },
}
