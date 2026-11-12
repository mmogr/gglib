//! Step 8: which models lose `format:*` tags, which keep their text exactly,
//! and that the order of the tags left is kept.

use sqlx::SqlitePool;
use sqlx::sqlite::SqlitePoolOptions;

use super::super::create_schema;
use super::super::migrations::{LATEST, run, user_version};

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

/// Add a model named `name` whose stored `tags` text is `tags`.
async fn plant(pool: &SqlitePool, name: &str, tags: Option<&str>) {
    sqlx::query(
        "INSERT INTO models (name, file_path, added_at, model_key, tags) \
         VALUES (?, ?, 'then', ?, ?)",
    )
    .bind(name)
    .bind(format!("/models/{name}.gguf"))
    .bind(format!("local:{name}"))
    .bind(tags)
    .execute(pool)
    .await
    .unwrap();
}

/// The `tags` text stored for `name`.
async fn tags_of(pool: &SqlitePool, name: &str) -> Option<String> {
    sqlx::query_scalar("SELECT tags FROM models WHERE name = ?")
        .bind(name)
        .fetch_one(pool)
        .await
        .unwrap()
}

/// Stamp the library as a build at step 7 left it.
async fn stamp_older(pool: &SqlitePool) {
    sqlx::query("PRAGMA user_version = 7")
        .execute(pool)
        .await
        .unwrap();
}

#[tokio::test]
async fn every_format_tag_goes_and_the_rest_keep_their_order() {
    let pool = library().await;
    plant(
        &pool,
        "agent",
        Some(r#"["format:hermes","reasoning","format:think-tag","agent"]"#),
    )
    .await;
    plant(&pool, "only", Some(r#"["format:qwen-xml"]"#)).await;
    plant(&pool, "owner", Some(r#"["Format:mine","format:mistral"]"#)).await;
    stamp_older(&pool).await;

    run(&pool, LATEST).await.unwrap();

    assert_eq!(
        tags_of(&pool, "agent").await.as_deref(),
        Some(r#"["reasoning","agent"]"#)
    );
    assert_eq!(tags_of(&pool, "only").await.as_deref(), Some("[]"));
    // The prefix as detection wrote it; a tag spelled otherwise is a user's.
    assert_eq!(
        tags_of(&pool, "owner").await.as_deref(),
        Some(r#"["Format:mine"]"#)
    );
    assert_eq!(user_version(&pool).await.unwrap(), LATEST);
}

/// A row with no `format:*` tag keeps its text byte for byte, and so does a
/// row this build cannot read as a list of tags: text that is not JSON, a
/// JSON object, and NULL.
#[tokio::test]
async fn a_row_without_format_tags_or_unreadable_tags_is_not_rewritten() {
    let pool = library().await;
    let kept = [
        ("spaced", Some(r#"[ "agent", "reasoning" ]"#)),
        ("garbage", Some("not json")),
        ("object", Some(r#"{"a":"format:hermes"}"#)),
        ("none", None),
    ];
    for (name, tags) in kept {
        plant(&pool, name, tags).await;
    }
    stamp_older(&pool).await;

    run(&pool, LATEST).await.unwrap();

    for (name, tags) in kept {
        assert_eq!(tags_of(&pool, name).await.as_deref(), tags, "{name}");
    }
}

/// Once stamped, a `format:` tag added later is left alone: it is a user's.
#[tokio::test]
async fn the_step_runs_once() {
    let pool = library().await;
    plant(&pool, "typed", Some(r#"["format:mine"]"#)).await;

    run(&pool, LATEST).await.unwrap();

    assert_eq!(
        tags_of(&pool, "typed").await.as_deref(),
        Some(r#"["format:mine"]"#)
    );
}
