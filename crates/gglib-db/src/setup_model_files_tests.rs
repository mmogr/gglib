//! Tests for adding `models.projector_path` to a library from before it, and
//! the link it makes from a model to the projector among its own files.

use std::path::Path;

use sqlx::sqlite::SqlitePoolOptions;

use super::super::create_schema;
use super::*;

/// One connection, so the in-memory database every statement sees is one.
async fn pool() -> SqlitePool {
    SqlitePoolOptions::new()
        .max_connections(1)
        .connect("sqlite::memory:")
        .await
        .unwrap()
}

/// A library as a build from before the column left it: today's schema with
/// `projector_path` taken away.
async fn old_library() -> SqlitePool {
    let pool = pool().await;
    create_schema(&pool).await.unwrap();
    sqlx::query("ALTER TABLE models DROP COLUMN projector_path")
        .execute(&pool)
        .await
        .unwrap();
    pool
}

/// Model `id`, whose weights are `dir/weights`, with `listed` as its
/// `file_paths_json` and `rows` as its `model_files` names.
async fn add_model(
    pool: &SqlitePool,
    id: i64,
    dir: &Path,
    weights: &str,
    listed: Option<&str>,
    rows: &[&str],
) {
    sqlx::query(
        "INSERT INTO models (id, name, file_path, param_count_b, added_at, model_key, file_paths_json) \
         VALUES (?1, ?2, ?3, 27.0, 'then', ?2, ?4)",
    )
    .bind(id)
    .bind(format!("model-{id}"))
    .bind(dir.join(weights).to_string_lossy().into_owned())
    .bind(listed)
    .execute(pool)
    .await
    .unwrap();
    for (index, name) in rows.iter().enumerate() {
        sqlx::query(
            "INSERT INTO model_files (model_id, file_path, file_index, expected_size) \
             VALUES (?, ?, ?, 10)",
        )
        .bind(id)
        .bind(name)
        .bind(i64::try_from(index).unwrap())
        .execute(pool)
        .await
        .unwrap();
    }
}

fn touch(dir: &Path, name: &str) -> String {
    let path = dir.join(name);
    std::fs::write(&path, b"x").unwrap();
    std::fs::canonicalize(path)
        .unwrap()
        .to_string_lossy()
        .into_owned()
}

async fn projector_of(pool: &SqlitePool, id: i64) -> Option<String> {
    sqlx::query_scalar("SELECT projector_path FROM models WHERE id = ?")
        .bind(id)
        .fetch_one(pool)
        .await
        .unwrap()
}

/// Every row of `models` without its link, and every row of `model_files`,
/// as text: what the link must leave exactly as it found it.
async fn everything_else(pool: &SqlitePool) -> Vec<String> {
    let mut rows: Vec<String> = sqlx::query_scalar(
        "SELECT json_object('id', id, 'name', name, 'file_path', file_path, 'key', model_key, \
                'files', file_paths_json, 'tags', tags, 'added', added_at) FROM models ORDER BY id",
    )
    .fetch_all(pool)
    .await
    .unwrap();
    let files: Vec<String> = sqlx::query_scalar(
        "SELECT json_object('id', id, 'model', model_id, 'path', file_path, 'index', file_index, \
                'size', expected_size) FROM model_files ORDER BY id",
    )
    .fetch_all(pool)
    .await
    .unwrap();
    rows.extend(files);
    rows
}

/// The shape the live library holds: the weights at index 0, the projector
/// stored as index 1, and `file_path` already on the weights.
#[tokio::test]
async fn a_model_with_a_projector_among_its_files_is_linked_to_it() {
    let dir = tempfile::tempdir().unwrap();
    touch(dir.path(), "X.Q8_0.gguf");
    let projector = touch(dir.path(), "X.mmproj-Q8_0.gguf");
    let pool = old_library().await;
    add_model(
        &pool,
        3,
        dir.path(),
        "X.Q8_0.gguf",
        None,
        &["X.Q8_0.gguf", "X.mmproj-Q8_0.gguf"],
    )
    .await;
    let before = everything_else(&pool).await;

    create_schema(&pool).await.unwrap();

    assert_eq!(projector_of(&pool, 3).await, Some(projector));
    assert_eq!(before.len(), 3, "one model and its two files");
    assert_eq!(
        everything_else(&pool).await,
        before,
        "no row deleted and nothing else rewritten"
    );
}

/// A second start finds the column there and writes nothing: the link stays
/// as made, and one the user has since removed stays removed.
#[tokio::test]
async fn a_second_run_changes_nothing() {
    let dir = tempfile::tempdir().unwrap();
    touch(dir.path(), "X.Q8_0.gguf");
    let projector = touch(dir.path(), "X.mmproj-Q8_0.gguf");
    let pool = old_library().await;
    let files = ["X.Q8_0.gguf", "X.mmproj-Q8_0.gguf"];
    add_model(&pool, 3, dir.path(), "X.Q8_0.gguf", None, &files).await;
    create_schema(&pool).await.unwrap();
    let linked = everything_else(&pool).await;

    create_schema(&pool).await.unwrap();
    assert_eq!(projector_of(&pool, 3).await, Some(projector));
    assert_eq!(everything_else(&pool).await, linked);

    sqlx::query("UPDATE models SET projector_path = NULL WHERE id = 3")
        .execute(&pool)
        .await
        .unwrap();
    create_schema(&pool).await.unwrap();
    assert_eq!(projector_of(&pool, 3).await, None, "an unlink is kept");
    assert_eq!(everything_else(&pool).await, linked);
}

/// The shard list alone is enough, and it holds absolute paths.
#[tokio::test]
async fn a_projector_named_only_in_the_shard_list_is_linked() {
    let dir = tempfile::tempdir().unwrap();
    let weights = touch(dir.path(), "X.Q8_0.gguf");
    let projector = touch(dir.path(), "mmproj-F16.gguf");
    let listed = serde_json::to_string(&[&weights, &projector]).unwrap();
    let pool = old_library().await;
    add_model(&pool, 1, dir.path(), "X.Q8_0.gguf", Some(&listed), &[]).await;

    create_schema(&pool).await.unwrap();

    assert_eq!(projector_of(&pool, 1).await, Some(projector));
}

/// Model 1 has no projector among its files; model 2's is not on disk, and a
/// link to it would fail a launch that works today; model 4 *is* a file named
/// as a projector, and is not linked to itself.
#[tokio::test]
async fn a_model_without_a_projector_on_disk_among_its_files_is_left_unlinked() {
    let dir = tempfile::tempdir().unwrap();
    touch(dir.path(), "plain.Q8_0.gguf");
    touch(dir.path(), "gone.Q8_0.gguf");
    touch(dir.path(), "mmproj-solo.gguf");
    let pool = old_library().await;
    let shards = ["plain-00001-of-00002.gguf", "plain-00002-of-00002.gguf"];
    add_model(&pool, 1, dir.path(), "plain.Q8_0.gguf", None, &shards).await;
    let gone = ["gone.Q8_0.gguf", "gone.mmproj-Q8_0.gguf"];
    add_model(&pool, 2, dir.path(), "gone.Q8_0.gguf", None, &gone).await;
    let solo = ["mmproj-solo.gguf"];
    add_model(
        &pool,
        4,
        dir.path(),
        "mmproj-solo.gguf",
        Some("not json"),
        &solo,
    )
    .await;

    create_schema(&pool).await.unwrap();

    assert_eq!(projector_of(&pool, 1).await, None);
    assert_eq!(projector_of(&pool, 2).await, None);
    assert_eq!(projector_of(&pool, 4).await, None);
}

/// A library made by this build has the column from the start, and a model
/// added to it is not linked behind its user's back.
#[tokio::test]
async fn a_library_that_has_the_column_is_never_linked() {
    let dir = tempfile::tempdir().unwrap();
    touch(dir.path(), "X.Q8_0.gguf");
    touch(dir.path(), "X.mmproj-Q8_0.gguf");
    let pool = pool().await;
    create_schema(&pool).await.unwrap();
    let files = ["X.Q8_0.gguf", "X.mmproj-Q8_0.gguf"];
    add_model(&pool, 3, dir.path(), "X.Q8_0.gguf", None, &files).await;

    create_schema(&pool).await.unwrap();

    assert_eq!(projector_of(&pool, 3).await, None);
}

/// A library old enough to need the `models` rebuild too gets both in one
/// start, and the rebuild, which runs last, carries the link over.
#[tokio::test]
async fn the_link_survives_the_models_rebuild_of_the_same_start() {
    let dir = tempfile::tempdir().unwrap();
    touch(dir.path(), "X.Q8_0.gguf");
    let projector = touch(dir.path(), "X.mmproj-Q8_0.gguf");
    let pool = pool().await;
    create_schema(&pool).await.unwrap();
    let files = ["X.Q8_0.gguf", "X.mmproj-Q8_0.gguf"];
    add_model(&pool, 3, dir.path(), "X.Q8_0.gguf", None, &files).await;
    super::super::models_tests::make_legacy(&pool, "SELECT 1").await;

    create_schema(&pool).await.unwrap();

    let sql = super::super::models_tests::models_sql(&pool).await;
    assert!(sql.contains("AUTOINCREMENT"), "rebuilt: {sql}");
    assert_eq!(projector_of(&pool, 3).await, Some(projector));
}
