//! Tests for the `models` rebuild against database files and pools as the
//! daemon and the CLI hold them: two processes opening one old library at
//! once, a rebuild that fails, and the model repository on a rebuilt table.
//!
//! Split from `setup_models_tests.rs`, whose fixtures these share, to keep
//! each file under the 300 lines a new file may be.

use std::path::Path;
use std::time::Duration;

use gglib_core::{ModelRepository, NewModel};
use sqlx::sqlite::SqlitePoolOptions;

use super::models_tests::{
    LEGACY_CHILDREN, child_counts, legacy_ddl, legacy_library, models_sql, pool_of_one, scalar,
};
use super::*;
use crate::SqliteModelRepository;

/// A one-connection pool over the database file at `path`, opened the way
/// `setup_database` opens it, so two of them contend as two processes do.
async fn file_pool(path: &Path) -> SqlitePool {
    SqlitePoolOptions::new()
        .max_connections(1)
        .connect_with(
            SqliteConnectOptions::new()
                .filename(path)
                .create_if_missing(true)
                .journal_mode(SqliteJournalMode::Wal)
                .busy_timeout(Duration::from_secs(5)),
        )
        .await
        .unwrap()
}

/// How far one rebuild moves `PRAGMA schema_version`, which counts every
/// change to the schema. A race that rebuilt twice would move it twice as far.
async fn one_rebuild_moves_the_schema_version_by() -> i64 {
    let pool = pool_of_one().await;
    legacy_library(&pool).await;
    let before = scalar(&pool, "PRAGMA schema_version").await;
    create_schema(&pool).await.unwrap();
    scalar(&pool, "PRAGMA schema_version").await - before
}

/// Two processes opening one old library at once: both boot, the table is
/// rebuilt once, nothing is lost, and both are left with foreign keys on.
///
/// Whether the second reaches its `BEGIN IMMEDIATE` before the first commits
/// is up to the scheduler, so the race runs several times. Either way the
/// outcome must be the same one.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn two_processes_opening_an_old_library_at_once_rebuild_it_once() {
    let one_rebuild = one_rebuild_moves_the_schema_version_by().await;
    for round in 0..5 {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("gglib.db");
        let fixture = file_pool(&path).await;
        legacy_library(&fixture).await;
        let before = scalar(&fixture, "PRAGMA schema_version").await;
        fixture.close().await;

        let (first, second) = (file_pool(&path).await, file_pool(&path).await);
        let racing = [first.clone(), second.clone()].map(|pool| {
            tokio::spawn(async move { create_schema(&pool).await.map_err(|e| e.to_string()) })
        });
        for opened in racing {
            opened.await.unwrap().unwrap();
        }

        let moved = scalar(&first, "PRAGMA schema_version").await - before;
        assert_eq!(moved, one_rebuild, "round {round}: rebuilt once, not twice");
        assert!(models_sql(&first).await.contains("AUTOINCREMENT"));
        assert_eq!(child_counts(&first).await, LEGACY_CHILDREN, "round {round}");
        assert_eq!(scalar(&first, "SELECT COUNT(*) FROM models").await, 2);
        for pool in [&first, &second] {
            assert_eq!(
                scalar(pool, "PRAGMA foreign_keys").await,
                1,
                "round {round}"
            );
        }
    }
}

/// A rebuild that fails leaves the library as it found it and puts no
/// connection with foreign keys off back in the pool. A legacy row with no
/// name stands in for any failure: the new table refuses it.
#[tokio::test]
async fn a_rebuild_that_fails_changes_nothing_and_keeps_foreign_keys_on() {
    let dir = tempfile::tempdir().unwrap();
    let pool = file_pool(&dir.path().join("gglib.db")).await;
    sqlx::raw_sql(&format!(
        "{};
         INSERT INTO models (id, param_count_b, file_path, added_at, model_key)
             VALUES (4, 7.0, '/m/nameless.gguf', 'then', 'hf:nameless');
         PRAGMA user_version = {CANONICAL_PATH_SCHEMA_VERSION};",
        legacy_ddl("models").replace("name TEXT NOT NULL", "name TEXT"),
    ))
    .execute(&pool)
    .await
    .unwrap();

    let error = create_schema(&pool)
        .await
        .expect_err("a row the new table refuses must stop the boot");

    assert!(
        format!("{error:#}").contains("NOT NULL"),
        "the cause reaches the caller, got: {error:#}"
    );
    assert!(!models_sql(&pool).await.contains("AUTOINCREMENT"));
    assert_eq!(scalar(&pool, "SELECT id FROM models").await, 4);
    assert_eq!(
        scalar(
            &pool,
            "SELECT COUNT(*) FROM sqlite_master WHERE name = 'models_new'"
        )
        .await,
        0,
        "the half-built table is rolled back"
    );
    assert_eq!(scalar(&pool, "PRAGMA foreign_keys").await, 1);
}

fn model(key: &str) -> NewModel {
    NewModel::new(
        key.to_string(),
        format!("/m/{key}.gguf").into(),
        7.0,
        chrono::Utc::now(),
    )
}

/// Registering a model that is already there updates its row and takes no
/// id from the sequence, so the next new model gets the next id, not one
/// past every re-registration.
#[tokio::test]
async fn re_registering_a_model_takes_no_id() {
    let repo = SqliteModelRepository::new(setup_test_database().await.unwrap());

    let mut ids = Vec::new();
    for _ in 0..3 {
        ids.push(repo.insert(&model("same")).await.unwrap().id);
    }
    ids.push(repo.insert(&model("other")).await.unwrap().id);

    assert_eq!(ids, [1, 1, 1, 2]);
}

/// The second shard of a sharded model still finds its model once the table
/// is rebuilt, because `file_paths_json` came across with it.
#[tokio::test]
async fn a_shard_finds_its_model_in_a_rebuilt_table() {
    let pool = pool_of_one().await;
    legacy_library(&pool).await;
    create_schema(&pool).await.unwrap();
    let repo = SqliteModelRepository::new(pool);

    let found = repo
        .find_by_path(Path::new("/m/s-2.gguf"))
        .await
        .unwrap()
        .expect("shard 2 belongs to the sharded model");

    assert_eq!(found.id, 2);
}
