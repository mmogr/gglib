//! Tests for the `models` rebuild: a library whose `models` table a build
//! from before `AUTOINCREMENT` made is rebuilt once, losing no row, id or
//! reference, and never reuses an id afterwards.
//!
//! Declared from `setup.rs` beside its other `#[path]` test files, so these
//! reach `create_schema` the way those do. Two processes racing, a rebuild
//! that fails, and the repository on a rebuilt table are in
//! `setup_models_pool_tests.rs`.

use sqlx::sqlite::SqlitePoolOptions;

use super::*;

/// One connection, so every statement runs on the connection the rebuild
/// used, and a pragma read afterwards reads that connection's (#1064).
pub(super) async fn pool_of_one() -> SqlitePool {
    SqlitePoolOptions::new()
        .max_connections(1)
        .connect("sqlite::memory:")
        .await
        .unwrap()
}

/// `models` as a build from before `AUTOINCREMENT` made it, named `table`.
pub(super) fn legacy_ddl(table: &str) -> String {
    format!(
        "CREATE TABLE {table} (
            id INTEGER PRIMARY KEY, name TEXT NOT NULL, file_path TEXT NOT NULL,
            param_count_b REAL NOT NULL, architecture TEXT, quantization TEXT,
            context_length INTEGER, inference_defaults TEXT, defaults_origin TEXT,
            server_defaults TEXT, expert_count INTEGER, expert_used_count INTEGER,
            expert_shared_count INTEGER, metadata TEXT, added_at TEXT NOT NULL,
            hf_repo_id TEXT, hf_commit_sha TEXT, hf_filename TEXT, download_date TEXT,
            last_update_check TEXT, tags TEXT DEFAULT '[]', model_key TEXT NOT NULL,
            file_paths_json TEXT, capabilities INTEGER DEFAULT 0, dialect_spec TEXT,
            template_caps TEXT
        )"
    )
}

/// Turns today's `models` back into the legacy table, running `extra` while
/// foreign keys are off. Every other table keeps the definition it has today,
/// which is the one every older build made too.
pub(super) async fn make_legacy(pool: &SqlitePool, extra: &str) {
    sqlx::raw_sql(&format!(
        "PRAGMA foreign_keys = OFF;
         ALTER TABLE models DROP COLUMN projector_path;
         {legacy};
         INSERT INTO legacy SELECT * FROM models;
         DROP TABLE models;
         ALTER TABLE legacy RENAME TO models;
         DELETE FROM sqlite_sequence WHERE name = 'models';
         {extra};
         PRAGMA foreign_keys = ON;",
        legacy = legacy_ddl("legacy"),
    ))
    .execute(pool)
    .await
    .unwrap();
    assert!(!models_sql(pool).await.contains("AUTOINCREMENT"));
}

/// A legacy library with a row in each of the seven tables that reference
/// `models`, a sharded model, a benchmark run naming a removed model (9), a
/// default model (7) and a perf result that already referenced no model (12).
pub(super) async fn legacy_library(pool: &SqlitePool) {
    create_schema(pool).await.unwrap();
    sqlx::raw_sql(
        r#"INSERT INTO models (id, name, file_path, param_count_b, added_at, model_key, file_paths_json)
             VALUES (1, 'Plain', '/m/plain.gguf', 7.0, 'then', 'hf:plain', NULL),
                    (2, 'Sharded', '/m/s-1.gguf', 7.0, 'then', 'hf:sharded',
                     '["/m/s-1.gguf","/m/s-2.gguf"]');
         INSERT INTO model_files (model_id, file_path, file_index, expected_size)
             VALUES (2, '/m/s-2.gguf', 1, 10);
         INSERT INTO chat_conversations (title, model_id) VALUES ('Kept', 2);
         INSERT INTO benchmark_runs (id, run_type, status, model_ids, created_at)
             VALUES (1, 'compare', 'completed', '[1, 9]', 'then');
         INSERT INTO model_compare_results (model_id, run_id, prompt_text, response_text, created_at)
             VALUES (1, 1, 'p', 'r', 'then');
         INSERT INTO model_perf_results (model_id, pp_tps, tg_tps, pp_tokens, tg_tokens, repetitions, created_at)
             VALUES (1, 1, 1, 1, 1, 1, 'then');
         INSERT INTO model_benchmark_summaries (model_id, last_benchmarked_at, updated_at)
             VALUES (1, 'then', 'then');
         INSERT INTO benchmark_tune_results (model_id, config_json, source_json, composite_score, task_results_json, created_at)
             VALUES (2, '{}', '{}', 1, '[]', 'then');
         INSERT INTO benchmark_agentic_results (model_id, raw_composite, gglib_composite, report_json, created_at)
             VALUES (2, 1, 1, '{}', 'then');
         INSERT INTO settings_kv (key, value, updated_at) VALUES ('default_model_id', '7', 'then');"#,
    )
    .execute(pool)
    .await
    .unwrap();
    make_legacy(
        pool,
        "INSERT INTO model_perf_results (model_id, pp_tps, tg_tps, pp_tokens, tg_tokens, repetitions, created_at)
             VALUES (12, 1, 1, 1, 1, 1, 'then')",
    )
    .await;
}

/// The `CREATE TABLE` statement `models` is stored under.
pub(super) async fn models_sql(pool: &SqlitePool) -> String {
    sqlx::query_scalar("SELECT sql FROM sqlite_master WHERE type = 'table' AND name = 'models'")
        .fetch_one(pool)
        .await
        .unwrap()
}

/// The one integer `sql` reads.
pub(super) async fn scalar(pool: &SqlitePool, sql: &str) -> i64 {
    sqlx::query_scalar(sql).fetch_one(pool).await.unwrap()
}

/// Adds a model under `key` and returns the id it was given.
async fn add_model(pool: &SqlitePool, key: &str) -> i64 {
    sqlx::query_scalar(
        "INSERT INTO models (name, file_path, param_count_b, added_at, model_key) \
         VALUES (?1, ?1, 7.0, 'now', ?1) RETURNING id",
    )
    .bind(key)
    .fetch_one(pool)
    .await
    .unwrap()
}

/// The row counts of the seven tables that reference `models`.
pub(super) async fn child_counts(pool: &SqlitePool) -> (i64, i64, i64, i64, i64, i64, i64) {
    sqlx::query_as(
        "SELECT (SELECT COUNT(*) FROM model_files), (SELECT COUNT(*) FROM chat_conversations),
            (SELECT COUNT(*) FROM model_compare_results), (SELECT COUNT(*) FROM model_perf_results),
            (SELECT COUNT(*) FROM model_benchmark_summaries),
            (SELECT COUNT(*) FROM benchmark_tune_results),
            (SELECT COUNT(*) FROM benchmark_agentic_results)",
    )
    .fetch_one(pool)
    .await
    .unwrap()
}

/// What `legacy_library` leaves in each table `child_counts` reads; two perf
/// results, counting the orphan.
pub(super) const LEGACY_CHILDREN: (i64, i64, i64, i64, i64, i64, i64) = (1, 1, 1, 2, 1, 1, 1);

#[tokio::test]
async fn an_old_library_is_rebuilt_keeping_every_row_and_reference() {
    let pool = pool_of_one().await;
    legacy_library(&pool).await;

    create_schema(&pool).await.unwrap();

    assert!(models_sql(&pool).await.contains("AUTOINCREMENT"));
    assert_eq!(
        child_counts(&pool).await,
        LEGACY_CHILDREN,
        "every child row survives"
    );
    let shards: (i64, String) =
        sqlx::query_as("SELECT id, file_paths_json FROM models WHERE name = 'Sharded'")
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(shards, (2, r#"["/m/s-1.gguf","/m/s-2.gguf"]"#.to_string()));
    assert_eq!(
        scalar(&pool, "SELECT id FROM models WHERE name = 'Plain'").await,
        1
    );
    let chat_model: Option<i64> = sqlx::query_scalar("SELECT model_id FROM chat_conversations")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(chat_model, Some(2), "a conversation keeps its model");
    assert_eq!(scalar(&pool, "PRAGMA foreign_keys").await, 1);
    assert_eq!(
        scalar(&pool, "SELECT COUNT(*) FROM pragma_foreign_key_check").await,
        1,
        "the orphan the library already had is left as it was, and no other"
    );
    let indexes = scalar(&pool, "SELECT COUNT(*) FROM pragma_index_list('models')").await;
    assert_eq!(indexes, 3);

    sqlx::query("DELETE FROM models WHERE id = 2")
        .execute(&pool)
        .await
        .unwrap();
    assert_eq!(
        add_model(&pool, "hf:next").await,
        13,
        "above the removed 2, the default 7, the benchmark run's 9 and the orphan's 12"
    );
}

/// Each place a removed model's id can live on raises the sequence on its
/// own, and a benchmark run whose list is not JSON adds nothing.
#[tokio::test]
async fn every_id_held_elsewhere_is_above_the_next_one() {
    let cases = [
        ("SELECT 1", 2),
        (
            "INSERT INTO chat_conversations (title, model_id) VALUES ('t', 4)",
            5,
        ),
        (
            "INSERT INTO benchmark_runs (run_type, status, model_ids, created_at)
                VALUES ('perf', 'completed', '[1, 6]', 'then'),
                       ('perf', 'completed', 'not json', 'then')",
            7,
        ),
        (
            "INSERT INTO settings_kv (key, value, updated_at) VALUES ('default_model_id', '8', 'then')",
            9,
        ),
        (
            "INSERT INTO model_files (model_id, file_path, file_index, expected_size) VALUES (10, '/m/x', 0, 1)",
            11,
        ),
    ];
    for (held, next) in cases {
        let pool = pool_of_one().await;
        create_schema(&pool).await.unwrap();
        add_model(&pool, "hf:one").await;
        make_legacy(&pool, held).await;

        create_schema(&pool).await.unwrap();

        assert_eq!(add_model(&pool, "hf:new").await, next, "after: {held}");
    }
}

/// Once rebuilt, opening the library again touches neither the schema nor
/// the sequence.
#[tokio::test]
async fn a_second_open_does_not_rebuild_again() {
    let pool = pool_of_one().await;
    legacy_library(&pool).await;
    create_schema(&pool).await.unwrap();
    sqlx::query("UPDATE sqlite_sequence SET seq = 50 WHERE name = 'models'")
        .execute(&pool)
        .await
        .unwrap();
    let schema_version = scalar(&pool, "PRAGMA schema_version").await;

    create_schema(&pool).await.unwrap();

    assert_eq!(scalar(&pool, "PRAGMA schema_version").await, schema_version);
    assert_eq!(add_model(&pool, "hf:next").await, 51);
}

/// A fresh library is made with `AUTOINCREMENT` and never rebuilt: a
/// rebuild would reseed the sequence from the ids present, and give a
/// removed model's id out again.
#[tokio::test]
async fn a_fresh_library_is_not_rebuilt() {
    let pool = pool_of_one().await;
    create_schema(&pool).await.unwrap();
    assert!(models_sql(&pool).await.contains("AUTOINCREMENT"));
    add_model(&pool, "hf:gone").await;
    sqlx::query("DELETE FROM models")
        .execute(&pool)
        .await
        .unwrap();

    create_schema(&pool).await.unwrap();

    assert_eq!(add_model(&pool, "hf:next").await, 2);
}

/// A library from before the chat, settings and benchmark tables is rebuilt
/// too: the rebuild reads them, so it runs after `create_schema` makes them.
#[tokio::test]
async fn a_library_older_than_the_later_tables_is_rebuilt() {
    let pool = pool_of_one().await;
    sqlx::raw_sql(&format!(
        r#"{};
         INSERT INTO models (id, name, file_path, param_count_b, added_at, model_key, file_paths_json)
             VALUES (3, 'Old', '/m/o-1.gguf', 7.0, 'then', 'hf:old', '["/m/o-1.gguf","/m/o-2.gguf"]');
         PRAGMA user_version = {CANONICAL_PATH_SCHEMA_VERSION};"#,
        legacy_ddl("models"),
    ))
    .execute(&pool)
    .await
    .unwrap();

    create_schema(&pool).await.unwrap();

    assert!(models_sql(&pool).await.contains("AUTOINCREMENT"));
    let old: (i64, String) = sqlx::query_as("SELECT id, file_paths_json FROM models")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(old, (3, r#"["/m/o-1.gguf","/m/o-2.gguf"]"#.to_string()));
    assert_eq!(
        scalar(&pool, "SELECT COUNT(*) FROM benchmark_runs").await,
        0
    );
}
