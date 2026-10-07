//! Configuration types for [`crate::CoreBootstrap::build`].

use std::path::PathBuf;

/// Configuration required to run [`crate::CoreBootstrap::build`].
///
/// All paths must be fully resolved by the caller before passing this struct.
/// The path-resolution helpers (`database_path`, `resolve_models_dir`) are
/// deliberately kept in `gglib-core::paths` so that adapters own their own
/// path strategies.
///
/// It holds no `HuggingFace` token: [`crate::CoreBootstrap::build`] reads the
/// one in the environment itself, so no adapter can be wired without it.
#[derive(Debug, Clone)]
pub struct BootstrapConfig {
    /// Absolute path to the `SQLite` database file.
    pub db_path: PathBuf,
    /// Absolute path to the directory where model files are stored.
    pub models_dir: PathBuf,
}
