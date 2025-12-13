//! Database setup and initialization.
//!
//! This module provides the `setup_database()` function for initializing
//! the `SQLite` database with full schema. Entry points call this with the
//! resolved database path.

use anyhow::Result;
use sqlx::{SqlitePool, sqlite::SqliteConnectOptions};
use std::path::Path;

/// Valid table names for schema operations.
const VALID_TABLES: [&str; 6] = [
    "chat_conversations",
    "chat_messages",
    "models",
    "mcp_servers",
    "mcp_server_env",
    "download_queue",
];

/// Valid SQL column types for migrations.
const VALID_COLUMN_TYPES: [&str; 4] = ["TEXT", "INTEGER", "REAL", "BLOB"];

/// Sets up the `SQLite` database connection and ensures the schema exists.
///
/// This function:
/// 1. Establishes a connection to the `SQLite` database file
/// 2. Creates the database file if it doesn't exist
/// 3. Creates all tables and indexes
/// 4. Runs any necessary schema migrations
///
/// # Arguments
///
/// * `db_path` - Path to the `SQLite` database file
///
/// # Returns
///
/// Returns a `Result<SqlitePool>` containing the database connection pool.
///
/// # Errors
///
/// Returns an error if:
/// - The database file cannot be opened or created
/// - Schema creation fails
///
/// # Example
///
/// ```rust,no_run
/// use gglib_db::setup_database;
/// use std::path::Path;
///
/// # async fn example() -> anyhow::Result<()> {
/// let db_path = Path::new("/path/to/gglib.db");
/// let pool = setup_database(db_path).await?;
/// # Ok(())
/// # }
/// ```
pub async fn setup_database(db_path: &Path) -> Result<SqlitePool> {
    // Ensure parent directory exists
    if let Some(parent) = db_path.parent() {
        std::fs::create_dir_all(parent)?;
    }

    let pool = SqlitePool::connect_with(
        SqliteConnectOptions::new()
            .filename(db_path)
            .create_if_missing(true),
    )
    .await?;

    // Create all tables and indexes
    create_schema(&pool).await?;

    // Run migrations for existing databases
    run_migrations(&pool).await?;

    // Initialize settings table
    init_settings_table(&pool).await?;

    Ok(pool)
}

/// Sets up an in-memory `SQLite` database for testing.
///
/// Creates a fresh in-memory database with the full production schema.
#[cfg(any(test, feature = "test-utils"))]
pub async fn setup_test_database() -> Result<SqlitePool> {
    let pool = SqlitePool::connect("sqlite::memory:").await?;
    create_schema(&pool).await?;
    run_migrations(&pool).await?;
    init_settings_table(&pool).await?;
    Ok(pool)
}

/// Creates the complete database schema.
///
/// This function creates all tables and indexes required by the application.
/// It is safe to call multiple times as all operations use IF NOT EXISTS.
async fn create_schema(pool: &SqlitePool) -> Result<()> {
    // Create the models table
    sqlx::query(
        r#"
        CREATE TABLE IF NOT EXISTS models (
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
        )
        "#,
    )
    .execute(pool)
    .await?;

    // Unique index on file path
    sqlx::query("CREATE UNIQUE INDEX IF NOT EXISTS idx_models_file_path ON models(file_path)")
        .execute(pool)
        .await?;

    // Index on model name for faster LIKE queries
    sqlx::query("CREATE INDEX IF NOT EXISTS idx_models_name ON models(name)")
        .execute(pool)
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
    .execute(pool)
    .await?;

    // Create chat conversations table
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
    .execute(pool)
    .await?;

    // Create chat messages table
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
    .execute(pool)
    .await?;

    // Index on conversation_id for faster message queries
    sqlx::query(
        "CREATE INDEX IF NOT EXISTS idx_messages_conversation ON chat_messages(conversation_id)",
    )
    .execute(pool)
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
    .execute(pool)
    .await?;

    // Create MCP server environment variables table
    sqlx::query(
        r#"
        CREATE TABLE IF NOT EXISTS mcp_server_env (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            server_id INTEGER NOT NULL,
            key TEXT NOT NULL,
            value TEXT NOT NULL,
            FOREIGN KEY (server_id) REFERENCES mcp_servers(id) ON DELETE CASCADE,
            UNIQUE(server_id, key)
        )
        "#,
    )
    .execute(pool)
    .await?;

    // Index for faster MCP env lookups
    sqlx::query("CREATE INDEX IF NOT EXISTS idx_mcp_env_server ON mcp_server_env(server_id)")
        .execute(pool)
        .await?;

    // Create download_queue table for persistent download state
    sqlx::query(
        r#"
        CREATE TABLE IF NOT EXISTS download_queue (
            id TEXT PRIMARY KEY,
            status TEXT NOT NULL,
            repo_id TEXT NOT NULL,
            filename TEXT NOT NULL,
            quantization TEXT,
            target_path TEXT NOT NULL,
            total_bytes INTEGER,
            downloaded_bytes INTEGER NOT NULL DEFAULT 0,
            error_message TEXT,
            created_at TEXT NOT NULL,
            started_at TEXT,
            completed_at TEXT
        )
        "#,
    )
    .execute(pool)
    .await?;

    Ok(())
}

/// Initialize the settings table with default values if empty.
async fn init_settings_table(pool: &SqlitePool) -> Result<()> {
    use crate::SqliteSettingsRepository;

    let repo = SqliteSettingsRepository::new(pool.clone());
    repo.ensure_table().await?;
    Ok(())
}

/// Run schema migrations for existing databases.
///
/// This function handles migrations for databases created with older schema versions.
/// Each migration is idempotent (safe to run multiple times).
async fn run_migrations(pool: &SqlitePool) -> Result<()> {
    // Migration: ensure system_prompt column exists in chat_conversations
    // (for databases created before this column was added)
    ensure_column_exists(pool, "chat_conversations", "system_prompt", "TEXT").await?;

    Ok(())
}

/// Ensures a column exists in a table, adding it if necessary.
///
/// This is used for schema migrations when new columns are added to existing tables.
///
/// # Arguments
///
/// * `pool` - Database connection pool
/// * `table` - Table name (must be in `VALID_TABLES` whitelist)
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

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_setup_test_database() {
        let pool = setup_test_database().await.unwrap();

        // Verify tables exist by querying them
        let _: (i64,) = sqlx::query_as("SELECT COUNT(*) FROM models")
            .fetch_one(&pool)
            .await
            .unwrap();

        let _: (i64,) = sqlx::query_as("SELECT COUNT(*) FROM settings_kv")
            .fetch_one(&pool)
            .await
            .unwrap();

        let _: (i64,) = sqlx::query_as("SELECT COUNT(*) FROM mcp_servers")
            .fetch_one(&pool)
            .await
            .unwrap();
    }
}
