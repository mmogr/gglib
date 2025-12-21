//! Path utilities for gglib data directories and user-configurable locations.
//!
//! This module provides the canonical path resolution for all gglib components:
//! - Database location
//! - Models directory
//! - Llama.cpp binaries
//! - Application data and resource roots
//!
//! # Design
//!
//! - Returns `PathBuf` and `PathError` for clear error handling
//! - No interactive/terminal I/O - adapters handle user prompts separately
//! - OS-specific logic is kept private in `platform`

mod config;
mod database;
mod ensure;
mod error;
mod llama;
mod models;
mod pids;
mod platform;
mod resolver;

#[cfg(test)]
mod test_utils;

// Re-export public API

// Error type
pub use error::PathError;

// Platform detection and roots
pub use platform::{data_root, is_prebuilt_binary, resource_root};

// Database
pub use database::database_path;

// Llama binaries
pub use llama::{
    gglib_data_dir, llama_cli_path, llama_config_path, llama_cpp_dir, llama_server_path,
};

// Models directory
pub use models::{
    DEFAULT_MODELS_DIR_RELATIVE, ModelsDirResolution, ModelsDirSource, default_models_dir,
    resolve_models_dir,
};

// PID tracking
pub use pids::pids_dir;

// Directory operations
pub use ensure::{DirectoryCreationStrategy, ensure_directory, verify_writable};

// Configuration persistence
pub use config::{env_file_path, persist_env_value, persist_models_dir};

// Pure resolver for testing and CLI
pub use resolver::ResolvedPaths;
