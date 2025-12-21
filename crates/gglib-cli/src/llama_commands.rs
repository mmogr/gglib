//! llama.cpp management subcommands.
//!
//! This module defines the llama.cpp installation and management commands.

use clap::Subcommand;

/// llama.cpp management commands.
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
