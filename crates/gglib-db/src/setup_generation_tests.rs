//! Tests for [`super`]: a newer database is refused before it is touched,
//! an older one is brought up and stamped, and the schema snapshot that
//! keeps [`SCHEMA_GENERATION`] honest.

use std::path::Path;

use sqlx::SqlitePool;

use super::SCHEMA_GENERATION;
use crate::setup::{setup_database, setup_test_database};

async fn generation(pool: &SqlitePool) -> i64 {
    sqlx::query_scalar("SELECT generation FROM schema_generation")
        .fetch_one(pool)
        .await
        .expect("a stamped generation")
}

/// A database file holding nothing but a stamp of `generation`.
async fn stamped_file(path: &Path, generation: i64) {
    let url = format!("sqlite://{}?mode=rwc", path.display());
    let pool = SqlitePool::connect(&url).await.expect("open");
    sqlx::query("CREATE TABLE schema_generation (generation INTEGER NOT NULL)")
        .execute(&pool)
        .await
        .expect("table");
    sqlx::query("INSERT INTO schema_generation (generation) VALUES (?)")
        .bind(generation)
        .execute(&pool)
        .await
        .expect("stamp");
    pool.close().await;
}

#[tokio::test]
async fn a_new_database_is_stamped_with_this_generation() {
    let dir = tempfile::tempdir().expect("tempdir");
    let pool = setup_database(&dir.path().join("gglib.db"))
        .await
        .expect("setup");
    assert_eq!(generation(&pool).await, SCHEMA_GENERATION);
}

#[tokio::test]
async fn a_database_from_a_newer_build_is_refused_and_left_as_it_was() {
    let dir = tempfile::tempdir().expect("tempdir");
    let path = dir.path().join("gglib.db");
    stamped_file(&path, SCHEMA_GENERATION + 1).await;

    let err = setup_database(&path)
        .await
        .expect_err("a newer file must be refused");
    assert!(err.to_string().contains("newer gglib"), "{err}");

    let pool = SqlitePool::connect(&format!("sqlite://{}", path.display()))
        .await
        .expect("reopen");
    let tables: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM sqlite_master WHERE type = 'table' AND name <> 'schema_generation'",
    )
    .fetch_one(&pool)
    .await
    .expect("count");
    assert_eq!(tables, 0, "nothing may be created in a refused file");
    assert_eq!(generation(&pool).await, SCHEMA_GENERATION + 1);
}

#[tokio::test]
async fn a_database_from_an_older_build_is_brought_up_and_restamped() {
    let dir = tempfile::tempdir().expect("tempdir");
    let path = dir.path().join("gglib.db");
    stamped_file(&path, SCHEMA_GENERATION - 1).await;

    let pool = setup_database(&path).await.expect("an older file opens");
    assert_eq!(generation(&pool).await, SCHEMA_GENERATION);
}

/// The schema a fresh database ends up with, as `schema_snapshot.sql` holds it.
async fn snapshot() -> String {
    let pool = setup_test_database().await.expect("setup");
    let user_version: i64 = sqlx::query_scalar("PRAGMA user_version")
        .fetch_one(&pool)
        .await
        .expect("user_version");
    let statements: Vec<String> = sqlx::query_scalar(
        "SELECT sql FROM sqlite_master
         WHERE sql IS NOT NULL AND name NOT LIKE 'sqlite_%'
         ORDER BY type, name",
    )
    .fetch_all(&pool)
    .await
    .expect("schema");
    let mut out = format!("-- user_version {user_version}\n\n");
    for sql in statements {
        out.push_str(sql.trim());
        out.push_str(";\n\n");
    }
    out
}

/// The generation a snapshot file was written at, from its first line.
fn snapshot_generation(file: &str) -> Option<i64> {
    file.lines()
        .next()?
        .strip_prefix("-- generation ")?
        .trim()
        .parse()
        .ok()
}

#[tokio::test]
async fn schema_snapshot() {
    const PATH: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/src/schema_snapshot.sql");
    let committed = std::fs::read_to_string(PATH).unwrap_or_default();
    let committed_generation = snapshot_generation(&committed);
    let committed_body = committed.split_once('\n').map_or("", |(_, body)| body);
    let current = snapshot().await;

    if std::env::var_os("GGLIB_UPDATE_SCHEMA_SNAPSHOT").is_some() {
        assert!(
            committed_body == current || committed_generation != Some(SCHEMA_GENERATION),
            "the schema changed but SCHEMA_GENERATION is still {SCHEMA_GENERATION}: \
             raise it in setup_generation.rs first"
        );
        std::fs::write(
            PATH,
            format!("-- generation {SCHEMA_GENERATION}\n{current}"),
        )
        .expect("write the snapshot");
        return;
    }

    assert_eq!(
        committed_generation,
        Some(SCHEMA_GENERATION),
        "schema_snapshot.sql was written at another generation. Rewrite it with \
         GGLIB_UPDATE_SCHEMA_SNAPSHOT=1 cargo test -p gglib-db schema_snapshot"
    );
    assert!(
        committed_body == current,
        "the database schema changed. Raise SCHEMA_GENERATION in setup_generation.rs, then \
         rewrite the snapshot with GGLIB_UPDATE_SCHEMA_SNAPSHOT=1 cargo test -p gglib-db \
         schema_snapshot"
    );
}
