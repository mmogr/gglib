//! Command-line interface definitions and argument parsing.
//!
//! This module defines the CLI structure using clap, including all
//! commands, arguments, and parsing logic for the GGUF library
//! management tool.

use clap::{Parser, Subcommand};

/// Command-line interface definition for the GGUF library management tool.
///
/// This module contains all CLI-related structures and parsing logic,
/// separated from the main application logic for better modularity.
#[derive(Parser)]
#[command(name = "gglib")]
#[command(about = "Manage and run local GGUF models")]
#[command(version)]
pub struct Cli {
    /// Override the models directory for this invocation
    #[arg(long = "models-dir", global = true)]
    pub models_dir: Option<String>,

    /// Enable verbose/debug output
    #[arg(short = 'v', long = "verbose", global = true)]
    pub verbose: bool,

    #[command(subcommand)]
    pub command: Option<Commands>,
}

/// Available commands for the GGUF library management tool.
///
/// Each command represents a different operation that can be performed
/// on GGUF models in the local library.
#[derive(Subcommand)]
pub enum Commands {
    /// Check system dependencies required for gglib
    CheckDeps,
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
        /// Add to database after download
        #[arg(long)]
        add_to_db: bool,
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

/// llama.cpp management commands
#[derive(Subcommand)]
pub enum LlamaCommand {
    /// Install llama.cpp and build llama-server
    Install {
        /// Build with CUDA support
        #[arg(long)]
        cuda: bool,
        /// Build with Metal support (macOS only)
        #[arg(long)]
        metal: bool,
        /// Build CPU-only version
        #[arg(long)]
        cpu_only: bool,
        /// Force rebuild even if already installed
        #[arg(short, long)]
        force: bool,
        /// Force building from source instead of downloading pre-built binaries
        #[arg(long)]
        build: bool,
    },
    /// Check for llama.cpp updates
    CheckUpdates,
    /// Update llama.cpp to latest version
    Update,
    /// Show llama.cpp build information and status
    Status,
    /// Rebuild llama-server with different options
    Rebuild {
        /// Build with CUDA support
        #[arg(long)]
        cuda: bool,
        /// Build with Metal support (macOS only)
        #[arg(long)]
        metal: bool,
        /// Build CPU-only version
        #[arg(long)]
        cpu_only: bool,
    },
    /// Remove llama.cpp installation
    Uninstall {
        /// Skip confirmation prompt
        #[arg(short, long)]
        force: bool,
    },
}

/// assistant-ui management commands
#[derive(Subcommand)]
pub enum AssistantUiCommand {
    /// Install assistant-ui npm dependencies
    Install,
    /// Update assistant-ui dependencies
    Update,
    /// Show assistant-ui installation status
    Status,
}

/// Configuration management commands
#[derive(Subcommand)]
pub enum ConfigCommand {
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

/// Models directory command variants
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

/// Settings command variants
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
        server_port: Option<u16>,
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
}
