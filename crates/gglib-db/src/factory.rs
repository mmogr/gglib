//! Composition utilities for building `Repos` with `SQLite` backends.
//!
//! This module provides factory functions for wiring up the application
//! with `SQLite` repositories. It is focused purely on construction and
//! should not contain any domain logic.

use sqlx::SqlitePool;
use std::sync::Arc;

use gglib_core::Repos;

use crate::repositories::{
    ModelFilesRepository, SqliteAttachmentStore, SqliteChatHistoryRepository, SqliteMcpRepository,
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
            Arc::new(ModelFilesRepository::new(pool.clone())),
            Arc::new(SqliteSettingsRepository::new(pool.clone())),
            Arc::new(SqliteMcpRepository::new(pool.clone())),
            Arc::new(SqliteChatHistoryRepository::new(pool.clone())),
            Arc::new(SqliteAttachmentStore::new(pool)),
        )
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
        gglib_core::services::ModelRegistrar::new(
            Arc::new(SqliteModelRepository::new(pool)),
            gguf_parser,
            None,
        )
    }
}
