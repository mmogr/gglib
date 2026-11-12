//! The runner on database files of the test's own: every step in order, each
//! once, under the lock, and a failed one stamped nowhere. What each step does
//! to its rows is tested beside it.

use std::path::Path;
use std::time::Duration;

use gglib_core::ports::NoopGgufParser;
use sqlx::SqlitePool;
use sqlx::sqlite::SqliteConnectOptions;

use super::*;
use crate::repositories::sqlite_model_repository::local_model_key_for;
use crate::setup_database;

/// A file with this build's schema, stamped `version`.
async fn stamped(path: &Path, version: i64) -> SqlitePool {
    let pool = setup_database(path, &NoopGgufParser)
        .await
        .expect("a fresh database");
    sql(&pool, &format!("PRAGMA user_version = {version}")).await;
    pool
}

async fn sql(pool: &SqlitePool, statement: &str) {
    sqlx::raw_sql(statement)
        .execute(pool)
        .await
        .expect("a statement");
}

async fn version(pool: &SqlitePool) -> i64 {
    user_version(pool).await.expect("a version")
}

/// A model `name` stored under `path` and `key`, counted `count`, holding
/// defaults labelled `auto_detected`, which step 3 clears.
async fn plant_model(pool: &SqlitePool, name: &str, path: &str, key: &str, count: f64) {
    sqlx::query(
        "INSERT INTO models (name, file_path, param_count_b, added_at, model_key, \
         inference_defaults, defaults_origin) VALUES (?, ?, ?, 'then', ?, '{}', 'auto_detected')",
    )
    .bind(name)
    .bind(path)
    .bind(count)
    .bind(key)
    .execute(pool)
    .await
    .expect("a planted model");
}

/// Rows for steps 2 and 4: a loop guard switched off, and a remote tunnel
/// whose list names a server.
async fn plant_rows(pool: &SqlitePool) {
    sql(
        pool,
        r#"INSERT INTO settings_kv (key, value, updated_at) VALUES
             ('proxy_loop_detection', 'false', 'then'),
             ('remote_enabled', 'true', 'then'),
             ('remote_serve', '{"mcpServers":["tavily"]}', 'then');
           INSERT INTO mcp_servers (name, type) VALUES ('tavily', 'stdio');"#,
    )
    .await;
}

/// Every row a step writes, as text.
async fn everything(pool: &SqlitePool) -> Vec<String> {
    sqlx::query_scalar(
        "SELECT json_object('path', file_path, 'key', model_key, 'count', param_count_b, \
                'defaults', inference_defaults, 'origin', defaults_origin) FROM models \
         UNION ALL SELECT json_object('key', key, 'value', value, 'at', updated_at) FROM settings_kv \
         UNION ALL SELECT json_object('server', name, 'remote', allow_remote) FROM mcp_servers",
    )
    .fetch_all(pool)
    .await
    .expect("the rows")
}

/// A file `name` in `dir`, by a spelling that is not its canonical one, and
/// by its canonical one.
fn spelled_and_canonical(dir: &Path, name: &str) -> (String, String) {
    std::fs::write(dir.join(name), b"weights").expect("a file");
    std::fs::create_dir_all(dir.join("sub")).expect("a dir");
    let spelled = dir.join("sub").join("..").join(name);
    let canonical = std::fs::canonicalize(&spelled).expect("resolved");
    (
        spelled.to_string_lossy().into_owned(),
        canonical.to_string_lossy().into_owned(),
    )
}

#[tokio::test]
async fn a_file_never_stamped_takes_every_step_in_one_run_and_a_second_run_writes_nothing() {
    let dir = tempfile::tempdir().expect("tempdir");
    let path = dir.path().join("gglib.db");
    let pool = stamped(&path, 0).await;
    let (spelled, canonical) = spelled_and_canonical(dir.path(), "m.gguf");
    plant_model(&pool, "m", &spelled, &local_model_key_for(&spelled), 0.0).await;
    plant_rows(&pool).await;

    run(&pool, LATEST).await.expect("the steps");

    assert_eq!(version(&pool).await, LATEST);
    let model: (String, String, Option<f64>, Option<String>) = sqlx::query_as(
        "SELECT file_path, model_key, param_count_b, inference_defaults FROM models",
    )
    .fetch_one(&pool)
    .await
    .expect("the model");
    let key = local_model_key_for(&canonical);
    assert_eq!(model, (canonical, key, None, None), "steps 1, 3 and 5");
    let settings: Vec<(String, String)> =
        sqlx::query_as("SELECT key, value FROM settings_kv WHERE key LIKE '%loop%'")
            .fetch_all(&pool)
            .await
            .expect("the settings");
    let off = ("loop_guard_mode".to_owned(), r#""off""#.to_owned());
    assert_eq!(settings, [off], "step 2");
    let flagged: i64 = sqlx::query_scalar("SELECT allow_remote FROM mcp_servers")
        .fetch_one(&pool)
        .await
        .expect("the server");
    assert_eq!(flagged, 1, "step 4");

    // A file at `LATEST` takes no write lock: another connection holds it.
    let after = everything(&pool).await;
    let other = SqlitePool::connect_with(SqliteConnectOptions::new().filename(&path))
        .await
        .expect("a second pool");
    let mut held = other.acquire().await.expect("a connection");
    sqlx::query("BEGIN IMMEDIATE")
        .execute(&mut *held)
        .await
        .expect("the lock");
    tokio::time::timeout(Duration::from_secs(1), run(&pool, LATEST))
        .await
        .expect("no wait for the lock")
        .expect("the second run");
    sqlx::query("ROLLBACK")
        .execute(&mut *held)
        .await
        .expect("released");

    assert_eq!(everything(&pool).await, after);
}

#[tokio::test]
async fn a_later_stamp_is_not_lowered() {
    let dir = tempfile::tempdir().expect("tempdir");
    let pool = stamped(&dir.path().join("gglib.db"), LATEST + 1).await;

    run(&pool, LATEST).await.expect("the run");

    assert_eq!(version(&pool).await, LATEST + 1);
}

/// Another process may stamp the file between the read and the lock: the
/// step is not run again, and the stamp is not lowered.
#[tokio::test]
async fn the_version_is_asked_again_under_the_lock() {
    let dir = tempfile::tempdir().expect("tempdir");
    let pool = stamped(&dir.path().join("gglib.db"), LATEST + 1).await;
    plant_rows(&pool).await;
    let before = everything(&pool).await;

    run_one(&pool, 2).await.expect("the step");

    assert_eq!(everything(&pool).await, before);
    assert_eq!(version(&pool).await, LATEST + 1);
}

/// Step 4 is kept and stamped; step 5 fails on a table it cannot find, and
/// its first `UPDATE` goes with its transaction.
#[tokio::test]
async fn a_failed_step_leaves_the_stamp_of_the_last_good_one() {
    let dir = tempfile::tempdir().expect("tempdir");
    let pool = stamped(&dir.path().join("gglib.db"), 3).await;
    plant_model(&pool, "m", "/m/m.gguf", "hf:m", 0.0).await;
    plant_rows(&pool).await;
    sql(&pool, "DROP TABLE benchmark_agentic_results").await;

    let error = run(&pool, LATEST).await.expect_err("step 5 fails");

    assert!(
        format!("{error:#}").contains("benchmark_agentic_results"),
        "{error:#}"
    );
    assert_eq!(version(&pool).await, 4);
    let flagged: i64 = sqlx::query_scalar("SELECT allow_remote FROM mcp_servers")
        .fetch_one(&pool)
        .await
        .expect("the server");
    assert_eq!(flagged, 1);
    let count: Option<f64> = sqlx::query_scalar("SELECT param_count_b FROM models")
        .fetch_one(&pool)
        .await
        .expect("the model");
    assert_eq!(count, Some(0.0));
}

/// A key that would collide fails one `UPDATE` inside step 1's transaction.
/// That statement alone is undone: the row after it is still re-keyed, and
/// the step is stamped.
#[tokio::test]
async fn a_colliding_key_is_skipped_and_the_rest_of_step_1_commits() {
    let dir = tempfile::tempdir().expect("tempdir");
    let pool = stamped(&dir.path().join("gglib.db"), 0).await;
    let (shared, canonical) = spelled_and_canonical(dir.path(), "shared.gguf");
    let (later, later_canonical) = spelled_and_canonical(dir.path(), "later.gguf");
    let stale = local_model_key_for(&shared);
    let kept = local_model_key_for(&canonical);
    plant_model(&pool, "stale", &shared, &stale, 7.0).await;
    plant_model(&pool, "kept", &canonical, &kept, 7.0).await;
    plant_model(&pool, "later", &later, &local_model_key_for(&later), 7.0).await;

    run(&pool, 1).await.expect("step 1");

    assert_eq!(version(&pool).await, 1);
    let rows: Vec<(String, String)> =
        sqlx::query_as("SELECT file_path, model_key FROM models ORDER BY id")
            .fetch_all(&pool)
            .await
            .expect("the rows");
    let rekeyed = local_model_key_for(&later_canonical);
    assert_eq!(
        rows,
        [
            (shared, stale),
            (canonical, kept),
            (later_canonical, rekeyed)
        ]
    );
}
