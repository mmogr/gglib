//! Composition utilities for building AppCore with SQLite backends.
//!
//! This module provides factory functions for wiring up the application
//! with SQLite repositories. It is focused purely on construction and
//! should not contain any domain logic.

use sqlx::SqlitePool;
use std::sync::Arc;

use gglib_core::{ModelRepository, SettingsRepository};

use crate::repositories::{SqliteModelRepository, SqliteSettingsRepository};

/// A container for all repository instances.
///
/// This struct provides a consistent way to wire repositories across adapters
/// without turning the factory into a second "app layer". It uses trait objects
/// so adapters depend only on the port interfaces, not concrete types.
pub struct Repos {
    /// Model repository instance.
    pub models: Arc<dyn ModelRepository>,
    /// Settings repository instance.
    pub settings: Arc<dyn SettingsRepository>,
}

/// Factory for creating repository instances with SQLite backends.
///
/// This struct provides composition utilities only — no domain logic.
pub struct CoreFactory;

impl CoreFactory {
    /// Create a SQLite connection pool.
    ///
    /// # Arguments
    ///
    /// * `db_url` - SQLite connection URL (e.g., "sqlite:~/.gglib/gglib.db")
    pub async fn create_pool(db_url: &str) -> anyhow::Result<SqlitePool> {
        let pool = SqlitePool::connect(db_url).await?;
        Ok(pool)
    }

    /// Create an in-memory SQLite pool for testing.
    pub async fn create_test_pool() -> anyhow::Result<SqlitePool> {
        let pool = SqlitePool::connect("sqlite::memory:").await?;
        Ok(pool)
    }

    /// Build all SQLite repositories from a pool.
    ///
    /// This is the recommended way for adapters to obtain repositories.
    /// Returns a `Repos` struct containing trait-object-wrapped repositories.
    pub fn build_repos(pool: SqlitePool) -> Repos {
        Repos {
            models: Arc::new(SqliteModelRepository::new(pool.clone())),
            settings: Arc::new(SqliteSettingsRepository::new(pool)),
        }
    }

    /// Create a model repository from a pool.
    pub fn model_repository(pool: SqlitePool) -> Arc<SqliteModelRepository> {
        Arc::new(SqliteModelRepository::new(pool))
    }

    /// Create a settings repository from a pool.
    pub fn settings_repository(pool: SqlitePool) -> Arc<SqliteSettingsRepository> {
        Arc::new(SqliteSettingsRepository::new(pool))
    }
}

/// Test database helper for integration tests.
///
/// Provides an in-memory SQLite database with schema already applied.
#[cfg(any(test, feature = "test-utils"))]
pub struct TestDb {
    pool: SqlitePool,
}

#[cfg(any(test, feature = "test-utils"))]
impl TestDb {
    /// Create a new in-memory test database with schema.
    pub async fn new() -> anyhow::Result<Self> {
        let pool = SqlitePool::connect("sqlite::memory:").await?;

        // Create models table
        sqlx::query(
            r#"
            CREATE TABLE IF NOT EXISTS models (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                name TEXT NOT NULL,
                file_path TEXT NOT NULL UNIQUE,
                param_count_b REAL NOT NULL,
                architecture TEXT,
                quantization TEXT,
                context_length INTEGER,
                metadata TEXT NOT NULL DEFAULT '{}',
                added_at TEXT NOT NULL,
                hf_repo_id TEXT,
                hf_commit_sha TEXT,
                hf_filename TEXT,
                download_date TEXT,
                last_update_check TEXT,
                tags TEXT NOT NULL DEFAULT '[]'
            )
            "#,
        )
        .execute(&pool)
        .await?;

        // Create settings table
        sqlx::query(
            r#"
            CREATE TABLE IF NOT EXISTS settings_kv (
                key TEXT PRIMARY KEY NOT NULL,
                value TEXT NOT NULL,
                updated_at TEXT NOT NULL
            )
            "#,
        )
        .execute(&pool)
        .await?;

        Ok(Self { pool })
    }

    /// Get the underlying pool.
    pub fn pool(&self) -> &SqlitePool {
        &self.pool
    }

    /// Create a model repository using this test database.
    pub fn model_repository(&self) -> SqliteModelRepository {
        SqliteModelRepository::new(self.pool.clone())
    }

    /// Create a settings repository using this test database.
    pub fn settings_repository(&self) -> SqliteSettingsRepository {
        SqliteSettingsRepository::new(self.pool.clone())
    }
}
