//! Configuration types for [`crate::CoreBootstrap::build`].

use std::path::PathBuf;

/// Configuration required to run [`crate::CoreBootstrap::build`].
///
/// All paths must be fully resolved by the caller before passing this struct.
/// The path-resolution helpers (`database_path`, `llama_server_path`,
/// `resolve_models_dir`) are deliberately kept in `gglib-core::paths` so
/// that adapters own their own path strategies.
#[derive(Debug, Clone)]
pub struct BootstrapConfig {
    /// Absolute path to the `SQLite` database file.
    pub db_path: PathBuf,
    /// Absolute path to the llama-server binary.
    pub llama_server_path: PathBuf,
    /// Maximum number of concurrently running llama-server processes.
    pub max_concurrent: usize,
    /// Absolute path to the directory where model files are stored.
    pub models_dir: PathBuf,
    /// Optional `HuggingFace` API token for authenticated downloads.
    pub hf_token: Option<String>,
}
