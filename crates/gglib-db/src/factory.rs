//! Composition utilities for building AppCore with SQLite backends.
//!
//! This module provides factory functions for wiring up the application
//! with SQLite repositories. It is focused purely on construction and
//! should not contain any domain logic.

use sqlx::SqlitePool;
use std::sync::Arc;

use gglib_core::Repos;

use crate::repositories::{SqliteModelRepository, SqliteSettingsRepository};

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
    /// Returns a `Repos` struct from `gglib-core` containing trait-object-wrapped
    /// repositories.
    pub fn build_repos(pool: SqlitePool) -> Repos {
        Repos::new(
            Arc::new(SqliteModelRepository::new(pool.clone())),
            Arc::new(SqliteSettingsRepository::new(pool)),
        )
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
/// Provides an in-memory SQLite database with full schema already applied.
/// Matches the production schema to ensure test parity.
#[cfg(any(test, feature = "test-utils"))]
pub struct TestDb {
    pool: SqlitePool,
}

#[cfg(any(test, feature = "test-utils"))]
impl TestDb {
    /// Create a new in-memory test database with full schema.
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

        // Create chat_conversations table
        sqlx::query(
            r#"
            CREATE TABLE IF NOT EXISTS chat_conversations (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                title TEXT NOT NULL,
                model_id INTEGER,
                system_prompt TEXT,
                created_at TEXT NOT NULL DEFAULT (datetime('now')),
                updated_at TEXT NOT NULL DEFAULT (datetime('now')),
                FOREIGN KEY (model_id) REFERENCES models(id) ON DELETE SET NULL
            )
            "#,
        )
        .execute(&pool)
        .await?;

        // Create chat_messages table
        sqlx::query(
            r#"
            CREATE TABLE IF NOT EXISTS chat_messages (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                conversation_id INTEGER NOT NULL,
                role TEXT NOT NULL CHECK(role IN ('system', 'user', 'assistant')),
                content TEXT NOT NULL,
                created_at TEXT NOT NULL DEFAULT (datetime('now')),
                FOREIGN KEY (conversation_id) REFERENCES chat_conversations(id) ON DELETE CASCADE
            )
            "#,
        )
        .execute(&pool)
        .await?;

        // Create index on conversation_id for faster message queries
        sqlx::query(
            r#"
            CREATE INDEX IF NOT EXISTS idx_messages_conversation 
            ON chat_messages(conversation_id)
            "#,
        )
        .execute(&pool)
        .await?;

        // Create MCP servers table
        sqlx::query(
            r#"
            CREATE TABLE IF NOT EXISTS mcp_servers (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                name TEXT NOT NULL,
                type TEXT NOT NULL CHECK (type IN ('stdio', 'sse')),
                enabled INTEGER NOT NULL DEFAULT 1,
                auto_start INTEGER NOT NULL DEFAULT 0,
                command TEXT,
                args TEXT,
                cwd TEXT,
                url TEXT,
                created_at TEXT NOT NULL DEFAULT (datetime('now')),
                last_connected_at TEXT
            )
            "#,
        )
        .execute(&pool)
        .await?;

        // Create MCP server environment variables table
        sqlx::query(
            r#"
            CREATE TABLE IF NOT EXISTS mcp_server_env (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                server_id INTEGER NOT NULL,
                key TEXT NOT NULL,
                value TEXT NOT NULL,
                FOREIGN KEY (server_id) REFERENCES mcp_servers(id) ON DELETE CASCADE
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
