//! The `SQLite` attachment store against a test database.

use crate::setup::setup_test_database;

use super::*;

fn info_for(bytes: &[u8], mime: &str, width: u32, height: u32) -> AttachmentInfo {
    AttachmentInfo {
        id: AttachmentId::of(bytes),
        mime: mime.to_owned(),
        width,
        height,
    }
}

async fn store() -> SqliteAttachmentStore {
    let pool = setup_test_database().await.expect("setup_test_database");
    SqliteAttachmentStore::new(pool)
}

async fn rows(store: &SqliteAttachmentStore) -> i64 {
    sqlx::query_scalar("SELECT COUNT(*) FROM attachments")
        .fetch_one(&store.pool)
        .await
        .unwrap()
}

#[tokio::test]
async fn a_stored_image_reads_back_its_facts_and_its_bytes() {
    let store = store().await;
    // Every byte value, so a store that mangled binary data would show.
    let bytes: Vec<u8> = (0..=255).collect();
    let info = info_for(&bytes, "image/jpeg", 4032, 3024);
    store.put(&info, &bytes).await.unwrap();

    assert_eq!(store.info(&info.id).await.unwrap(), Some(info.clone()));
    let blob = store.blob(&info.id).await.unwrap().unwrap();
    assert_eq!(blob.mime, "image/jpeg");
    assert_eq!(blob.data, bytes);
    assert_eq!(store.size(&info.id).await.unwrap(), Some(256));
}

#[tokio::test]
async fn the_same_id_twice_is_one_row_as_first_stored() {
    let store = store().await;
    let info = info_for(b"one image", "image/png", 10, 20);
    store.put(&info, b"one image").await.unwrap();

    let again = AttachmentInfo {
        width: 99,
        ..info.clone()
    };
    store.put(&again, b"other bytes").await.unwrap();

    assert_eq!(rows(&store).await, 1);
    assert_eq!(store.info(&info.id).await.unwrap(), Some(info.clone()));
    assert_eq!(
        store.blob(&info.id).await.unwrap().unwrap().data,
        b"one image"
    );
}

/// Storing an image again says it is wanted now: the sweep counts its day
/// from the last time, so an image sent again for a new turn is not deleted
/// for having first been stored long ago.
#[tokio::test]
async fn storing_an_image_again_moves_when_it_was_last_stored_to_now() {
    let store = store().await;
    let info = info_for(b"one image", "image/png", 10, 20);
    store.put(&info, b"one image").await.unwrap();
    sqlx::query("UPDATE attachments SET created_at = datetime('now', '-2 days')")
        .execute(&store.pool)
        .await
        .unwrap();

    store.put(&info, b"one image").await.unwrap();

    let swept = crate::setup::sweep_unlinked_attachments(&store.pool).await;
    assert_eq!(swept.unwrap(), 0);
    assert_eq!(store.info(&info.id).await.unwrap(), Some(info));
}

#[tokio::test]
async fn an_id_never_stored_is_none() {
    let store = store().await;
    store
        .put(&info_for(b"kept", "image/png", 1, 1), b"kept")
        .await
        .unwrap();
    let missing = AttachmentId::of(b"never stored");
    assert_eq!(store.info(&missing).await.unwrap(), None);
    assert_eq!(store.blob(&missing).await.unwrap(), None);
    assert_eq!(store.size(&missing).await.unwrap(), None);
}

#[tokio::test]
async fn a_store_failure_is_the_storage_error_with_no_code() {
    let store = store().await;
    sqlx::query("DROP TABLE message_attachments")
        .execute(&store.pool)
        .await
        .unwrap();
    sqlx::query("DROP TABLE attachments")
        .execute(&store.pool)
        .await
        .unwrap();
    let bytes = b"SECRETPIXELS";
    let failure = store
        .put(&info_for(bytes, "image/png", 1, 1), bytes)
        .await
        .unwrap_err();
    assert!(matches!(failure, AttachmentError::Storage(_)));
    assert_eq!(failure.code(), None);
    // The failure names the statement, never the image.
    assert!(!failure.to_string().contains("SECRETPIXELS"));
}
