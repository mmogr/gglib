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

/// `PRAGMA user_version` once the canonical-path backfills have run.
///
/// Databases predating this carry `0`, SQLite's default. The first version
/// this project has assigned; anything later must take a higher number and
/// leave this one meaning what it means now.
const CANONICAL_PATH_SCHEMA_VERSION: i64 = 1;

/// Whether a `SQLite` error is a unique-index violation.
///
/// Used to tell the one failure the path backfill deliberately tolerates —
/// two rows resolving onto one key — apart from a locked or full database,
/// which must surface rather than be counted as a tidy skip.
fn is_unique_violation(error: &sqlx::Error) -> bool {
    matches!(
        error,
        sqlx::Error::Database(db) if db.code().as_deref() == Some("2067")
            || db.code().as_deref() == Some("1555")
    )
}

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

    // The idle-time auto-tune scheduler was removed; reclaim its settings row.
    //
    // `Settings` is `#[serde(default)]` and nothing validates the key set, so
    // a stale row is silently dropped at load and would never break anything —
    // but `save()` iterates only the serialised struct, so it would also never
    // be swept. House style reclaims dropped *tables*; an orphan key would
    // otherwise sit in every existing database forever, reading like a setting
    // that still does something.
    sqlx::query("DELETE FROM settings_kv WHERE key = 'auto_tune'")
        .execute(&pool)
        .await?;

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
            dialect_spec TEXT
        )
        "#,
    )
    .execute(pool)
    .await?;

    // Migration: add defaults_origin to models — tracks whether
    // `inference_defaults` was set by the user or auto-detected at import
    // time (see `gglib_core::domain::DefaultsOrigin`), so resolution can
    // rank an auto-detected guess below the user's own global settings
    // instead of silently outranking them. No batch backfill for rows
    // written before this column existed — `row_to_model` derives an answer
    // for those from `inference_defaults` itself on every read instead (see
    // `row_mappers::resolve_defaults_origin`), so a backfill pass would only
    // duplicate work every row already gets for free.
    let _ = sqlx::query(r#"ALTER TABLE models ADD COLUMN defaults_origin TEXT"#)
        .execute(pool)
        .await;
    // Ignore error if column already exists

    // Migration: add dialect_spec to models — the structured tool-call
    // dialect detected at import/retag time (JSON-serialized
    // `gglib_core::domain::DialectSpec`). No backfill: rows without a spec
    // fall back to their `format:*` tag at context-resolution time, and
    // `gglib model retag` re-derives the spec from persisted metadata.
    let _ = sqlx::query(r#"ALTER TABLE models ADD COLUMN dialect_spec TEXT"#)
        .execute(pool)
        .await;
    // Ignore error if column already exists

    // Index on file path for lookups (no longer unique)
    sqlx::query("CREATE INDEX IF NOT EXISTS idx_models_file_path ON models(file_path)")
        .execute(pool)
        .await?;

    // Unique index on model_key (canonical identity)
    sqlx::query("CREATE UNIQUE INDEX IF NOT EXISTS idx_models_model_key ON models(model_key)")
        .execute(pool)
        .await?;

    // The path/key backfills below resolve every stored path, which is a
    // blocking syscall per row and per shard entry — on Windows that opens a
    // handle to each file, so an ungated pass would stall launch on a model
    // living on a sleeping drive or a dead mount. Gated on `user_version` so
    // the cost is paid once per library.
    //
    // The gate makes the repair one-shot, which is a real trade and not merely
    // an optimisation: once stamped, a row written under the old key rule
    // afterwards is never repaired. That needs an older binary writing to an
    // already-migrated database — mixed-version writers are out of scope, the
    // usual bargain for a version-stamped migration. The passes themselves
    // stay idempotent, so a crash before the stamp simply re-runs them.
    let schema_version: i64 = sqlx::query_scalar("PRAGMA user_version")
        .fetch_one(pool)
        .await?;
    if schema_version < CANONICAL_PATH_SCHEMA_VERSION {
        backfill_local_model_keys(pool).await?;
        backfill_shard_path_lists(pool).await?;
        // Not bindable as a parameter: SQLite requires a literal here.
        sqlx::query(&format!(
            "PRAGMA user_version = {CANONICAL_PATH_SCHEMA_VERSION}"
        ))
        .execute(pool)
        .await?;
    }

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

    // The council/orchestrator feature was removed; reclaim its tables.
    // Events first — it holds the ON DELETE CASCADE foreign key into runs.
    // Backwards compatibility is deliberately not preserved: any stored run
    // history is dropped rather than migrated.
    sqlx::query("DROP TABLE IF EXISTS orchestrator_events")
        .execute(pool)
        .await?;
    sqlx::query("DROP TABLE IF EXISTS orchestrator_runs")
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
            applied_json TEXT,
            error        TEXT,
            created_at   TEXT    NOT NULL,
            completed_at TEXT
        )",
    )
    .execute(pool)
    .await?;

    // Migration: add applied_json to benchmark_runs — the JSON-serialized
    // apply record (`tune::apply::ApplyRecord`) written when a tune run's
    // winner is stored as a model's Measured defaults, so the provenance a
    // model reports can always be traced back to the gate numbers that
    // licensed it. NULL on every run that was never applied.
    //
    // This ALTER must run *after* the CREATE above: it originally sat in
    // the models migration block, before benchmark_runs existed on a fresh
    // database — where "no such table" was silently swallowed and the CREATE
    // (which then lacked the column) left every fresh install unable to
    // store an apply record until a second boot re-ran the migration.
    let _ = sqlx::query(r#"ALTER TABLE benchmark_runs ADD COLUMN applied_json TEXT"#)
        .execute(pool)
        .await;
    // Ignore error if column already exists

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

    // Per-model tune candidate results. `config_json`/`source_json`/
    // `task_results_json` store the corresponding `InferenceConfig`,
    // `CandidateSource`, and `Vec<TuneTaskResult>` domain types respectively —
    // no separate normalized tables, mirroring how `benchmark_runs.config_json`
    // already stores whole-config JSON blobs elsewhere in this schema.
    sqlx::query(
        "CREATE TABLE IF NOT EXISTS benchmark_tune_results (
            id                INTEGER PRIMARY KEY,
            model_id          INTEGER NOT NULL REFERENCES models(id) ON DELETE CASCADE,
            run_id            INTEGER REFERENCES benchmark_runs(id) ON DELETE SET NULL,
            config_json       TEXT    NOT NULL,
            source_json       TEXT    NOT NULL,
            composite_score   REAL    NOT NULL,
            pruned            INTEGER NOT NULL DEFAULT 0,
            tg_tps            REAL,
            task_results_json TEXT    NOT NULL,
            created_at        TEXT    NOT NULL
        )",
    )
    .execute(pool)
    .await?;

    // One row per raw-vs-gglib A/B run. The whole `AgenticEvalReport` is
    // stored as a JSON blob for the same reason the tune table stores its
    // task results that way: the report is a leaderboard interchange format
    // read back whole, and normalizing per-arm scores into columns would buy
    // queries nobody makes. The scalar columns are the ones worth filtering
    // and sorting on.
    sqlx::query(
        "CREATE TABLE IF NOT EXISTS benchmark_agentic_results (
            id                INTEGER PRIMARY KEY,
            model_id          INTEGER NOT NULL REFERENCES models(id) ON DELETE CASCADE,
            run_id            INTEGER REFERENCES benchmark_runs(id) ON DELETE SET NULL,
            raw_composite     REAL    NOT NULL,
            gglib_composite   REAL    NOT NULL,
            report_json       TEXT    NOT NULL,
            created_at        TEXT    NOT NULL
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

    sqlx::query(
        "CREATE INDEX IF NOT EXISTS idx_tune_results_model ON benchmark_tune_results(model_id, created_at DESC)",
    )
    .execute(pool)
    .await?;

    sqlx::query(
        "CREATE INDEX IF NOT EXISTS idx_agentic_results_model ON benchmark_agentic_results(model_id, created_at DESC)",
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

/// Migration: put local models' paths and keys onto the canonical rule.
///
/// A `local:` key is a hash of the model's path. Until recently the hash was
/// taken over the path *as the caller spelled it* — `gglib model add
/// ./model.gguf` hashed `./model.gguf` — while `file_path` was normalised on
/// insert. Rows added through a relative path, a symlinked models directory,
/// or a macOS temp path (`/var` → `/private/var`) therefore carry a key no
/// current build will ever recompute.
///
/// The consequence is silent and bad: `ON CONFLICT(model_key)` misses the row,
/// `file_path` carries no unique index, and re-registering that file appends a
/// second one — which is exactly the duplicate this whole area exists to
/// prevent, arriving by way of the upgrade rather than by way of a bug.
///
/// **The stored column cannot simply be trusted.** `insert` normalised it, but
/// `update` did not until this change, so any row that went through
/// `PATCH /api/models/{id}` holds whatever spelling the caller sent; and
/// `insert`'s normalisation falls back to the literal path when the file is
/// missing. Re-keying from the column verbatim would therefore compute a key
/// from a non-canonical string, leaving the row exactly as unreachable as
/// before while reporting success. So each path is resolved here first, the
/// column rewritten when it moves, and the key derived from the resolved form
/// — the same value `insert` would compute for that file today.
///
/// Idempotent: once a row is normalised the recomputed values equal the stored
/// ones and no write happens. A row whose new key would collide with another
/// row's is left alone rather than failing the unique index — that collision
/// means two rows genuinely name one file, which is a merge a startup
/// migration has no business performing silently. Any other database error is
/// propagated rather than counted as a skip.
async fn backfill_local_model_keys(pool: &SqlitePool) -> Result<()> {
    use crate::repositories::sqlite_model_repository::local_model_key_for;
    use gglib_core::paths::canonical_model_path_string;
    use sqlx::Row;
    use std::path::Path;

    let rows =
        sqlx::query("SELECT id, file_path, model_key FROM models WHERE model_key LIKE 'local:%'")
            .fetch_all(pool)
            .await?;

    let mut rekeyed = 0_usize;
    let mut skipped = 0_usize;
    for row in &rows {
        let id: i64 = row.try_get("id")?;
        let file_path: String = row.try_get("file_path")?;
        let stored_key: String = row.try_get("model_key")?;

        let canonical = canonical_model_path_string(Path::new(&file_path));
        let expected = local_model_key_for(&canonical);
        if expected == stored_key && canonical == file_path {
            continue;
        }

        match sqlx::query("UPDATE models SET model_key = ?, file_path = ? WHERE id = ?")
            .bind(&expected)
            .bind(&canonical)
            .bind(id)
            .execute(pool)
            .await
        {
            Ok(_) => rekeyed += 1,
            // Only the collision described above is absorbed. A locked or
            // full database must not be reported as a tidy "skipped".
            Err(e) if is_unique_violation(&e) => skipped += 1,
            Err(e) => return Err(e.into()),
        }
    }

    if rekeyed > 0 || skipped > 0 {
        tracing::info!(
            rekeyed,
            skipped,
            "re-keyed local models onto the canonical-path rule"
        );
    }
    Ok(())
}

/// Migration: canonicalise the stored shard path lists.
///
/// Companion to [`backfill_local_model_keys`], and needed for the same reason:
/// `file_paths_json` used to be written exactly as the download handed it over
/// — absolute, but never symlink-resolved — while the duplicate lookup
/// compares those entries against a resolved path.
///
/// Left alone, `gglib model add <shard-2>` against a sharded model already in
/// the library matches nothing, so the add proceeds and appends a second row
/// for a model that is already there. Unlike the key backfill this needs no
/// `--force` to reach: it is the plain add path.
///
/// Applies to every row with a shard list, not only `local:` ones — a
/// downloaded sharded model keeps its `hf:` key but had its paths written the
/// same raw way. Idempotent: once resolved, re-resolving is the identity.
async fn backfill_shard_path_lists(pool: &SqlitePool) -> Result<()> {
    use gglib_core::paths::canonical_model_path_string;
    use sqlx::Row;
    use std::path::Path;

    let rows =
        sqlx::query("SELECT id, file_paths_json FROM models WHERE file_paths_json IS NOT NULL")
            .fetch_all(pool)
            .await?;

    let mut rewritten = 0_usize;
    for row in &rows {
        let id: i64 = row.try_get("id")?;
        let stored: String = row.try_get("file_paths_json")?;

        // A column written before this serialisation existed, or by hand, is
        // left exactly as found rather than guessed at.
        let Ok(paths) = serde_json::from_str::<Vec<String>>(&stored) else {
            continue;
        };

        let resolved: Vec<String> = paths
            .iter()
            .map(|p| canonical_model_path_string(Path::new(p)))
            .collect();
        if resolved == paths {
            continue;
        }

        let Ok(encoded) = serde_json::to_string(&resolved) else {
            continue;
        };
        sqlx::query("UPDATE models SET file_paths_json = ? WHERE id = ?")
            .bind(&encoded)
            .bind(id)
            .execute(pool)
            .await?;
        rewritten += 1;
    }

    if rewritten > 0 {
        tracing::info!(rewritten, "canonicalised stored shard path lists");
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A row written by a build that hashed the path as the user spelled it
    /// must be re-keyed onto the canonical rule, or it stays unreachable by
    /// `ON CONFLICT(model_key)` and the next registration of that file
    /// silently appends a second row.
    ///
    /// The non-canonical spelling is built with `..` rather than borrowed from
    /// the platform. A `tempfile` directory is non-canonical on macOS, where
    /// `/var` resolves through `/private/var`, and already canonical on Linux
    /// — so leaning on it wrote a test that passed on the machine it was
    /// written on and failed in CI. `..` survives as a real path component
    /// everywhere, which is what makes the two spellings differ on every
    /// platform.
    #[tokio::test]
    async fn backfill_rekeys_a_row_stored_under_a_non_canonical_spelling() {
        use crate::repositories::sqlite_model_repository::local_model_key_for;
        use std::collections::hash_map::DefaultHasher;
        use std::hash::{Hash, Hasher};

        let pool = setup_test_database().await.unwrap();

        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("Legacy.gguf");
        std::fs::File::create(&file).unwrap();
        std::fs::create_dir(dir.path().join("sub")).unwrap();

        let spelled = dir.path().join("sub").join("..").join("Legacy.gguf");
        let canonical = std::fs::canonicalize(&file).unwrap();
        assert_ne!(
            spelled, canonical,
            "this test needs the two spellings to differ"
        );

        // Exactly what a previous build wrote: canonical column, key hashed
        // from the path it was handed.
        let legacy_key = {
            let mut hasher = DefaultHasher::new();
            spelled.hash(&mut hasher);
            format!("local:{:x}", hasher.finish())
        };
        sqlx::query(
            "INSERT INTO models (name, file_path, param_count_b, added_at, model_key) \
             VALUES (?, ?, ?, ?, ?)",
        )
        .bind("Legacy")
        .bind(canonical.to_string_lossy().as_ref())
        .bind(7.0_f64)
        .bind(chrono::Utc::now().to_string())
        .bind(&legacy_key)
        .execute(&pool)
        .await
        .unwrap();

        backfill_local_model_keys(&pool).await.unwrap();

        let key: String = sqlx::query_scalar("SELECT model_key FROM models WHERE name = 'Legacy'")
            .fetch_one(&pool)
            .await
            .unwrap();

        assert_eq!(
            key,
            local_model_key_for(canonical.to_string_lossy().as_ref()),
            "the stranded row must be re-keyed onto the rule this build computes"
        );
        assert_ne!(key, legacy_key, "the key must actually have moved");

        // Idempotent: a second pass is a no-op.
        backfill_local_model_keys(&pool).await.unwrap();
        let again: String =
            sqlx::query_scalar("SELECT model_key FROM models WHERE name = 'Legacy'")
                .fetch_one(&pool)
                .await
                .unwrap();
        assert_eq!(again, key);
    }

    /// A row whose stored `file_path` is itself non-canonical must be repaired,
    /// not merely re-keyed from the bad value.
    ///
    /// `insert` normalised the column but `update` did not, so any row that
    /// went through `PATCH /api/models/{id}` holds whatever spelling the caller
    /// sent — and `insert`'s own normalisation falls back to the literal path
    /// when the file is missing. Hashing the column verbatim would compute a
    /// key from that non-canonical string and leave the row exactly as
    /// unreachable as before, while reporting success.
    #[tokio::test]
    async fn backfill_repairs_a_row_whose_stored_path_is_not_canonical() {
        use crate::repositories::sqlite_model_repository::local_model_key_for;

        let pool = setup_test_database().await.unwrap();

        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("Patched.gguf");
        std::fs::File::create(&file).unwrap();
        std::fs::create_dir(dir.path().join("sub")).unwrap();

        // What pre-fix `update` left behind: a spelling only the filesystem
        // can equate with the real path.
        let non_canonical = dir.path().join("sub").join("..").join("Patched.gguf");
        let canonical = std::fs::canonicalize(&file).unwrap();
        assert_ne!(
            non_canonical.to_string_lossy(),
            canonical.to_string_lossy(),
            "the fixture must actually be non-canonical"
        );

        sqlx::query(
            "INSERT INTO models (name, file_path, param_count_b, added_at, model_key) \
             VALUES (?, ?, ?, ?, ?)",
        )
        .bind("Patched")
        .bind(non_canonical.to_string_lossy().as_ref())
        .bind(7.0_f64)
        .bind(chrono::Utc::now().to_string())
        .bind(local_model_key_for(
            non_canonical.to_string_lossy().as_ref(),
        ))
        .execute(&pool)
        .await
        .unwrap();

        backfill_local_model_keys(&pool).await.unwrap();

        let (stored_path, stored_key): (String, String) =
            sqlx::query_as("SELECT file_path, model_key FROM models WHERE name = 'Patched'")
                .fetch_one(&pool)
                .await
                .unwrap();

        assert_eq!(
            stored_path,
            canonical.to_string_lossy(),
            "the column itself must be repaired, or find_by_path still misses it"
        );
        assert_eq!(
            stored_key,
            local_model_key_for(canonical.to_string_lossy().as_ref()),
            "the key must be the one a fresh `model add` of this file computes"
        );
    }

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
