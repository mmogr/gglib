//! Model management subcommands.
//!
//! This module defines the model CRUD, verification, download, and
//! HuggingFace discovery commands that live under `gglib model <sub>`.

use clap::Subcommand;

/// Model management commands.
///
/// Covers the full lifecycle of GGUF models: adding, listing, removing,
/// updating metadata, downloading from HuggingFace, verifying integrity,
/// and repairing corrupt files.
#[derive(Subcommand)]
pub enum ModelCommand {
    /// Add a GGUF model to the database
    Add {
        /// Path to GGUF file to add
        file_path: String,
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
        /// Set default temperature for this model (0.0-2.0)
        #[arg(long)]
        temperature: Option<f32>,
        /// Set default top-p for this model (0.0-1.0)
        #[arg(long = "top-p")]
        top_p: Option<f32>,
        /// Set default top-k for this model
        #[arg(long = "top-k")]
        top_k: Option<i32>,
        /// Set default max-tokens for this model
        #[arg(long = "max-tokens")]
        max_tokens: Option<u32>,
        /// Set default repeat-penalty for this model
        #[arg(long = "repeat-penalty")]
        repeat_penalty: Option<f32>,
        /// Clear all inference parameter defaults (revert to inherit mode)
        #[arg(long)]
        clear_inference_defaults: bool,
        /// Show preview without applying changes
        #[arg(long)]
        dry_run: bool,
        /// Skip confirmation prompt
        #[arg(short, long)]
        force: bool,
    },

    /// Verify model integrity by computing SHA256 hashes
    Verify {
        /// ID of the model to verify
        model_id: i64,
        /// Show detailed progress for each shard
        #[arg(short, long)]
        verbose: bool,
    },

    /// Repair a corrupt model by re-downloading failed shards
    Repair {
        /// ID of the model to repair
        model_id: i64,
        /// Specific shard indices to repair (comma-separated, e.g., "0,2,5")
        #[arg(short, long)]
        shards: Option<String>,
        /// Skip confirmation prompt
        #[arg(short, long)]
        force: bool,
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

    /// Upgrade a model to the latest version
    Upgrade {
        /// ID of the model to upgrade
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
}
