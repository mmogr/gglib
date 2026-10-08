//! Configuration types for [`crate::CoreBootstrap::build`].

use std::path::PathBuf;

/// Configuration required to run [`crate::CoreBootstrap::build`].
///
/// The database path must be fully resolved by the caller before passing
/// this struct. The helper that resolves it (`database_path`) is deliberately
/// kept in `gglib-core::paths` so that adapters own their own path
/// strategies.
///
/// It holds no `HuggingFace` token: [`crate::CoreBootstrap::build`] reads the
/// one in the environment itself, so no adapter can be wired without it.
///
/// It holds no models directory either. One a caller resolved would be the
/// directory as the adapter started, and a daemon would download there for
/// the rest of its life. The download manager is handed none and asks
/// `resolve_models_dir` as each download starts, so a download goes where
/// the directory resolves then, not where it resolved as the adapter started.
#[derive(Debug, Clone)]
pub struct BootstrapConfig {
    /// Absolute path to the `SQLite` database file.
    pub db_path: PathBuf,
}
