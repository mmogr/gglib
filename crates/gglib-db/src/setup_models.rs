//! The `models` table: its definition, the columns added to it since, and
//! its indexes.
//!
//! A `#[path]` child of `setup.rs`, whose `create_schema` calls both
//! functions here first, before any table that references `models`.

use anyhow::Result;
use sqlx::SqlitePool;

use super::add_column_if_missing;

/// The `CREATE TABLE` statement for `models`, naming the table `table`.
fn models_ddl(table: &str) -> String {
    format!(
        r"
        CREATE TABLE IF NOT EXISTS {table} (
            id INTEGER PRIMARY KEY,
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

/// Creates the indexes on `models`.
pub(super) async fn create_model_indexes(pool: &SqlitePool) -> Result<()> {
    // Index on file path for lookups (not unique)
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

    Ok(())
}
