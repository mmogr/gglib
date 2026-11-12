//! A library released gglib 0.33.0 made, opened by this build.
//!
//! `tests/fixtures/library-0.33.0.sqlite`, whose README says what is in it
//! and which rows 0.33.0's own code wrote. While this build's generation is
//! 0.33.0's, the open is no upgrade, and these tests prove its rows survive
//! and read back. Once a build raises the generation, the same tests prove
//! the upgrade: that it backs the file up first, keeps every row readable, and
//! ends at the schema a new library gets.

use std::path::PathBuf;

use gglib_core::ports::NoopGgufParser;
use sqlx::{Row, SqlitePool};

use super::generation::{SCHEMA_GENERATION, stored};
use super::{backup_beside, setup_database, setup_test_database};

#[path = "setup_fixture_history_tests.rs"]
mod history;
#[path = "setup_fixture_library_tests.rs"]
mod library;

/// The schema generation gglib 0.33.0 stamps.
const FIXTURE_GENERATION: i64 = 5;

const FIXTURE: &str = concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/tests/fixtures/library-0.33.0.sqlite"
);

/// A copy of the fixture in a folder of its own, opened by this build.
async fn open_fixture() -> (tempfile::TempDir, PathBuf, SqlitePool) {
    let dir = tempfile::tempdir().expect("tempdir");
    let db = dir.path().join("gglib.db");
    std::fs::copy(FIXTURE, &db).expect("copy the fixture");
    let pool = setup_database(&db, &NoopGgufParser)
        .await
        .expect("this build opens a library 0.33.0 made");
    (dir, db, pool)
}

/// A time as the tables store one, `2026-11-12 01:37:58[.941536][ UTC]`.
fn at(text: &str) -> chrono::DateTime<chrono::Utc> {
    let text = text.trim_end_matches(" UTC");
    chrono::NaiveDateTime::parse_from_str(text, "%Y-%m-%d %H:%M:%S%.f")
        .unwrap()
        .and_utc()
}

#[tokio::test]
async fn the_library_is_backed_up_only_when_its_generation_is_raised() {
    let (_dir, db, pool) = open_fixture().await;

    assert_eq!(
        backup_beside(&db).map(|(generation, _)| generation),
        (FIXTURE_GENERATION < SCHEMA_GENERATION).then_some(FIXTURE_GENERATION),
    );
    assert_eq!(stored(&pool).await.unwrap(), SCHEMA_GENERATION);
    pool.close().await;
}

#[tokio::test]
async fn a_second_open_changes_nothing() {
    let (_dir, db, pool) = open_fixture().await;
    let first = contents(&pool).await;
    pool.close().await;

    let pool = setup_database(&db, &NoopGgufParser).await.unwrap();
    assert_eq!(contents(&pool).await, first);
    pool.close().await;
}

#[tokio::test]
async fn the_opened_library_has_the_schema_a_new_one_gets() {
    let (_dir, _db, pool) = open_fixture().await;
    let fresh = setup_test_database().await.unwrap();

    assert_eq!(shape(&pool).await, shape(&fresh).await);
    pool.close().await;
}

/// Every row of every table, as text, with `user_version`.
async fn contents(pool: &SqlitePool) -> Vec<String> {
    let mut out = vec![format!("user_version {}", user_version(pool).await)];
    for table in texts(
        pool,
        "SELECT name FROM sqlite_master WHERE type = 'table' ORDER BY name",
        "",
    )
    .await
    {
        let columns = texts(
            pool,
            "SELECT name FROM pragma_table_xinfo(?) ORDER BY cid",
            &table,
        )
        .await;
        let row = columns
            .iter()
            .map(|c| format!("quote(\"{c}\")"))
            .collect::<Vec<_>>()
            .join(" || '|' || ");
        let query = format!("SELECT {row} FROM \"{table}\" ORDER BY rowid");
        out.extend(
            texts(pool, &query, "")
                .await
                .into_iter()
                .map(|r| format!("{table}: {r}")),
        );
    }
    out
}

/// The schema, as `SQLite` reports it: each object by type and name, each
/// table's columns by name (not position, which an `ALTER TABLE ADD COLUMN`
/// sets), its indexes and their columns, and its foreign keys.
async fn shape(pool: &SqlitePool) -> Vec<String> {
    let mut out = vec![format!("user_version {}", user_version(pool).await)];
    out.extend(
        texts(
            pool,
            "SELECT type || ' ' || name FROM sqlite_master ORDER BY 1",
            "",
        )
        .await,
    );
    for table in texts(
        pool,
        "SELECT name FROM sqlite_master WHERE type = 'table' ORDER BY name",
        "",
    )
    .await
    {
        let columns = "SELECT name || ' ' || type || ' notnull=' || \"notnull\" || ' default=' \
             || quote(dflt_value) || ' pk=' || pk || ' hidden=' || hidden \
             FROM pragma_table_xinfo(?) ORDER BY name";
        let indexes = "SELECT name || ' unique=' || \"unique\" || ' origin=' || origin \
             || ' partial=' || partial FROM pragma_index_list(?) ORDER BY name";
        let keys = "SELECT \"table\" || '.' || quote(\"to\") || ' from ' || \"from\" || ' on update ' \
             || on_update || ' on delete ' || on_delete || ' match ' || \"match\" \
             FROM pragma_foreign_key_list(?) ORDER BY 1";
        for (kind, query) in [
            ("column", columns),
            ("index", indexes),
            ("references", keys),
        ] {
            out.extend(
                texts(pool, query, &table)
                    .await
                    .into_iter()
                    .map(|r| format!("{table} {kind} {r}")),
            );
        }
        for index in texts(
            pool,
            "SELECT name FROM pragma_index_list(?) ORDER BY name",
            &table,
        )
        .await
        {
            let columns = "SELECT seqno || ' ' || quote(name) || ' desc=' || \"desc\" || ' coll=' \
                 || quote(coll) || ' key=' || \"key\" FROM pragma_index_xinfo(?) ORDER BY seqno";
            out.extend(
                texts(pool, columns, &index)
                    .await
                    .into_iter()
                    .map(|r| format!("{index} {r}")),
            );
        }
    }
    out
}

async fn user_version(pool: &SqlitePool) -> i64 {
    sqlx::query_scalar("PRAGMA user_version")
        .fetch_one(pool)
        .await
        .unwrap()
}

/// The one text column `query` returns, its `?` bound to `arg` when it has one.
async fn texts(pool: &SqlitePool, query: &str, arg: &str) -> Vec<String> {
    let query = sqlx::query(query);
    let query = if arg.is_empty() {
        query
    } else {
        query.bind(arg)
    };
    query
        .fetch_all(pool)
        .await
        .unwrap()
        .iter()
        .map(|row| row.get::<String, _>(0))
        .collect()
}
