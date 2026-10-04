//! Tests for the attachment tables and the sweep of unlinked images.

use super::super::{setup_database, setup_test_database};
use super::*;

/// Store an image whose id and bytes are `id`, as if `age` ago: an
/// `SQLite` time modifier, such as `-2 days`.
async fn put_aged(pool: &SqlitePool, id: &str, age: &str) {
    sqlx::query(
        "INSERT INTO attachments (id, mime, width, height, data, created_at) \
         VALUES (?, 'image/png', 1, 1, ?, datetime('now', ?))",
    )
    .bind(id)
    .bind(id.as_bytes())
    .bind(age)
    .execute(pool)
    .await
    .unwrap();
}

/// Store an image whose id and bytes are `id`, long enough ago to be swept
/// when nothing links it.
async fn put(pool: &SqlitePool, id: &str) {
    put_aged(pool, id, "-2 days").await;
}

/// Save a message that carries `images`, and answer its conversation.
async fn message_with(pool: &SqlitePool, images: &[&str]) -> i64 {
    let conversation = sqlx::query("INSERT INTO chat_conversations (title) VALUES ('c')")
        .execute(pool)
        .await
        .unwrap()
        .last_insert_rowid();
    let message = sqlx::query(
        "INSERT INTO chat_messages (conversation_id, role, content) VALUES (?, 'user', 'look')",
    )
    .bind(conversation)
    .execute(pool)
    .await
    .unwrap()
    .last_insert_rowid();
    for (position, image) in (0_i64..).zip(images) {
        sqlx::query(
            "INSERT INTO message_attachments (message_id, attachment_id, position) \
             VALUES (?, ?, ?)",
        )
        .bind(message)
        .bind(image)
        .bind(position)
        .execute(pool)
        .await
        .unwrap();
    }
    conversation
}

async fn stored(pool: &SqlitePool) -> Vec<String> {
    sqlx::query_scalar("SELECT id FROM attachments ORDER BY id")
        .fetch_all(pool)
        .await
        .unwrap()
}

#[tokio::test]
async fn the_sweep_deletes_only_images_no_message_carries() {
    let pool = setup_test_database().await.unwrap();
    for id in ["linked", "linked-twice", "orphan-a", "orphan-b"] {
        put(&pool, id).await;
    }
    message_with(&pool, &["linked", "linked-twice"]).await;
    message_with(&pool, &["linked-twice"]).await;

    assert_eq!(sweep_unlinked_attachments(&pool).await.unwrap(), 2);

    assert_eq!(stored(&pool).await, ["linked", "linked-twice"]);
    // Nothing is left to sweep, and the links are as they were.
    assert_eq!(sweep_unlinked_attachments(&pool).await.unwrap(), 0);
    let links: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM message_attachments")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(links, 3);
}

#[tokio::test]
async fn an_image_is_swept_once_the_last_message_carrying_it_is_gone() {
    let pool = setup_test_database().await.unwrap();
    put(&pool, "shared").await;
    let first = message_with(&pool, &["shared"]).await;
    let second = message_with(&pool, &["shared"]).await;

    sqlx::query("DELETE FROM chat_conversations WHERE id = ?")
        .bind(first)
        .execute(&pool)
        .await
        .unwrap();
    assert_eq!(sweep_unlinked_attachments(&pool).await.unwrap(), 0);
    assert_eq!(stored(&pool).await, ["shared"]);

    sqlx::query("DELETE FROM chat_conversations WHERE id = ?")
        .bind(second)
        .execute(&pool)
        .await
        .unwrap();
    assert_eq!(sweep_unlinked_attachments(&pool).await.unwrap(), 1);
    assert!(stored(&pool).await.is_empty());
}

/// An unlinked image stored within the last day may belong to a turn in
/// flight, so the sweep leaves it; one stored before that goes.
#[tokio::test]
async fn the_sweep_leaves_an_unlinked_image_stored_within_the_last_day() {
    let pool = setup_test_database().await.unwrap();
    put_aged(&pool, "just-stored", "-0 seconds").await;
    put_aged(&pool, "an-hour-old", "-1 hour").await;
    put_aged(&pool, "almost-a-day-old", "-23 hours").await;
    put_aged(&pool, "over-a-day-old", "-25 hours").await;

    assert_eq!(sweep_unlinked_attachments(&pool).await.unwrap(), 1);

    assert_eq!(
        stored(&pool).await,
        ["almost-a-day-old", "an-hour-old", "just-stored"]
    );
}

/// Opening the database is what every CLI invocation does. It must leave an
/// image that is uploaded and not yet linked where it is.
#[tokio::test]
async fn opening_the_database_again_sweeps_nothing() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("gglib.db");
    let pool = setup_database(&path).await.unwrap();
    put(&pool, "just-uploaded").await;
    pool.close().await;

    let pool = setup_database(&path).await.unwrap();
    assert_eq!(stored(&pool).await, ["just-uploaded"]);
    pool.close().await;
}

/// The pool `setup_database` opens enforces foreign keys: a link cannot name
/// an image or a message that is not there, and a deleted message takes its
/// links with it.
#[tokio::test]
async fn the_database_pool_enforces_the_links_foreign_keys() {
    let dir = tempfile::tempdir().unwrap();
    let pool = setup_database(&dir.path().join("gglib.db")).await.unwrap();
    put(&pool, "stored").await;
    let conversation = message_with(&pool, &["stored"]).await;
    let message: i64 = sqlx::query_scalar("SELECT id FROM chat_messages")
        .fetch_one(&pool)
        .await
        .unwrap();

    let link = |message: i64, image: &'static str| {
        sqlx::query(
            "INSERT INTO message_attachments (message_id, attachment_id, position) \
             VALUES (?, ?, 7)",
        )
        .bind(message)
        .bind(image)
    };
    assert!(link(message, "not-stored").execute(&pool).await.is_err());
    assert!(link(message + 1, "stored").execute(&pool).await.is_err());
    // A linked image cannot be deleted from under its message.
    assert!(
        sqlx::query("DELETE FROM attachments")
            .execute(&pool)
            .await
            .is_err()
    );

    sqlx::query("DELETE FROM chat_conversations WHERE id = ?")
        .bind(conversation)
        .execute(&pool)
        .await
        .unwrap();
    let links: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM message_attachments")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(links, 0);
    pool.close().await;
}
