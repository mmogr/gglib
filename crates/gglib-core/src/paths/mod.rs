#![doc = include_str!("README.md")]
mod config;
mod database;
mod ensure;
mod error;
mod llama;
mod models;
mod pids;
mod platform;
mod resolver;
mod slots;

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
    gglib_data_dir, llama_bench_path, llama_config_path, llama_cpp_dir, llama_server_path,
};

// Models directory
#[cfg(not(target_os = "windows"))]
pub use models::DEFAULT_MODELS_DIR_RELATIVE;
pub use models::{ModelsDirResolution, ModelsDirSource, default_models_dir, resolve_models_dir};

// PID tracking
pub use pids::pids_dir;

// Directory operations
pub use ensure::{DirectoryCreationStrategy, ensure_directory, verify_writable};

// Configuration persistence
pub use config::{env_file_path, persist_env_value, persist_models_dir};

// Pure resolver for testing and CLI
pub use resolver::ResolvedPaths;

// Slot cache paths
pub use slots::{
    slot_bin_path, slot_file_name, slot_model_prefix, slot_session_from_stem, slot_tmp_file_name,
};
