//! Step 9: which chats lose the four sampling keys, that every other key
//! keeps its value and place, and which rows keep their text exactly.

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

/// Add a chat titled `title` whose stored `settings` text is `settings`.
async fn plant(pool: &SqlitePool, title: &str, settings: Option<&str>) {
    sqlx::query("INSERT INTO chat_conversations (title, settings) VALUES (?, ?)")
        .bind(title)
        .bind(settings)
        .execute(pool)
        .await
        .unwrap();
}

/// The `settings` text stored for `title`.
async fn settings_of(pool: &SqlitePool, title: &str) -> Option<String> {
    sqlx::query_scalar("SELECT settings FROM chat_conversations WHERE title = ?")
        .bind(title)
        .fetch_one(pool)
        .await
        .unwrap()
}

/// Stamp the library as a build at step 8 left it.
async fn stamp_older(pool: &SqlitePool) {
    sqlx::query("PRAGMA user_version = 8")
        .execute(pool)
        .await
        .unwrap();
}

#[tokio::test]
async fn the_four_sampling_keys_go_and_the_rest_keep_their_order() {
    let pool = library().await;
    plant(
        &pool,
        "all",
        Some(
            r#"{"model_name":"Qwen","temperature":0.6,"top_p":0.9,"top_k":20,"max_tokens":1024,"repeat_penalty":1.1,"profile":"coding"}"#,
        ),
    )
    .await;
    plant(&pool, "one", Some(r#"{"top_k":40}"#)).await;
    // A value this build could never read goes all the same.
    plant(
        &pool,
        "odd",
        Some(r#"{"temperature":"warm","later":[1],"thinking":"off"}"#),
    )
    .await;
    stamp_older(&pool).await;

    run(&pool, LATEST).await.unwrap();

    assert_eq!(
        settings_of(&pool, "all").await.as_deref(),
        Some(r#"{"model_name":"Qwen","max_tokens":1024,"profile":"coding"}"#)
    );
    assert_eq!(settings_of(&pool, "one").await.as_deref(), Some("{}"));
    assert_eq!(
        settings_of(&pool, "odd").await.as_deref(),
        Some(r#"{"later":[1],"thinking":"off"}"#)
    );
    assert_eq!(user_version(&pool).await.unwrap(), LATEST);
}

/// A chat with none of the four keeps its text byte for byte, and so does a
/// row this build cannot read as settings: text that is not JSON, a JSON
/// array, and NULL.
#[tokio::test]
async fn a_row_without_the_keys_or_unreadable_settings_is_not_rewritten() {
    let pool = library().await;
    let kept = [
        (
            "spaced",
            Some(r#"{ "model_name": "Qwen", "max_tokens": 64 }"#),
        ),
        ("garbage", Some("not json")),
        ("array", Some(r#"[{"temperature":0.2}]"#)),
        ("none", None),
    ];
    for (title, settings) in kept {
        plant(&pool, title, settings).await;
    }
    stamp_older(&pool).await;

    run(&pool, LATEST).await.unwrap();

    for (title, settings) in kept {
        assert_eq!(
            settings_of(&pool, title).await.as_deref(),
            settings,
            "{title}"
        );
    }
}
