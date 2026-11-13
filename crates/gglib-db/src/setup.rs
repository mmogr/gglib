//! Database setup and initialization.
//!
//! This module provides the `setup_database()` function for initializing
//! the `SQLite` database with full schema. Entry points call this with the
//! resolved database path.

use anyhow::{Context, Result};
use sqlx::{
    SqliteConnection, SqlitePool,
    sqlite::{SqliteConnectOptions, SqliteJournalMode, SqlitePoolOptions},
};
use std::path::Path;
use std::time::Duration;

use gglib_core::ports::GgufParserPort;

pub use attachments::sweep_unlinked_attachments;
pub use backup::backup_beside;

#[path = "setup_attachments.rs"]
mod attachments;
#[path = "setup_backup.rs"]
mod backup;
#[path = "setup_branches.rs"]
mod branches;
#[path = "setup_format_tags.rs"]
mod format_tags;
#[path = "setup_generation.rs"]
mod generation;
#[path = "setup_loop_guard.rs"]
mod loop_guard;
#[path = "setup_mcp_servers.rs"]
mod mcp_servers;
#[path = "setup_migrations.rs"]
mod migrations;
#[path = "setup_model_components.rs"]
mod model_components;
#[path = "setup_model_files.rs"]
mod model_files;
#[path = "setup_models.rs"]
mod models;
#[path = "setup_recipes.rs"]
mod recipes;
#[path = "setup_remote_grant.rs"]
mod remote_grant;
#[path = "setup_settings.rs"]
mod settings;
#[path = "setup_unknown_params.rs"]
mod unknown_params;

/// Whether a `SQLite` error is a unique-index violation.
///
/// Tells the one failure the path backfill deliberately tolerates — two rows
/// resolving onto one key — apart from a locked or full database, which must
/// surface rather than be counted as a tidy skip.
fn is_unique_violation(error: &sqlx::Error) -> bool {
    matches!(
        error,
        sqlx::Error::Database(db) if db.code().as_deref() == Some("2067")
            || db.code().as_deref() == Some("1555")
    )
}

/// Adds `column` to `table`, unless the table already has it.
///
/// Idempotent because it asks the database its shape (`PRAGMA table_info`),
/// not because it discards the ALTER's error: `duplicate column name` reads
/// identically to `no such table`, a locked database and a full disk. A column
/// already present is a skip; anything else the ALTER hits fails the boot
/// ([#921]).
///
/// **Columns are not on the `PRAGMA user_version` ladder.** Its versions gate
/// passes over rows that run once per database (`setup_migrations.rs`) and say
/// nothing of columns: a field database stamped `1` or later may or may not
/// carry `template_caps`, depending on which build last opened it, so a
/// version-gated ALTER that propagates errors would abort startup with
/// `duplicate column name`.
///
/// The identifiers are interpolated rather than bound: `SQLite` accepts no
/// parameters in DDL. Every caller passes a literal.
///
/// [#921]: https://github.com/mmogr/gglib/pull/921
async fn add_column_if_missing(
    pool: &SqlitePool,
    table: &str,
    column: &str,
    decl: &str,
) -> Result<()> {
    if column_present(pool, table, column).await? {
        return Ok(());
    }

    // Two processes opening one library at once may both find the column
    // missing. The second's ALTER then fails, and is a skip when the column
    // is there after all; any other failure still fails the boot.
    if let Err(error) = sqlx::query(&format!("ALTER TABLE {table} ADD COLUMN {column} {decl}"))
        .execute(pool)
        .await
    {
        if !column_present(pool, table, column).await? {
            return Err(error.into());
        }
    }
    Ok(())
}

/// Drops `column` from `table`, when the table still has it.
///
/// Asks the table's shape first, as `add_column_if_missing` does, and as it
/// does takes a failed `DROP` for a skip when another process opening the
/// library at once dropped the column first.
async fn drop_column_if_present(pool: &SqlitePool, table: &str, column: &str) -> Result<()> {
    if !column_present(pool, table, column).await? {
        return Ok(());
    }
    if let Err(error) = sqlx::query(&format!("ALTER TABLE {table} DROP COLUMN {column}"))
        .execute(pool)
        .await
    {
        if column_present(pool, table, column).await? {
            return Err(error).with_context(|| format!("dropping {table}.{column}"));
        }
    }
    Ok(())
}

/// Whether `table` has `column`, as `PRAGMA table_info` reads it.
async fn column_present(pool: &SqlitePool, table: &str, column: &str) -> Result<bool> {
    let present: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM pragma_table_info(?) WHERE name = ?")
            .bind(table)
            .bind(column)
            .fetch_one(pool)
            .await?;
    Ok(present > 0)
}

/// Sets up the `SQLite` database connection and ensures the schema exists.
///
/// This function:
/// 1. Establishes a connection to the `SQLite` database file
/// 2. Creates the database file if it doesn't exist; it and the directory holding it are made this user's alone
/// 3. Copies a file it is about to upgrade to a backup beside it
/// 4. Creates all tables and indexes
/// 5. Runs any necessary schema migrations, unless a newer build wrote the file
///
/// # Arguments
///
/// * `db_path` - Path to the `SQLite` database file
/// * `gguf_parser` - What reads a model file's header, for the one migration
///   that asks what a file is: linking an older library's models to their
///   projectors
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
/// use gglib_core::ports::NoopGgufParser;
/// use gglib_db::setup_database;
/// use std::path::Path;
///
/// # async fn example() -> anyhow::Result<()> {
/// let db_path = Path::new("/path/to/gglib.db");
/// let pool = setup_database(db_path, &NoopGgufParser).await?;
/// # Ok(())
/// # }
/// ```
pub async fn setup_database(
    db_path: &Path,
    gguf_parser: &dyn GgufParserPort,
) -> Result<SqlitePool> {
    // Private before SQLite opens it; `database_file::prepare` says why then.
    crate::database_file::prepare(db_path)?;
    crate::sqlite_init::ensure_initialised().await?;

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

    generation::refuse_newer(&pool, db_path).await?; // before anything is created or altered
    backup::before_upgrade(&pool, db_path).await?;
    create_schema_reading(&pool, gguf_parser).await?;
    generation::stamp(&pool).await?;

    Ok(pool)
}

/// Sets up an in-memory `SQLite` database for testing.
///
/// Creates a fresh in-memory database with the full production schema.
#[cfg(any(test, feature = "test-utils"))]
pub async fn setup_test_database() -> Result<SqlitePool> {
    crate::sqlite_init::ensure_initialised().await?;
    let pool = SqlitePool::connect("sqlite::memory:").await?;
    create_schema(&pool).await?;
    generation::stamp(&pool).await?;
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

/// [`create_schema_reading`] with a parser that reads every file as weights,
/// so an older library's models are linked to no projector. A fresh
/// database has the projector column from the start and nothing to link.
#[cfg(any(test, feature = "test-utils"))]
async fn create_schema(pool: &SqlitePool) -> Result<()> {
    create_schema_reading(pool, &gglib_core::ports::NoopGgufParser).await
}

/// Creates the complete database schema, reading a model file's header with
/// `gguf_parser` where a migration asks what the file is.
///
/// This function creates all tables and indexes required by the application.
/// It is safe to call multiple times as all operations use IF NOT EXISTS.
#[expect(
    clippy::too_many_lines,
    reason = "the schema's steps in the order they must run, most of them one CREATE each; the order is the point"
)]
async fn create_schema_reading(pool: &SqlitePool, gguf_parser: &dyn GgufParserPort) -> Result<()> {
    models::create_models_table(pool).await?;
    models::create_model_indexes(&mut *pool.acquire().await?).await?;

    // Step 1, the canonical paths, ahead of the rest: the projector link
    // below finds a model's folder from the `file_path` it rewrites.
    migrations::run(pool, 1).await?;

    model_files::create_model_files_table(pool).await?;
    model_files::add_projector_column(pool, gguf_parser).await?;
    model_components::create_model_components_table(pool).await?;

    // The settings table, from the one place that defines it. Here and not
    // after the schema, because the models rebuild below reads it.
    crate::SqliteSettingsRepository::new(pool.clone())
        .ensure_table()
        .await?;

    // A `chat_messages` table predating the 'tool' role cannot store a
    // tool-role message: its CHECK constraint rejects the insert. Startup
    // refuses such a database, naming the file, rather than dropping the
    // user's chat history; the user decides what happens to it. The CREATE
    // below carries the role (#362), so only a database older than that
    // reaches this branch.
    let chat_messages_sql: Option<String> = sqlx::query_scalar(
        "SELECT sql FROM sqlite_master WHERE type='table' AND name='chat_messages'",
    )
    .fetch_optional(pool)
    .await?;
    if chat_messages_sql.is_some_and(|sql| !sql.contains("'tool'")) {
        let file = pool.connect_options().get_filename().display().to_string();
        anyhow::bail!(
            "the chat_messages table in {file} predates the 'tool' role and cannot \
             store tool-role messages, so this build will not run against it.\n\
             Nothing has been changed. Either move that file aside, in which case \
             gglib creates a fresh database, or drop the two chat tables yourself \
             if the history is expendable:\n  \
             sqlite3 {file} 'DROP TABLE chat_messages; DROP TABLE chat_conversations;'"
        );
    }

    // Create chat conversations table
    sqlx::query(
        r"
        CREATE TABLE IF NOT EXISTS chat_conversations (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            title TEXT NOT NULL,
            model_id INTEGER,
            system_prompt TEXT,
            created_at TEXT NOT NULL DEFAULT (datetime('now')),
            updated_at TEXT NOT NULL DEFAULT (datetime('now')),
            FOREIGN KEY (model_id) REFERENCES models(id) ON DELETE SET NULL
        )
        ",
    )
    .execute(pool)
    .await?;

    // Create chat messages table
    sqlx::query(
        r"
        CREATE TABLE IF NOT EXISTS chat_messages (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            conversation_id INTEGER NOT NULL,
            role TEXT NOT NULL CHECK(role IN ('system', 'user', 'assistant', 'tool')),
            content TEXT NOT NULL,
            metadata TEXT,
            created_at TEXT NOT NULL DEFAULT (datetime('now')),
            FOREIGN KEY (conversation_id) REFERENCES chat_conversations(id) ON DELETE CASCADE
        )
        ",
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
    add_column_if_missing(pool, "chat_messages", "metadata", "TEXT").await?;

    // Migration: Add settings column for session parameter persistence.
    add_column_if_missing(pool, "chat_conversations", "settings", "TEXT").await?;

    attachments::create_attachment_tables(pool).await?;
    branches::add_branch_columns(pool).await?;

    mcp_servers::create_mcp_tables(pool).await?;

    // The download queue lives in memory; reclaim the table that shadowed it.
    // Nothing ever wrote a row, so there is nothing to migrate.
    sqlx::query("DROP TABLE IF EXISTS download_queue")
        .execute(pool)
        .await?;

    // Nothing reads the council/orchestrator tables; reclaim them.
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
    // This ALTER must run *after* the CREATE above: on a fresh database the
    // table does not exist before it, and the ALTER's error propagates, so a
    // misplaced ALTER fails the boot it is made on (#796).
    add_column_if_missing(pool, "benchmark_runs", "applied_json", "TEXT").await?;

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

    // The loop guard's log (#1052): a row per decision the guard took, and a
    // row per UTC day, model and gglib version counting the requests it
    // scanned. `gglib_db::LoopGuardTripWriter` writes both;
    // `SqliteLoopGuardTripLog` reads them back, a row for every day either
    // table knows.
    sqlx::query(loop_guard::TRIPS_TABLE).execute(pool).await?;
    sqlx::query(loop_guard::TRIPS_INDEX).execute(pool).await?;
    sqlx::query(loop_guard::SCANS_TABLE).execute(pool).await?;

    // After every table it reads exists, however old the library.
    models::rebuild_models_if_needed(pool).await?;

    // The other steps, after every table they read exists and after the
    // rebuild, which lets step 5 store a NULL count.
    migrations::run(pool, migrations::LATEST).await
}

/// Step 1: put local models' paths and keys onto the canonical rule.
///
/// A `local:` key is a hash of the model's canonical path. A row written
/// before [#850] may carry a key hashed over the path *as the caller spelled
/// it* (a relative path, a symlinked models directory, `/var` for
/// `/private/var`), which no current build recomputes: `ON CONFLICT(model_key)`
/// misses the row, `file_path` carries no unique index, and re-registering
/// that file appends a second one.
///
/// **The stored column cannot simply be trusted.** Such a row's `file_path`
/// may hold whatever spelling a `PATCH /api/models/{id}` sent, and `insert`'s
/// normalisation falls back to the literal path when the file is missing.
/// Re-keying from the column verbatim would compute a key from a
/// non-canonical string and report success. So each path is resolved here
/// first, the column rewritten when it moves, and the key derived from the
/// resolved form — the value `insert` computes for that file.
///
/// Idempotent: once a row is normalised the recomputed values equal the stored
/// ones and no write happens. A row whose new key would collide with another
/// row's is left alone rather than failing the unique index — that collision
/// means two rows genuinely name one file, which is a merge a startup
/// migration has no business performing silently. Any other database error is
/// propagated rather than counted as a skip.
///
/// Once per library, because it resolves every stored path: a blocking
/// syscall per row and shard, which on a dead mount stalls the launch. So a
/// non-canonical row an older build writes after the stamp is never repaired.
///
/// [#850]: https://github.com/mmogr/gglib/pull/850
async fn backfill_local_model_keys(conn: &mut SqliteConnection) -> Result<()> {
    use crate::repositories::sqlite_model_repository::local_model_key_for;
    use gglib_core::paths::canonical_model_path_string;
    use sqlx::Row;
    use std::path::Path;

    let rows =
        sqlx::query("SELECT id, file_path, model_key FROM models WHERE model_key LIKE 'local:%'")
            .fetch_all(&mut *conn)
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
            .execute(&mut *conn)
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

/// Step 1 too: canonicalise the stored shard path lists.
///
/// Companion to [`backfill_local_model_keys`], and needed for the same reason:
/// an older row's `file_paths_json` may hold each shard exactly as the
/// download handed it over — absolute, but never symlink-resolved — while the
/// duplicate lookup compares those entries against a resolved path.
///
/// Left alone, `gglib model add <shard-2>` against a sharded model already in
/// the library matches nothing, so the add proceeds and appends a second row
/// for a model that is already there. Unlike the key backfill this needs no
/// `--reimport` to reach: it is the plain add path.
///
/// Applies to every row with a shard list, not only `local:` ones — a
/// downloaded sharded model keeps its `hf:` key but may hold the same raw
/// paths. Idempotent: once resolved, re-resolving is the identity.
async fn backfill_shard_path_lists(conn: &mut SqliteConnection) -> Result<()> {
    use gglib_core::paths::canonical_model_path_string;
    use sqlx::Row;
    use std::path::Path;

    let rows =
        sqlx::query("SELECT id, file_paths_json FROM models WHERE file_paths_json IS NOT NULL")
            .fetch_all(&mut *conn)
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
            .execute(&mut *conn)
            .await?;
        rewritten += 1;
    }

    if rewritten > 0 {
        tracing::info!(rewritten, "canonicalised stored shard path lists");
    }
    Ok(())
}

#[cfg(test)]
#[path = "setup_tests.rs"]
mod tests;

#[cfg(test)]
#[path = "setup_backfill_tests.rs"]
mod backfill_tests;

#[cfg(test)]
#[path = "setup_models_tests.rs"]
mod models_tests;

#[cfg(test)]
#[path = "setup_models_pool_tests.rs"]
mod models_pool_tests;

#[cfg(test)]
#[path = "setup_models_quantization_tests.rs"]
mod models_quantization_tests;

#[cfg(test)]
#[path = "setup_fixture_tests.rs"]
mod fixture_tests;
