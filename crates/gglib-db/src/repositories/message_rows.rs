//! A message's row and the links to its images, written and read as one.
//!
//! Every path that saves a message goes through [`insert`], inside the
//! caller's transaction, so a message and its image links are saved together
//! or not at all.

use std::collections::HashMap;

use sqlx::{Row, SqliteConnection, SqlitePool};

use gglib_core::domain::attachment::{AttachmentId, AttachmentInfo};
use gglib_core::domain::chat::NewMessage;
use gglib_core::ports::attachment_store::AttachmentError;
use gglib_core::ports::chat_history::ChatHistoryError;

use super::sqlite_attachment_store::info_of;

fn db(error: &sqlx::Error) -> ChatHistoryError {
    ChatHistoryError::Database(error.to_string())
}

/// Insert `msg` and a link to each image it carries, and answer its id.
///
/// An id the store lacks is [`AttachmentError::NotFound`], asked before the
/// link is written so the refusal names the id. The caller's transaction is
/// then dropped uncommitted, and nothing of the message is saved.
pub(super) async fn insert(
    tx: &mut SqliteConnection,
    msg: &NewMessage,
) -> Result<i64, ChatHistoryError> {
    let metadata = msg
        .metadata
        .as_ref()
        .map(|m| serde_json::to_string(m).unwrap_or_default());
    let message_id = sqlx::query(
        "INSERT INTO chat_messages (conversation_id, role, content, metadata) VALUES (?, ?, ?, ?)",
    )
    .bind(msg.conversation_id)
    .bind(msg.role.as_str())
    .bind(&msg.content)
    .bind(&metadata)
    .execute(&mut *tx)
    .await
    .map_err(|e| db(&e))?
    .last_insert_rowid();
    for (position, image) in (0_i64..).zip(&msg.images) {
        link(tx, message_id, position, image).await?;
    }
    Ok(message_id)
}

async fn link(
    tx: &mut SqliteConnection,
    message_id: i64,
    position: i64,
    image: &AttachmentId,
) -> Result<(), ChatHistoryError> {
    let stored = sqlx::query("SELECT 1 FROM attachments WHERE id = ?")
        .bind(image.as_str())
        .fetch_optional(&mut *tx)
        .await
        .map_err(|e| db(&e))?;
    if stored.is_none() {
        return Err(AttachmentError::NotFound(image.clone()).into());
    }
    sqlx::query(
        "INSERT INTO message_attachments (message_id, attachment_id, position) VALUES (?, ?, ?)",
    )
    .bind(message_id)
    .bind(image.as_str())
    .bind(position)
    .execute(&mut *tx)
    .await
    .map_err(|e| db(&e))?;
    Ok(())
}

/// Link `image` to message `message_id` after the images it has, unless it
/// already carries it. [`AttachmentError::NotFound`] for an image the store
/// lacks, as [`insert`] answers it.
pub(super) async fn link_after(
    tx: &mut SqliteConnection,
    message_id: i64,
    image: &AttachmentId,
) -> Result<(), ChatHistoryError> {
    let carried: Option<i64> = sqlx::query_scalar(
        "SELECT position FROM message_attachments WHERE message_id = ? AND attachment_id = ?",
    )
    .bind(message_id)
    .bind(image.as_str())
    .fetch_optional(&mut *tx)
    .await
    .map_err(|e| db(&e))?;
    if carried.is_some() {
        return Ok(());
    }
    let next: i64 = sqlx::query_scalar(
        "SELECT COALESCE(MAX(position), -1) + 1 FROM message_attachments WHERE message_id = ?",
    )
    .bind(message_id)
    .fetch_one(&mut *tx)
    .await
    .map_err(|e| db(&e))?;
    link(tx, message_id, next, image).await
}

/// The images of message `message_id`, in order. No bytes are read.
pub(super) async fn images_of(
    tx: &mut SqliteConnection,
    message_id: i64,
) -> Result<Vec<AttachmentInfo>, ChatHistoryError> {
    let rows = sqlx::query(
        "SELECT a.id, a.mime, a.width, a.height FROM message_attachments l \
         JOIN attachments a ON a.id = l.attachment_id \
         WHERE l.message_id = ? ORDER BY l.position",
    )
    .bind(message_id)
    .fetch_all(&mut *tx)
    .await
    .map_err(|e| db(&e))?;
    Ok(rows.iter().filter_map(info_of).collect())
}

/// Stamp `conversation_id` as changed now.
pub(super) async fn touch(
    tx: &mut SqliteConnection,
    conversation_id: i64,
) -> Result<(), ChatHistoryError> {
    sqlx::query("UPDATE chat_conversations SET updated_at = datetime('now') WHERE id = ?")
        .bind(conversation_id)
        .execute(&mut *tx)
        .await
        .map_err(|e| db(&e))?;
    Ok(())
}

/// The images of every message of `conversation_id`, by message id, each
/// list in the order it was sent. No bytes are read.
pub(super) async fn images_by_message(
    pool: &SqlitePool,
    conversation_id: i64,
) -> Result<HashMap<i64, Vec<AttachmentInfo>>, ChatHistoryError> {
    let rows = sqlx::query(
        "SELECT l.message_id, a.id, a.mime, a.width, a.height \
         FROM message_attachments l \
         JOIN attachments a ON a.id = l.attachment_id \
         JOIN chat_messages m ON m.id = l.message_id \
         WHERE m.conversation_id = ? \
         ORDER BY l.message_id, l.position",
    )
    .bind(conversation_id)
    .fetch_all(pool)
    .await
    .map_err(|e| db(&e))?;
    let mut images: HashMap<i64, Vec<AttachmentInfo>> = HashMap::new();
    for row in &rows {
        if let Some(info) = info_of(row) {
            images.entry(row.get("message_id")).or_default().push(info);
        }
    }
    Ok(images)
}
