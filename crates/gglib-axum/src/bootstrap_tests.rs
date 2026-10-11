//! What the daemon's bootstrap does to the database it opens.

use gglib_core::domain::AttachmentId;
use gglib_core::ports::AttachmentError;

use super::*;

/// Store the image whose bytes are `bytes`, linked to no message, as if
/// `age` ago: an `SQLite` time modifier.
async fn unlinked(pool: &sqlx::SqlitePool, bytes: &[u8], age: &str) -> AttachmentId {
    let id = AttachmentId::of(bytes);
    sqlx::query(
        "INSERT INTO attachments (id, mime, width, height, data, created_at) \
         VALUES (?, 'image/png', 1, 1, ?, datetime('now', ?))",
    )
    .bind(id.as_str())
    .bind(bytes)
    .bind(age)
    .execute(pool)
    .await
    .unwrap();
    id
}

/// The bootstrap runs the startup repairs on the database it opens: an old
/// image no message carries is gone once it returns, and one just stored,
/// which may be a turn's in flight, is still there.
#[tokio::test]
async fn bootstrap_deletes_an_old_unlinked_image_and_keeps_a_fresh_one() {
    let dir = tempfile::tempdir().unwrap();
    let db_path = dir.path().join("gglib.db");
    let config = || ServerConfig {
        base_port: Some(19_300),
        llama_server_path: "/nonexistent/llama-server".into(),
        sd_server_path: "/nonexistent/sd-server".into(),
        db_path: Some(db_path.clone()),
        device_keys_path: Some(dir.path().join("remote_devices")),
        ..ServerConfig::with_defaults().unwrap()
    };
    // A first bootstrap lays the schema down; the images are stored after it,
    // so only the second one's repairs can have judged them.
    drop(bootstrap(config()).await.unwrap());
    let url = format!("sqlite://{}", db_path.display());
    let pool = sqlx::SqlitePool::connect(&url).await.unwrap();
    let old = unlinked(&pool, b"left by a turn never sent", "-2 days").await;
    let fresh = unlinked(&pool, b"of a turn in flight", "-1 minute").await;
    pool.close().await;

    let ctx = bootstrap(config()).await.unwrap();

    let attachments = ctx.core.attachments();
    assert!(matches!(
        attachments.blob(&old).await,
        Err(AttachmentError::NotFound(_))
    ));
    assert_eq!(
        attachments.blob(&fresh).await.unwrap().data,
        b"of a turn in flight"
    );
}
