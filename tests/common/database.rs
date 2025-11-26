//! Database test utilities.
//!
//! Provides functions for creating isolated in-memory test databases
//! that match the production schema exactly.

use anyhow::Result;
use sqlx::SqlitePool;

/// Creates an in-memory SQLite database with the full production schema.
///
/// This function creates a fresh database for each test, ensuring complete
/// isolation between tests. The schema matches production exactly by using
/// the same `create_schema()` function.
///
/// # Example
///
/// ```rust,ignore
/// use crate::common::database::setup_test_pool;
///
/// #[tokio::test]
/// async fn test_something() {
///     let pool = setup_test_pool().await.unwrap();
///     // Test with the pool...
/// }
/// ```
pub async fn setup_test_pool() -> Result<SqlitePool> {
    let pool = SqlitePool::connect("sqlite::memory:").await?;
    
    // Create the models table with enhanced metadata fields
    sqlx::query(
        "CREATE TABLE IF NOT EXISTS models (
            id INTEGER PRIMARY KEY, 
            name TEXT NOT NULL, 
            file_path TEXT NOT NULL, 
            param_count_b REAL NOT NULL,
            architecture TEXT,
            quantization TEXT,
            context_length INTEGER,
            metadata TEXT,
            added_at TEXT NOT NULL,
            hf_repo_id TEXT,
            hf_commit_sha TEXT,
            hf_filename TEXT,
            download_date TEXT,
            last_update_check TEXT,
            tags TEXT DEFAULT '[]'
        )",
    )
    .execute(&pool)
    .await?;

    // Prevent duplicate entries for the same on-disk GGUF file.
    sqlx::query("CREATE UNIQUE INDEX IF NOT EXISTS idx_models_file_path ON models(file_path)")
        .execute(&pool)
        .await?;

    // Index on model name for faster LIKE queries
    sqlx::query("CREATE INDEX IF NOT EXISTS idx_models_name ON models(name)")
        .execute(&pool)
        .await?;

    // Create chat conversations table for chat history
    sqlx::query(
        "CREATE TABLE IF NOT EXISTS chat_conversations (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            title TEXT NOT NULL,
            model_id INTEGER,
            system_prompt TEXT,
            created_at TEXT NOT NULL DEFAULT (datetime('now')),
            updated_at TEXT NOT NULL DEFAULT (datetime('now')),
            FOREIGN KEY (model_id) REFERENCES models(id) ON DELETE SET NULL
        )",
    )
    .execute(&pool)
    .await?;

    // Create chat messages table for storing conversation messages
    sqlx::query(
        "CREATE TABLE IF NOT EXISTS chat_messages (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            conversation_id INTEGER NOT NULL,
            role TEXT NOT NULL CHECK(role IN ('system', 'user', 'assistant')),
            content TEXT NOT NULL,
            created_at TEXT NOT NULL DEFAULT (datetime('now')),
            FOREIGN KEY (conversation_id) REFERENCES chat_conversations(id) ON DELETE CASCADE
        )",
    )
    .execute(&pool)
    .await?;

    // Create index on conversation_id for faster message queries
    sqlx::query(
        "CREATE INDEX IF NOT EXISTS idx_messages_conversation 
         ON chat_messages(conversation_id)",
    )
    .execute(&pool)
    .await?;

    Ok(pool)
}
