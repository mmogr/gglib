//! Database schema management and migrations.
//!
//! This module contains all table definitions, index creation,
//! and schema migration utilities.

use anyhow::Result;
use sqlx::SqlitePool;

/// Valid table names for schema operations.
pub const VALID_TABLES: [&str; 3] = ["chat_conversations", "chat_messages", "models"];

/// Valid SQL column types for migrations.
pub const VALID_COLUMN_TYPES: [&str; 4] = ["TEXT", "INTEGER", "REAL", "BLOB"];

/// Creates the complete database schema.
///
/// This function creates all tables and indexes required by the application.
/// It is safe to call multiple times as all operations use IF NOT EXISTS.
///
/// Used by both production `setup_database()` and test helpers to ensure
/// schema parity between production and test environments.
pub(crate) async fn create_schema(pool: &SqlitePool) -> Result<()> {
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
    .execute(pool)
    .await?;

    // Prevent duplicate entries for the same on-disk GGUF file.
    sqlx::query("CREATE UNIQUE INDEX IF NOT EXISTS idx_models_file_path ON models(file_path)")
        .execute(pool)
        .await?;

    // Index on model name for faster LIKE queries
    sqlx::query("CREATE INDEX IF NOT EXISTS idx_models_name ON models(name)")
        .execute(pool)
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
    .execute(pool)
    .await?;

    // Migration: ensure system_prompt column exists for older databases
    ensure_column_exists(pool, "chat_conversations", "system_prompt", "TEXT").await?;

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
    .execute(pool)
    .await?;

    // Create index on conversation_id for faster message queries
    sqlx::query(
        "CREATE INDEX IF NOT EXISTS idx_messages_conversation 
         ON chat_messages(conversation_id)",
    )
    .execute(pool)
    .await?;

    Ok(())
}

/// Ensures a column exists in a table, adding it if necessary.
///
/// This is used for schema migrations when new columns are added to existing tables.
///
/// # Arguments
///
/// * `pool` - Database connection pool
/// * `table` - Table name (must be in VALID_TABLES whitelist)
/// * `column` - Column name to check/add
/// * `definition` - SQL column definition (e.g., "TEXT", "INTEGER NOT NULL")
///
/// # Errors
///
/// Returns an error if:
/// - Table name is not in the whitelist
/// - Column name contains invalid characters
/// - Column definition doesn't start with a valid SQL type
/// - Database operation fails
pub async fn ensure_column_exists(
    pool: &SqlitePool,
    table: &str,
    column: &str,
    definition: &str,
) -> Result<()> {
    // Validate table name against a whitelist
    if !VALID_TABLES.contains(&table) {
        return Err(anyhow::anyhow!("Invalid table name: {}", table));
    }

    // Validate column name contains only alphanumeric and underscore
    if !column.chars().all(|c| c.is_alphanumeric() || c == '_') {
        return Err(anyhow::anyhow!("Invalid column name: {}", column));
    }

    // Validate column definition starts with a valid SQL type
    if !VALID_COLUMN_TYPES
        .iter()
        .any(|def| definition.to_uppercase().starts_with(def))
    {
        return Err(anyhow::anyhow!("Invalid column definition: {}", definition));
    }

    // Use double quotes to safely quote the table name identifier
    let pragma_query = format!(
        "SELECT COUNT(*) FROM pragma_table_info(\"{}\") WHERE name = ?",
        table
    );
    let column_exists: i64 = sqlx::query_scalar(&pragma_query)
        .bind(column)
        .fetch_one(pool)
        .await?;

    if column_exists == 0 {
        // Use double quotes to safely quote identifiers
        let alter_stmt = format!(
            "ALTER TABLE \"{}\" ADD COLUMN \"{}\" {}",
            table, column, definition
        );

        if let Err(err) = sqlx::query(&alter_stmt).execute(pool).await {
            let is_duplicate_column = matches!(
                &err,
                sqlx::Error::Database(db_err)
                    if db_err.message().contains("duplicate column name")
            );

            if is_duplicate_column {
                return Ok(());
            }

            return Err(err.into());
        }
    }

    Ok(())
}
