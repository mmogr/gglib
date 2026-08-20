//! Composition utilities for building `AppCore` with `SQLite` backends.
//!
//! This module provides factory functions for wiring up the application
//! with `SQLite` repositories. It is focused purely on construction and
//! should not contain any domain logic.

use sqlx::SqlitePool;
use std::sync::Arc;

use gglib_core::Repos;
use gglib_core::services::AppCore;

use crate::repositories::{
    SqliteChatHistoryRepository, SqliteDownloadStateRepository, SqliteMcpRepository,
    SqliteModelRepository, SqliteSettingsRepository,
};

/// Factory for creating repository instances with `SQLite` backends.
///
/// This struct provides composition utilities only — no domain logic.
pub struct CoreFactory;

impl CoreFactory {
    /// Build all `SQLite` repositories from a pool.
    ///
    /// This is the recommended way for adapters to obtain repositories.
    /// Returns a `Repos` struct from `gglib-core` containing trait-object-wrapped
    /// repositories.
    pub fn build_repos(pool: SqlitePool) -> Repos {
        Repos::new(
            Arc::new(SqliteModelRepository::new(pool.clone())),
            Arc::new(SqliteSettingsRepository::new(pool.clone())),
            Arc::new(SqliteMcpRepository::new(pool.clone())),
            Arc::new(SqliteChatHistoryRepository::new(pool)),
        )
    }

    /// Build a complete `AppCore` instance from a pool.
    ///
    /// This is the recommended single-step way for adapters to obtain
    /// a fully composed `AppCore`. Equivalent to:
    ///
    /// ```ignore
    /// let repos = CoreFactory::build_repos(pool);
    /// let core = AppCore::new(repos);
    /// ```
    ///
    /// # Arguments
    ///
    /// * `pool` - `SQLite` connection pool from `setup_database()`
    ///
    /// # Example
    ///
    /// ```ignore
    /// use gglib_db::{CoreFactory, setup_database};
    ///
    /// let pool = setup_database(&db_path).await?;
    /// let core = CoreFactory::build_app_core(pool);
    /// ```
    pub fn build_app_core(pool: SqlitePool) -> AppCore {
        let repos = Self::build_repos(pool);
        AppCore::new(repos)
    }

    /// Create a model repository from a pool.
    pub fn model_repository(pool: SqlitePool) -> Arc<SqliteModelRepository> {
        Arc::new(SqliteModelRepository::new(pool))
    }

    /// Create a settings repository from a pool.
    pub fn settings_repository(pool: SqlitePool) -> Arc<SqliteSettingsRepository> {
        Arc::new(SqliteSettingsRepository::new(pool))
    }

    /// Create an MCP server repository from a pool.
    pub fn mcp_repository(pool: SqlitePool) -> Arc<SqliteMcpRepository> {
        Arc::new(SqliteMcpRepository::new(pool))
    }

    /// Create a download state repository from a pool.
    pub fn download_state_repository(pool: SqlitePool) -> Arc<SqliteDownloadStateRepository> {
        Arc::new(SqliteDownloadStateRepository::new(pool))
    }

    /// Build a `ModelRegistrar` for tests.
    ///
    /// `gglib-bootstrap` is the sole production call site for
    /// `ModelRegistrar::new` (enforced by `scripts/check_boundaries.sh`), so
    /// integration tests that need a registrar go through this composition
    /// point — itself an allowed caller — instead of constructing one
    /// directly.
    #[cfg(any(test, feature = "test-utils"))]
    pub fn model_registrar_for_test(
        pool: SqlitePool,
        gguf_parser: Arc<dyn gglib_core::ports::GgufParserPort>,
    ) -> gglib_core::services::ModelRegistrar {
        gglib_core::services::ModelRegistrar::new(Self::model_repository(pool), gguf_parser, None)
    }
}
