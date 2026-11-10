//! The step that makes stored `0.0` counts unknown: which rows it changes,
//! that it runs once, and that a table which required a count is rebuilt
//! so it can hold none.

use sqlx::sqlite::SqlitePoolOptions;

use super::super::create_schema;
use super::*;

/// One connection, so every statement and pragma reads the one database.
async fn library() -> SqlitePool {
    let pool = SqlitePoolOptions::new()
        .max_connections(1)
        .connect("sqlite::memory:")
        .await
        .unwrap();
    create_schema(&pool).await.unwrap();
    pool
}

/// Add a model named `name` whose stored count is `count`.
async fn plant(pool: &SqlitePool, name: &str, count: f64) {
    sqlx::query(
        "INSERT INTO models (name, file_path, param_count_b, added_at, model_key) \
         VALUES (?, ?, ?, 'then', ?)",
    )
    .bind(name)
    .bind(format!("/models/{name}.gguf"))
    .bind(count)
    .bind(format!("local:{name}"))
    .execute(pool)
    .await
    .unwrap();
}

/// The count stored for `name`.
async fn count_of(pool: &SqlitePool, name: &str) -> Option<f64> {
    sqlx::query_scalar("SELECT param_count_b FROM models WHERE name = ?")
        .bind(name)
        .fetch_one(pool)
        .await
        .unwrap()
}

/// Stamp the library as a build at the remote grant's version left it.
async fn stamp_older(pool: &SqlitePool) {
    sqlx::query("PRAGMA user_version = 4")
        .execute(pool)
        .await
        .unwrap();
}

#[tokio::test]
async fn a_count_of_none_becomes_unknown_and_a_real_one_is_kept() {
    let pool = library().await;
    plant(&pool, "drawn", 0.0).await;
    plant(&pool, "chat", 7.0).await;
    stamp_older(&pool).await;

    forget_zero_param_counts(&pool).await.unwrap();

    assert_eq!(count_of(&pool, "drawn").await, None);
    assert_eq!(count_of(&pool, "chat").await, Some(7.0));
    assert_eq!(
        user_version(&pool).await.unwrap(),
        UNKNOWN_PARAMS_SCHEMA_VERSION
    );
}

/// Once stamped, a later `0.0` (an owner's, written by hand) is left alone.
#[tokio::test]
async fn the_step_runs_once() {
    let pool = library().await;
    plant(&pool, "typed", 0.0).await;

    forget_zero_param_counts(&pool).await.unwrap();

    assert_eq!(count_of(&pool, "typed").await, Some(0.0));
}

/// Turn today's `models` back into the shape a build before the count could
/// be NULL made: the same table, `AUTOINCREMENT` and all, whose
/// `param_count_b` must be set. Its rows and its sequence are kept, as such
/// a library has them.
async fn require_a_count(pool: &SqlitePool) {
    let today: String = sqlx::query_scalar(
        "SELECT sql FROM sqlite_master WHERE type = 'table' AND name = 'models'",
    )
    .fetch_one(pool)
    .await
    .unwrap();
    let required = today
        .replacen("param_count_b REAL,", "param_count_b REAL NOT NULL,", 1)
        .replacen("TABLE models", "TABLE required", 1);
    assert_ne!(
        required, today,
        "the column is spelled as the DDL spells it"
    );
    let seq: i64 = sqlx::query_scalar(
        "SELECT COALESCE(MAX(seq), 0) FROM sqlite_sequence WHERE name = 'models'",
    )
    .fetch_one(pool)
    .await
    .unwrap();
    sqlx::raw_sql(&format!(
        "PRAGMA foreign_keys = OFF;
         {required};
         INSERT INTO required SELECT * FROM models;
         DROP TABLE models;
         ALTER TABLE required RENAME TO models;
         DELETE FROM sqlite_sequence WHERE name = 'models';
         INSERT INTO sqlite_sequence (name, seq) VALUES ('models', {seq});
         PRAGMA foreign_keys = ON;"
    ))
    .execute(pool)
    .await
    .unwrap();
}

/// A library from before the count could be NULL: its `models` requires
/// one. Opening it rebuilds the table to allow none, then forgets its
/// `0.0`s.
#[tokio::test]
async fn a_table_that_required_a_count_is_rebuilt_to_hold_none() {
    let pool = library().await;
    plant(&pool, "drawn", 0.0).await;
    require_a_count(&pool).await;
    stamp_older(&pool).await;

    create_schema(&pool).await.unwrap();

    let still_required: bool = sqlx::query_scalar(
        "SELECT \"notnull\" FROM pragma_table_info('models') WHERE name = 'param_count_b'",
    )
    .fetch_one(&pool)
    .await
    .unwrap();
    assert!(!still_required);
    assert_eq!(count_of(&pool, "drawn").await, None);
}

/// The rebuild keeps the sequence the table had: a model removed before it,
/// whose id nothing references any more, never has its id given again.
#[tokio::test]
async fn the_rebuild_never_gives_a_removed_models_id_again() {
    let pool = library().await;
    for name in ["one", "two", "three"] {
        plant(&pool, name, 7.0).await;
    }
    sqlx::query("DELETE FROM models WHERE name = 'three'")
        .execute(&pool)
        .await
        .unwrap();
    require_a_count(&pool).await;
    stamp_older(&pool).await;

    create_schema(&pool).await.unwrap();
    plant(&pool, "four", 7.0).await;

    let id: i64 = sqlx::query_scalar("SELECT id FROM models WHERE name = 'four'")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(id, 4, "id 3 was model three's");
}

/// A saved agentic report's count of none is made unknown with the rows'.
#[tokio::test]
async fn a_saved_reports_count_of_none_becomes_unknown() {
    let pool = library().await;
    plant(&pool, "chat", 7.0).await;
    sqlx::query(
        "INSERT INTO benchmark_agentic_results \
         (model_id, raw_composite, gglib_composite, report_json, created_at) \
         VALUES (1, 1, 1, '{\"model_name\":\"chat\",\"param_count_b\":0.0}', 'then'), \
                (1, 1, 1, '{\"model_name\":\"chat\",\"param_count_b\":7.0}', 'then')",
    )
    .execute(&pool)
    .await
    .unwrap();
    stamp_older(&pool).await;

    forget_zero_param_counts(&pool).await.unwrap();

    let counts: Vec<Option<f64>> = sqlx::query_scalar(
        "SELECT json_extract(report_json, '$.param_count_b') FROM benchmark_agentic_results \
         ORDER BY rowid",
    )
    .fetch_all(&pool)
    .await
    .unwrap();
    assert_eq!(counts, [None, Some(7.0)]);
}
