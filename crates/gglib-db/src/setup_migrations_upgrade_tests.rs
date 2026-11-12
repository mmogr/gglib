//! A library never stamped, opened by this build: its paths, keys and
//! projector links come out as every build since the projector link has made
//! them. Through `setup_database` alone, so it runs unchanged against a build
//! from before the steps were one list.

use std::os::unix::fs::symlink;
use std::path::Path;

use gglib_core::ports::{FirstBytesParser, NoopGgufParser};
use sqlx::SqlitePool;

use crate::repositories::sqlite_model_repository::local_model_key_for;
use crate::setup_database;

/// `name` under `dir`, as text.
fn at(dir: &Path, name: &str) -> String {
    dir.join(name).to_string_lossy().into_owned()
}

/// A file `name` in `dir` whose header says `role`.
fn file(dir: &Path, name: &str, role: &str) {
    std::fs::create_dir_all(dir).expect("a dir");
    std::fs::write(dir.join(name), role).expect("a file");
}

/// Model `id` at `path` under `key`, its shard list `listed`, and its
/// `model_files` names `rows`.
async fn plant(
    pool: &SqlitePool,
    id: i64,
    path: &str,
    key: &str,
    listed: Option<&str>,
    rows: &[&str],
) {
    sqlx::query(
        "INSERT INTO models (id, name, file_path, param_count_b, added_at, model_key, \
         file_paths_json) VALUES (?, ?, ?, 7.0, 'then', ?, ?)",
    )
    .bind(id)
    .bind(format!("model-{id}"))
    .bind(path)
    .bind(key)
    .bind(listed)
    .execute(pool)
    .await
    .expect("a model");
    for (index, row) in (0_i64..).zip(rows) {
        sqlx::query(
            "INSERT INTO model_files (model_id, file_path, file_index, expected_size) \
             VALUES (?, ?, ?, 10)",
        )
        .bind(id)
        .bind(row)
        .bind(index)
        .execute(pool)
        .await
        .expect("a file row");
    }
}

#[tokio::test]
async fn a_library_never_stamped_comes_out_canonical_and_linked() {
    let dir = tempfile::tempdir().expect("tempdir");
    let root = std::fs::canonicalize(dir.path()).expect("resolved");
    let (store, link) = (root.join("store"), dir.path().join("link"));
    file(&store, "a-1.gguf", "weights");
    file(&store, "a-2.gguf", "weights");
    file(&store, "mmproj-a.gguf", "projector");
    file(&store.join("c"), "c.gguf", "weights");
    file(&store.join("c"), "mmproj-c.gguf", "projector");
    file(&root.join("elsewhere"), "b.gguf", "weights");
    file(&root.join("elsewhere"), "mmproj-b.gguf", "projector");
    symlink(&store, &link).expect("a linked dir");
    std::fs::create_dir(dir.path().join("links")).expect("a dir");
    let b_link = dir.path().join("links").join("b-link.gguf");
    symlink(root.join("elsewhere").join("b.gguf"), &b_link).expect("a linked file");

    let db = dir.path().join("gglib.db");
    let pool = setup_database(&db, &NoopGgufParser)
        .await
        .expect("a fresh database");
    sqlx::raw_sql("ALTER TABLE models DROP COLUMN projector_path; PRAGMA user_version = 0;")
        .execute(&pool)
        .await
        .expect("an old library");
    // A local model through a linked dir, its projector one of its shards.
    let a = at(&link, "a-1.gguf");
    let a_list = serde_json::json!([a, at(&link, "a-2.gguf"), at(&link, "mmproj-a.gguf")]);
    plant(
        &pool,
        1,
        &a,
        &local_model_key_for(&a),
        Some(&a_list.to_string()),
        &[],
    )
    .await;
    // A local model whose file is a link, its projector beside the target.
    let b = b_link.to_string_lossy().into_owned();
    let b_rows = ["b.gguf", "mmproj-b.gguf"];
    plant(&pool, 2, &b, &local_model_key_for(&b), None, &b_rows).await;
    // A download through a linked dir.
    let c = at(&link.join("c"), "c.gguf");
    let c_list = serde_json::json!([c]).to_string();
    let c_rows = ["c.gguf", "mmproj-c.gguf"];
    plant(&pool, 3, &c, "hf:o/c@sha#c.gguf", Some(&c_list), &c_rows).await;
    pool.close().await;

    let pool = setup_database(&db, &FirstBytesParser)
        .await
        .expect("the upgrade");

    let models: Vec<(String, String, Option<String>, Option<String>)> = sqlx::query_as(
        "SELECT file_path, model_key, file_paths_json, projector_path FROM models ORDER BY id",
    )
    .fetch_all(&pool)
    .await
    .expect("the models");
    let a = at(&store, "a-1.gguf");
    let a_list = serde_json::json!([a, at(&store, "a-2.gguf"), at(&store, "mmproj-a.gguf")]);
    let b = at(&root.join("elsewhere"), "b.gguf");
    let c_list = serde_json::json!([at(&store.join("c"), "c.gguf")]).to_string();
    assert_eq!(
        models,
        [
            (
                a.clone(),
                local_model_key_for(&a),
                Some(a_list.to_string()),
                Some(at(&store, "mmproj-a.gguf")),
            ),
            (
                b.clone(),
                local_model_key_for(&b),
                None,
                Some(at(&root.join("elsewhere"), "mmproj-b.gguf")),
            ),
            (
                c,
                "hf:o/c@sha#c.gguf".to_owned(),
                Some(c_list),
                Some(at(&store.join("c"), "mmproj-c.gguf")),
            ),
        ]
    );
}
