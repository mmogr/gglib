//! Pure path resolver for testing and CLI introspection.
//!
//! This module provides a single struct that captures all resolved paths
//! in one call, making it easy to compare path resolution across adapters
//! and expose via `gglib paths` CLI command.

use std::path::PathBuf;

use super::{
    ModelsDirSource, PathError, data_root, database_path, llama_server_path, resolve_models_dir,
    resource_root,
};

/// All resolved paths captured in a single struct.
///
/// This is the "golden truth" for path resolution - use it for:
/// - Integration tests comparing adapter parity
/// - CLI `gglib paths` command output
/// - Debugging path resolution issues
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResolvedPaths {
    /// Root directory for application data (database, logs, etc.)
    pub data_root: PathBuf,
    /// Root directory for application resources (binaries, assets)
    pub resource_root: PathBuf,
    /// Path to the `SQLite` database file
    pub database_path: PathBuf,
    /// Path to the llama-server binary
    pub llama_server_path: PathBuf,
    /// Path to the models directory
    pub models_dir: PathBuf,
    /// How the models directory was resolved
    pub models_source: ModelsDirSource,
}

impl ResolvedPaths {
    /// Resolve all paths using the current environment.
    ///
    /// This calls each path resolver once and captures the results.
    /// Use this instead of calling individual resolvers when you need
    /// multiple paths - it's more efficient and guarantees consistency.
    pub fn resolve() -> Result<Self, PathError> {
        let data_root = data_root()?;
        let resource_root = resource_root()?;
        let database_path = database_path()?;
        let llama_server_path = llama_server_path()?;
        let models_resolution = resolve_models_dir(None)?;

        Ok(Self {
            data_root,
            resource_root,
            database_path,
            llama_server_path,
            models_dir: models_resolution.path,
            models_source: models_resolution.source,
        })
    }

    /// Resolve with an explicit models directory override.
    ///
    /// Use this to test behavior when `--models-dir` is passed.
    pub fn resolve_with_models_dir(models_dir: Option<&str>) -> Result<Self, PathError> {
        let data_root = data_root()?;
        let resource_root = resource_root()?;
        let database_path = database_path()?;
        let llama_server_path = llama_server_path()?;
        let models_resolution = resolve_models_dir(models_dir)?;

        Ok(Self {
            data_root,
            resource_root,
            database_path,
            llama_server_path,
            models_dir: models_resolution.path,
            models_source: models_resolution.source,
        })
    }
}

impl std::fmt::Display for ResolvedPaths {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        writeln!(f, "data_root = {}", self.data_root.display())?;
        writeln!(f, "resource_root = {}", self.resource_root.display())?;
        writeln!(f, "database_path = {}", self.database_path.display())?;
        writeln!(
            f,
            "llama_server_path = {}",
            self.llama_server_path.display()
        )?;
        writeln!(f, "models_dir = {}", self.models_dir.display())?;
        write!(f, "models_source = {:?}", self.models_source)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::paths::test_utils::ENV_LOCK;

    #[test]
    fn resolve_returns_consistent_paths() {
        // Lock ensures this test doesn't run concurrently with config.rs tests
        // that modify GGLIB_DATA_DIR, preventing non-deterministic results
        let _guard = ENV_LOCK.lock().unwrap();

        let first = ResolvedPaths::resolve().expect("first resolve");
        let second = ResolvedPaths::resolve().expect("second resolve");

        assert_eq!(first, second, "path resolution should be deterministic");
    }

    #[test]
    fn display_format_is_parseable() {
        let paths = ResolvedPaths::resolve().expect("resolve");
        let output = paths.to_string();

        // Should contain key = value pairs
        assert!(output.contains("data_root = "));
        assert!(output.contains("resource_root = "));
        assert!(output.contains("database_path = "));
        assert!(output.contains("llama_server_path = "));
        assert!(output.contains("models_dir = "));
        assert!(output.contains("models_source = "));
    }
}
