//! The `models` table: its definition, the columns added to it since, its
//! indexes, and the one-time rebuild that stops it reusing ids.
//!
//! A `#[path]` child of `setup.rs`, whose `create_schema` creates the table
//! and its indexes first, before any table that references `models`, and runs
//! the rebuild last, once every table the rebuild reads exists.

use anyhow::{Context, Result};
use sqlx::{SqliteConnection, SqlitePool};

use super::add_column_if_missing;

/// The `CREATE TABLE` statement for `models`, naming the table `table`.
///
/// `AUTOINCREMENT` makes an id permanent: a removed model's id is never given
/// to the next one, so an id remembered elsewhere (a conversation, a benchmark
/// run, another machine) names that model or none.
fn models_ddl(table: &str) -> String {
    format!(
        r"
        CREATE TABLE IF NOT EXISTS {table} (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            name TEXT NOT NULL,
            file_path TEXT NOT NULL,
            param_count_b REAL NOT NULL,
            architecture TEXT,
            quantization TEXT,
            context_length INTEGER,
            inference_defaults TEXT,
            defaults_origin TEXT,
            server_defaults TEXT,
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
            capabilities INTEGER DEFAULT 0,
            dialect_spec TEXT,
            template_caps TEXT
        )
        ",
    )
}

/// Creates `models`, and adds the columns a library from before them lacks.
pub(super) async fn create_models_table(pool: &SqlitePool) -> Result<()> {
    // Create the models table
    sqlx::query(&models_ddl("models")).execute(pool).await?;

    // Migration: add defaults_origin to models — tracks whether
    // `inference_defaults` was set by the user or auto-detected at import
    // time (see `gglib_core::domain::DefaultsOrigin`), so resolution can
    // rank an auto-detected guess below the user's own global settings
    // instead of silently outranking them. No batch backfill for rows
    // written before this column existed — `row_to_model` derives an answer
    // for those from `inference_defaults` itself on every read instead (see
    // `row_mappers::resolve_defaults_origin`), so a backfill pass would only
    // duplicate work every row already gets for free.
    add_column_if_missing(pool, "models", "defaults_origin", "TEXT").await?;

    // Migration: add dialect_spec to models — the structured tool-call
    // dialect detected at import/retag time (JSON-serialized
    // `gglib_core::domain::DialectSpec`). No backfill: rows without a spec
    // fall back to their `format:*` tag at context-resolution time, and
    // `gglib model retag` re-derives the spec from persisted metadata.
    add_column_if_missing(pool, "models", "dialect_spec", "TEXT").await?;

    // Migration: add template_caps to models — llama-server's per-template
    // capability self-report (`chat_template_caps` from GET /props),
    // JSON-serialized `gglib_core::domain::TemplateCaps`, recorded after a
    // launch observes it (ADR 0007). No backfill, necessarily: the caps are
    // a fact about the binary–model pair that only a launch can learn, and a
    // NULL here *is* the tri-state's "never observed" — manufacturing a
    // value would collapse it into an answer nobody measured.
    add_column_if_missing(pool, "models", "template_caps", "TEXT").await?;

    Ok(())
}

/// Creates the indexes on `models`, over `conn`.
pub(super) async fn create_model_indexes(conn: &mut SqliteConnection) -> Result<()> {
    // Index on file path for lookups (not unique)
    sqlx::query("CREATE INDEX IF NOT EXISTS idx_models_file_path ON models(file_path)")
        .execute(&mut *conn)
        .await?;

    // Unique index on model_key (canonical identity)
    sqlx::query("CREATE UNIQUE INDEX IF NOT EXISTS idx_models_model_key ON models(model_key)")
        .execute(&mut *conn)
        .await?;

    // Index on model name for faster LIKE queries
    sqlx::query("CREATE INDEX IF NOT EXISTS idx_models_name ON models(name)")
        .execute(&mut *conn)
        .await?;

    Ok(())
}

/// Whether `models` is declared `AUTOINCREMENT`.
async fn never_reuses_ids(conn: &mut SqliteConnection) -> Result<bool> {
    let sql: String = sqlx::query_scalar(
        "SELECT sql FROM sqlite_master WHERE type = 'table' AND name = 'models'",
    )
    .fetch_one(&mut *conn)
    .await?;
    Ok(sql.to_ascii_uppercase().contains("AUTOINCREMENT"))
}

/// Rebuilds a `models` table made without `AUTOINCREMENT`, keeping every row,
/// every id and every row elsewhere that references one.
///
/// `SQLite` cannot add `AUTOINCREMENT` to a table in place, so this is its
/// documented table rebuild: the new table is made beside the old, filled, and
/// renamed over it. It runs last in `create_schema`, because the sequence it
/// seeds reads `chat_conversations`, `settings_kv` and the benchmark tables,
/// which a library old enough to need it may lack until `create_schema` makes
/// them.
///
/// Foreign keys are off for the rebuild, or dropping the old table would
/// cascade into `model_files` and every benchmark table and null
/// `chat_conversations.model_id`. `SQLite` ignores that pragma inside a
/// transaction, so it is set on one connection before `BEGIN`. A failure
/// anywhere after rolls the transaction back and detaches the connection, so
/// none goes back to the pool with foreign keys off.
///
/// Two processes opening one old library at once both get this far. `BEGIN
/// IMMEDIATE` lets one write at a time, and the shape is asked again inside the
/// transaction, so the second finds the work done and changes nothing.
pub(super) async fn rebuild_models_if_needed(pool: &SqlitePool) -> Result<()> {
    let mut conn = pool.acquire().await?;
    if never_reuses_ids(&mut conn).await? {
        return Ok(());
    }
    match rebuild_models(&mut conn).await {
        Ok(()) => Ok(()),
        Err(error) => {
            drop(conn.detach());
            Err(error)
        }
    }
}

/// The rebuild on `conn`, with foreign keys off around its transaction.
async fn rebuild_models(conn: &mut SqliteConnection) -> Result<()> {
    sqlx::query("PRAGMA foreign_keys = OFF")
        .execute(&mut *conn)
        .await?;
    sqlx::query("BEGIN IMMEDIATE").execute(&mut *conn).await?;
    if let Err(error) = rebuild_models_in_transaction(conn).await {
        return match sqlx::query("ROLLBACK").execute(&mut *conn).await {
            Ok(_) => Err(error),
            Err(rollback) => Err(error.context(format!("and the rollback failed: {rollback}"))),
        };
    }
    sqlx::query("COMMIT").execute(&mut *conn).await?;
    sqlx::query("PRAGMA foreign_keys = ON")
        .execute(&mut *conn)
        .await?;
    Ok(())
}

/// The rebuild's statements, inside the transaction `conn` holds.
async fn rebuild_models_in_transaction(conn: &mut SqliteConnection) -> Result<()> {
    if never_reuses_ids(conn).await? {
        return Ok(());
    }
    // A library can already hold rows that reference no model. Those must not
    // stop it booting; only a row the rebuild itself orphans may. Counted under
    // the lock, so no other process can change the count before the rebuild.
    let violations_before = foreign_key_violations(conn).await?;
    sqlx::query(&models_ddl("models_new"))
        .execute(&mut *conn)
        .await?;

    // Every column both tables have, `id` and `file_paths_json` among them,
    // each by name: one the old table lacks takes its default, and one only
    // the old table has goes with it.
    let columns: Vec<String> = sqlx::query_scalar(
        "SELECT name FROM pragma_table_info('models') \
         WHERE name IN (SELECT name FROM pragma_table_info('models_new')) ORDER BY cid",
    )
    .fetch_all(&mut *conn)
    .await?;
    let columns = columns
        .iter()
        .map(|column| format!("\"{}\"", column.replace('"', "\"\"")))
        .collect::<Vec<_>>()
        .join(", ");
    sqlx::query(&format!(
        "INSERT INTO models_new ({columns}) SELECT {columns} FROM models"
    ))
    .execute(&mut *conn)
    .await?;

    // Drop, then rename: in that order the other tables' references to
    // `models` keep their text and land on the new table.
    sqlx::query("DROP TABLE models").execute(&mut *conn).await?;
    sqlx::query("ALTER TABLE models_new RENAME TO models")
        .execute(&mut *conn)
        .await?;
    create_model_indexes(conn).await?;

    let violations_after = foreign_key_violations(conn).await?;
    anyhow::ensure!(
        violations_after <= violations_before,
        "rebuilding the models table would leave {violations_after} rows referencing \
         a model that is not there, where there were {violations_before}"
    );

    // The highest id anything has held. A removed model's id can live on in a
    // conversation, a benchmark run's list, the default-model setting (a JSON
    // value) or a row the library already had that references no model, and
    // the next model must not be given it, or take over that row. A list that
    // is not JSON adds nothing rather than failing the boot.
    let highest: i64 = sqlx::query_scalar(
        "SELECT COALESCE(MAX(seen), 0) FROM (
            SELECT MAX(id) AS seen FROM models
            UNION ALL SELECT MAX(model_id) FROM model_files
            UNION ALL SELECT MAX(model_id) FROM model_compare_results
            UNION ALL SELECT MAX(model_id) FROM model_perf_results
            UNION ALL SELECT MAX(model_id) FROM model_benchmark_summaries
            UNION ALL SELECT MAX(model_id) FROM benchmark_tune_results
            UNION ALL SELECT MAX(model_id) FROM benchmark_agentic_results
            UNION ALL SELECT MAX(model_id) FROM chat_conversations
            UNION ALL SELECT MAX(ids.value) FROM benchmark_runs AS runs,
                json_each(IIF(json_valid(runs.model_ids), runs.model_ids, '[]')) AS ids
                WHERE ids.type = 'integer'
            UNION ALL SELECT CAST(value AS INTEGER) FROM settings_kv
                WHERE key = 'default_model_id' AND json_valid(value)
                AND json_type(value) = 'integer'
        )",
    )
    .fetch_one(&mut *conn)
    .await
    .context("reading the highest model id anything holds")?;
    sqlx::query("DELETE FROM sqlite_sequence WHERE name = 'models'")
        .execute(&mut *conn)
        .await?;
    sqlx::query("INSERT INTO sqlite_sequence (name, seq) VALUES ('models', ?)")
        .bind(highest)
        .execute(&mut *conn)
        .await?;
    Ok(())
}

/// How many rows `PRAGMA foreign_key_check` reports.
async fn foreign_key_violations(conn: &mut SqliteConnection) -> Result<i64> {
    let count = sqlx::query_scalar("SELECT COUNT(*) FROM pragma_foreign_key_check")
        .fetch_one(&mut *conn)
        .await?;
    Ok(count)
}
