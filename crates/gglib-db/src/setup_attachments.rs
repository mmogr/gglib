//! The `attachments` and `message_attachments` tables, and the sweep of
//! images no message carries.
//!
//! A `#[path]` child of `setup.rs`. `create_schema` creates both tables
//! after `chat_messages`, which `message_attachments` refers to.

use anyhow::Result;
use sqlx::SqlitePool;

/// Creates `attachments`, an image's bytes under the hash of them, and
/// `message_attachments`, the images each message carries, in order.
///
/// A link goes with its message: deleting the message, or its conversation,
/// deletes the link by cascade. An image is deleted only by
/// [`sweep_unlinked_attachments`], which reads `created_at`: when the image
/// was last stored, in UTC, as `datetime('now')` writes it.
pub(super) async fn create_attachment_tables(pool: &SqlitePool) -> Result<()> {
    sqlx::query(
        r"
        CREATE TABLE IF NOT EXISTS attachments (
            id TEXT PRIMARY KEY NOT NULL,
            mime TEXT NOT NULL,
            width INTEGER NOT NULL,
            height INTEGER NOT NULL,
            data BLOB NOT NULL,
            created_at TEXT NOT NULL
        )
        ",
    )
    .execute(pool)
    .await?;

    sqlx::query(
        r"
        CREATE TABLE IF NOT EXISTS message_attachments (
            message_id INTEGER NOT NULL REFERENCES chat_messages(id) ON DELETE CASCADE,
            attachment_id TEXT NOT NULL REFERENCES attachments(id),
            position INTEGER NOT NULL,
            PRIMARY KEY (message_id, position)
        )
        ",
    )
    .execute(pool)
    .await?;

    // The sweep asks, of every image, whether a link names it.
    sqlx::query(
        "CREATE INDEX IF NOT EXISTS idx_message_attachments_attachment \
         ON message_attachments(attachment_id)",
    )
    .execute(pool)
    .await?;
    Ok(())
}

/// How long an image no message carries is kept after it was last stored,
/// as the `SQLite` time modifier that reaches back that far.
///
/// An image is stored before the message that carries it is saved, so the
/// image of a turn in flight is unlinked for as long as that turn takes.
/// The daemon's start is no proof that no turn is in flight: the CLI stores
/// an image in its own process and then starts the daemon when none is up,
/// and a daemon may start during any CLI turn. A day outlasts a turn.
const UNLINKED_GRACE: &str = "-1 day";

/// Delete every stored image that no message carries and that was last
/// stored more than a day ago, and answer how many went.
///
/// Call this **once** at daemon boot, after the schema is ready, and nowhere
/// else: no CLI command deletes an image.
pub async fn sweep_unlinked_attachments(pool: &SqlitePool) -> Result<u64> {
    let swept = sqlx::query(
        "DELETE FROM attachments \
         WHERE created_at < datetime('now', ?) \
         AND NOT EXISTS \
         (SELECT 1 FROM message_attachments WHERE attachment_id = attachments.id)",
    )
    .bind(UNLINKED_GRACE)
    .execute(pool)
    .await?;
    Ok(swept.rows_affected())
}

#[cfg(test)]
#[path = "setup_attachments_tests.rs"]
mod tests;
