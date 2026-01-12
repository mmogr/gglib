//! Main commands enum and primary subcommands.
//!
//! This module defines the available commands for the CLI tool.

use clap::Subcommand;

use crate::assistant_ui_commands::AssistantUiCommand;
use crate::config_commands::ConfigCommand;
use crate::llama_commands::LlamaCommand;

/// Available commands for the GGUF library management tool.
///
/// Each command represents a different operation that can be performed
/// on GGUF models in the local library.
#[derive(Subcommand)]
pub enum Commands {
    /// Check system dependencies required for gglib
    CheckDeps,

    /// Show resolved paths for all gglib directories
    Paths,

    /// Add a GGUF model to the database
    Add {
        /// Path to GGUF file to add
        file_path: String,
    },

    /// Download a GGUF model from HuggingFace Hub
    Download {
        /// HuggingFace model repository (e.g., "microsoft/DialoGPT-medium")
        model_id: String,
        /// Specific quantization to download (e.g., "Q4_K_M", "F16")
        #[arg(short, long)]
        quantization: Option<String>,
        /// List available quantizations for the model
        #[arg(long)]
        list_quants: bool,
        /// Skip adding to database after download (models are registered by default)
        #[arg(long)]
        skip_db: bool,
        /// HuggingFace token for private models
        #[arg(long)]
        token: Option<String>,
        /// Skip confirmation prompt
        #[arg(short, long)]
        force: bool,
    },

    /// Check for updates to downloaded models
    CheckUpdates {
        /// Check specific model by ID
        #[arg(short, long)]
        model_id: Option<u32>,
        /// Check all models
        #[arg(long)]
        all: bool,
    },

    /// Update a model to the latest version
    UpdateModel {
        /// ID of the model to update
        model_id: u32,
        /// Skip confirmation prompt
        #[arg(short, long)]
        force: bool,
    },

    /// Search HuggingFace Hub for GGUF models
    Search {
        /// Search query (model name, author, or keywords)
        query: String,
        /// Limit number of results
        #[arg(short, long, default_value = "10")]
        limit: u32,
        /// Sort by: "downloads", "created", "likes", "updated"
        #[arg(short, long, default_value = "downloads")]
        sort: String,
        /// Only show models with GGUF files
        #[arg(long)]
        gguf_only: bool,
    },

    /// Browse popular GGUF models on HuggingFace Hub
    Browse {
        /// Category to browse: "popular", "recent", "trending"
        #[arg(default_value = "popular")]
        category: String,
        /// Limit number of results
        #[arg(short, long, default_value = "20")]
        limit: u32,
        /// Filter by model size (e.g., "7B", "13B", "70B")
        #[arg(long)]
        size: Option<String>,
    },

    /// List all GGUF models in the database
    List,

    /// Remove a GGUF model from the database
    Remove {
        /// Name or ID of the model to remove
        identifier: String,
        /// Skip confirmation prompt
        #[arg(short, long)]
        force: bool,
    },

    /// Update model metadata in the database
    Update {
        /// ID of the model to update
        id: u32,
        /// New name for the model
        #[arg(short, long)]
        name: Option<String>,
        /// Update parameter count (in billions)
        #[arg(short, long)]
        param_count: Option<f64>,
        /// Update architecture
        #[arg(short, long)]
        architecture: Option<String>,
        /// Update quantization type
        #[arg(short, long)]
        quantization: Option<String>,
        /// Update context length
        #[arg(short, long)]
        context_length: Option<u64>,
        /// Add or update metadata (format: key=value)
        #[arg(short, long, action = clap::ArgAction::Append)]
        metadata: Vec<String>,
        /// Remove specific metadata keys (comma-separated)
        #[arg(long)]
        remove_metadata: Option<String>,
        /// Replace entire metadata instead of merging
        #[arg(long)]
        replace_metadata: bool,
        /// Show preview without applying changes
        #[arg(long)]
        dry_run: bool,
        /// Skip confirmation prompt
        #[arg(short, long)]
        force: bool,
    },

    /// Serve a GGUF model with llama-server
    Serve {
        /// ID of the model to serve
        id: u32,
        /// Context size (use 'max' to auto-detect from model metadata)
        #[arg(short, long)]
        ctx_size: Option<String>,
        /// Enable memory lock
        #[arg(long)]
        mlock: bool,
        /// Force-enable Jinja template parsing for chat templates
        #[arg(long)]
        jinja: bool,
        /// Port to serve on
        #[arg(short, long, default_value = "8080")]
        port: u16,
    },

    /// Chat with a model directly via llama-cli
    Chat {
        /// Name or ID of the model to chat with
        identifier: String,
        /// Context size (use 'max' to auto-detect from model metadata)
        #[arg(short, long)]
        ctx_size: Option<String>,
        /// Enable memory lock
        #[arg(long)]
        mlock: bool,
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
    },

    /// Ask a question with optional context from stdin or file
    #[command(
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
        /// Context size (use 'max' to auto-detect from model metadata)
        #[arg(short, long)]
        ctx_size: Option<String>,
        /// Enable memory lock
        #[arg(long)]
        mlock: bool,
        /// Show the constructed prompt before sending
        #[arg(long)]
        verbose: bool,
        /// Cleaner output for scripting (no prompt echo, no timings)
        #[arg(long, short = 'Q')]
        quiet: bool,
    },

    /// Launch the Tauri desktop GUI
    Gui {
        /// Run in development mode with hot-reload (requires Node.js and npm)
        #[arg(long)]
        dev: bool,
    },

    /// Start the web-based GUI server
    Web {
        /// Port to serve the web GUI on
        #[arg(short, long, default_value = "9887")]
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

    /// Start OpenAI-compatible proxy for automatic model swapping
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

    /// Manage persistent configuration (models directory, etc.)
    Config {
        #[command(subcommand)]
        command: ConfigCommand,
    },
}
