//! Database setup and initialization.
//!
//! This module provides the `setup_database()` function for initializing
//! the `SQLite` database with full schema. Entry points call this with the
//! resolved database path.

use anyhow::Result;
use sqlx::{
    SqlitePool,
    sqlite::{SqliteConnectOptions, SqliteJournalMode, SqlitePoolOptions},
};
use std::path::Path;
use std::time::Duration;

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

    let pool = SqlitePoolOptions::new()
        .max_connections(5)
        .connect_with(
            SqliteConnectOptions::new()
                .filename(db_path)
                .create_if_missing(true)
                .journal_mode(SqliteJournalMode::Wal)
                .busy_timeout(Duration::from_secs(5))
                .pragma("synchronous", "NORMAL"),
        )
        .await?;

    // Create all tables and indexes
    create_schema(&pool).await?;

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
    init_settings_table(&pool).await?;
    Ok(pool)
}

/// Mark any benchmark runs that are stuck in `running` status as `failed`.
///
/// Call this **once** at daemon boot, after the schema is ready. It corrects
/// rows left in an inconsistent state by a prior crash. This function is
/// intentionally **not** called by the CLI — the CLI cannot safely determine
/// whether a `running` row belongs to a live daemon session.
pub async fn cleanup_zombie_benchmark_runs(pool: &SqlitePool) -> Result<()> {
    sqlx::query(
        "UPDATE benchmark_runs \
         SET status = 'failed', error = 'Process terminated unexpectedly' \
         WHERE status = 'running'",
    )
    .execute(pool)
    .await?;
    Ok(())
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
            inference_defaults TEXT,
            expert_count INTEGER,
            expert_used_count INTEGER,
            expert_shared_count INTEGER,
            metadata TEXT,
            added_at TEXT NOT NULL,
            hf_repo_id TEXT,
            hf_commit_sha TEXT,
            hf_filename TEXT,
            download_date TEXT,
            last_update_check TEXT,
            tags TEXT DEFAULT '[]',
            model_key TEXT NOT NULL,
            file_paths_json TEXT,
            capabilities INTEGER DEFAULT 0
        )
        "#,
    )
    .execute(pool)
    .await?;

    // Index on file path for lookups (no longer unique)
    sqlx::query("CREATE INDEX IF NOT EXISTS idx_models_file_path ON models(file_path)")
        .execute(pool)
        .await?;

    // Unique index on model_key (canonical identity)
    sqlx::query("CREATE UNIQUE INDEX IF NOT EXISTS idx_models_model_key ON models(model_key)")
        .execute(pool)
        .await?;

    // Index on model name for faster LIKE queries
    sqlx::query("CREATE INDEX IF NOT EXISTS idx_models_name ON models(name)")
        .execute(pool)
        .await?;

    // Create model_files junction table for per-shard OID tracking
    sqlx::query(
        r#"
        CREATE TABLE IF NOT EXISTS model_files (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            model_id INTEGER NOT NULL,
            file_path TEXT NOT NULL,
            file_index INTEGER NOT NULL,
            expected_size INTEGER NOT NULL,
            hf_oid TEXT,
            last_verified_at TEXT,
            FOREIGN KEY (model_id) REFERENCES models(id) ON DELETE CASCADE,
            UNIQUE (model_id, file_path)
        )
        "#,
    )
    .execute(pool)
    .await?;

    // Index on model_id for faster model_files lookups
    sqlx::query("CREATE INDEX IF NOT EXISTS idx_model_files_model_id ON model_files(model_id)")
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

    // Guard: drop chat tables if the schema is out of date (missing 'tool' role).
    // No backwards-compat needed — tables are recreated below.
    let needs_recreate: bool = sqlx::query_scalar::<_, String>(
        "SELECT sql FROM sqlite_master WHERE type='table' AND name='chat_messages'",
    )
    .fetch_optional(pool)
    .await?
    .is_some_and(|sql| !sql.contains("'tool'"));

    if needs_recreate {
        // Drop messages first (FK child), then conversations.
        sqlx::query("DROP TABLE IF EXISTS chat_messages")
            .execute(pool)
            .await?;
        sqlx::query("DROP TABLE IF EXISTS chat_conversations")
            .execute(pool)
            .await?;
    }

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
            role TEXT NOT NULL CHECK(role IN ('system', 'user', 'assistant', 'tool')),
            content TEXT NOT NULL,
            metadata TEXT,
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

    // Migration: Add metadata column for tool usage, etc.
    let _ = sqlx::query(r#"ALTER TABLE chat_messages ADD COLUMN metadata TEXT"#)
        .execute(pool)
        .await;
    // Ignore error if column already exists

    // Migration: Add settings column for session parameter persistence.
    let _ = sqlx::query(r#"ALTER TABLE chat_conversations ADD COLUMN settings TEXT"#)
        .execute(pool)
        .await;
    // Ignore error if column already exists

    // Create MCP servers table
    sqlx::query(
        r#"
        CREATE TABLE IF NOT EXISTS mcp_servers (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            name TEXT NOT NULL,
            type TEXT NOT NULL CHECK (type IN ('stdio', 'sse')),
            enabled INTEGER NOT NULL DEFAULT 1,
            lifecycle TEXT NOT NULL DEFAULT 'lazy' CHECK (lifecycle IN ('eager', 'lazy', 'manual')),
            command TEXT,
            resolved_path_cache TEXT,
            args TEXT NOT NULL DEFAULT '[]',
            cwd TEXT,
            path_extra TEXT,
            url TEXT,
            created_at TEXT NOT NULL DEFAULT (datetime('now')),
            last_connected_at TEXT,
            is_valid INTEGER NOT NULL DEFAULT 0,
            last_error TEXT
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

    // Create orchestrator_runs table for persistent council run records.
    // Table name: orchestrator_runs — historical name, kept for schema compatibility.
    sqlx::query(
        r#"
        CREATE TABLE IF NOT EXISTS orchestrator_runs ( -- historical name
            id TEXT PRIMARY KEY NOT NULL,
            goal TEXT NOT NULL,
            graph_json TEXT,
            status TEXT NOT NULL,
            hitl_mode TEXT NOT NULL,
            conversation_id INTEGER,
            created_at TEXT NOT NULL DEFAULT (datetime('now')),
            updated_at TEXT NOT NULL DEFAULT (datetime('now'))
        )
        "#,
    )
    .execute(pool)
    .await?;

    // Index to allow efficient listing by status.
    sqlx::query(
        "CREATE INDEX IF NOT EXISTS idx_orchestrator_runs_status ON orchestrator_runs(status)",
    )
    .execute(pool)
    .await?;

    // Index to allow efficient ordering by creation time.
    sqlx::query(
        "CREATE INDEX IF NOT EXISTS idx_orchestrator_runs_created ON orchestrator_runs(created_at)",
    )
    .execute(pool)
    .await?;

    // Create orchestrator_events table for the append-only council event log.
    // Table name: orchestrator_events — historical name, kept for schema compatibility.
    sqlx::query(
        r#"
        CREATE TABLE IF NOT EXISTS orchestrator_events ( -- historical name
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            run_id TEXT NOT NULL,
            seq INTEGER NOT NULL,
            event_json TEXT NOT NULL,
            created_at TEXT NOT NULL DEFAULT (datetime('now')),
            wave_index INTEGER NOT NULL DEFAULT 0,
            FOREIGN KEY (run_id) REFERENCES orchestrator_runs(id) ON DELETE CASCADE,
            UNIQUE (run_id, seq)
        )
        "#,
    )
    .execute(pool)
    .await?;

    // Phase M migration: add wave_index to existing databases created before
    // this column existed.  Must run before any index that references the
    // column so that the CREATE INDEX below succeeds on old databases.
    // This is a no-op on fresh databases (column already present).
    let _ = sqlx::query(
        "ALTER TABLE orchestrator_events ADD COLUMN wave_index INTEGER NOT NULL DEFAULT 0",
    )
    .execute(pool)
    .await; // intentionally ignore the error (column already exists)

    // Index to allow efficient event retrieval per run.
    sqlx::query(
        "CREATE INDEX IF NOT EXISTS idx_orchestrator_events_run ON orchestrator_events(run_id, seq)",
    )
    .execute(pool)
    .await?;

    // Index to allow efficient rewind lookups per run + wave.
    sqlx::query(
        "CREATE INDEX IF NOT EXISTS idx_orchestrator_events_wave ON orchestrator_events(run_id, wave_index)",
    )
    .execute(pool)
    .await?;

    // ── Benchmark tables ─────────────────────────────────────────────────────

    // Lightweight grouping record; results reference this via SET NULL FK so
    // deleting a run does not delete the per-model data.
    sqlx::query(
        "CREATE TABLE IF NOT EXISTS benchmark_runs (
            id           INTEGER PRIMARY KEY,
            run_type     TEXT    NOT NULL,
            status       TEXT    NOT NULL,
            model_ids    TEXT    NOT NULL,
            prompt_text  TEXT,
            system_prompt TEXT,
            config_json  TEXT,
            error        TEXT,
            created_at   TEXT    NOT NULL,
            completed_at TEXT
        )",
    )
    .execute(pool)
    .await?;

    // Per-model compare results: real inference quality + real-world timing.
    // Timing fields are nullable — llama-server may omit the timings object.
    sqlx::query(
        "CREATE TABLE IF NOT EXISTS model_compare_results (
            id               INTEGER PRIMARY KEY,
            model_id         INTEGER NOT NULL REFERENCES models(id) ON DELETE CASCADE,
            run_id           INTEGER REFERENCES benchmark_runs(id) ON DELETE SET NULL,
            prompt_text      TEXT    NOT NULL,
            system_prompt    TEXT,
            response_text    TEXT    NOT NULL,
            was_truncated    INTEGER NOT NULL DEFAULT 0,
            prompt_tokens    INTEGER,
            completion_tokens INTEGER,
            prompt_ms        REAL,
            generation_ms    REAL,
            prompt_tps       REAL,
            generation_tps   REAL,
            created_at       TEXT    NOT NULL
        )",
    )
    .execute(pool)
    .await?;

    // Per-model perf results: synthetic llama-bench throughput.
    sqlx::query(
        "CREATE TABLE IF NOT EXISTS model_perf_results (
            id           INTEGER PRIMARY KEY,
            model_id     INTEGER NOT NULL REFERENCES models(id) ON DELETE CASCADE,
            run_id       INTEGER REFERENCES benchmark_runs(id) ON DELETE SET NULL,
            pp_tps       REAL    NOT NULL,
            tg_tps       REAL    NOT NULL,
            pp_tokens    INTEGER NOT NULL,
            tg_tokens    INTEGER NOT NULL,
            backend      TEXT,
            ngl          INTEGER,
            context_size INTEGER,
            repetitions  INTEGER NOT NULL,
            created_at   TEXT    NOT NULL
        )",
    )
    .execute(pool)
    .await?;

    // 1:1 with models; upserted on every result save; LEFT JOINed into model
    // list so the frontend can show speed badges without extra round-trips.
    sqlx::query(
        "CREATE TABLE IF NOT EXISTS model_benchmark_summaries (
            model_id             INTEGER PRIMARY KEY REFERENCES models(id) ON DELETE CASCADE,
            best_tg_tps          REAL,
            best_pp_tps          REAL,
            latest_tg_tps        REAL,
            latest_pp_tps        REAL,
            latest_backend       TEXT,
            perf_run_count       INTEGER NOT NULL DEFAULT 0,
            compare_run_count    INTEGER NOT NULL DEFAULT 0,
            last_benchmarked_at  TEXT    NOT NULL,
            updated_at           TEXT    NOT NULL
        )",
    )
    .execute(pool)
    .await?;

    // Indexes for common benchmark queries
    sqlx::query(
        "CREATE INDEX IF NOT EXISTS idx_compare_results_model ON model_compare_results(model_id, created_at DESC)",
    )
    .execute(pool)
    .await?;

    sqlx::query(
        "CREATE INDEX IF NOT EXISTS idx_perf_results_model ON model_perf_results(model_id, created_at DESC)",
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
