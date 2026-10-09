//! `SQLite` implementation of the `AttachmentStore` trait.

use async_trait::async_trait;
use sqlx::{Row, SqlitePool, sqlite::SqliteRow};

use gglib_core::domain::attachment::{AttachmentBlob, AttachmentId, AttachmentInfo};
use gglib_core::ports::attachment_store::{AttachmentError, AttachmentStore};

/// `SQLite` implementation of the `AttachmentStore` trait: one row an image,
/// its bytes in a BLOB.
pub struct SqliteAttachmentStore {
    pool: SqlitePool,
}

impl SqliteAttachmentStore {
    /// Create a new `SQLite` attachment store.
    pub const fn new(pool: SqlitePool) -> Self {
        Self { pool }
    }
}

/// A database failure as the port's error. `sqlx` errors name the statement
/// and never a bound value, so no image byte is in the text.
fn storage(error: &sqlx::Error) -> AttachmentError {
    AttachmentError::Storage(error.to_string())
}

/// An `attachments` row, less its bytes. `None` when the row's id or size
/// is not one this store wrote.
pub(super) fn info_of(row: &SqliteRow) -> Option<AttachmentInfo> {
    Some(AttachmentInfo {
        id: AttachmentId::parse(row.get("id")).ok()?,
        mime: row.get("mime"),
        width: u32::try_from(row.get::<i64, _>("width")).ok()?,
        height: u32::try_from(row.get::<i64, _>("height")).ok()?,
    })
}

#[async_trait]
impl AttachmentStore for SqliteAttachmentStore {
    /// An image already stored keeps its row and its bytes; only when it
    /// was last stored moves, so the sweep's day of grace starts again for
    /// an image sent a second time.
    async fn put(&self, info: &AttachmentInfo, bytes: &[u8]) -> Result<(), AttachmentError> {
        sqlx::query(
            "INSERT INTO attachments (id, mime, width, height, data, created_at) \
             VALUES (?, ?, ?, ?, ?, datetime('now')) \
             ON CONFLICT(id) DO UPDATE SET created_at = excluded.created_at",
        )
        .bind(info.id.as_str())
        .bind(&info.mime)
        .bind(i64::from(info.width))
        .bind(i64::from(info.height))
        .bind(bytes)
        .execute(&self.pool)
        .await
        .map_err(|e| storage(&e))?;
        Ok(())
    }

    async fn info(&self, id: &AttachmentId) -> Result<Option<AttachmentInfo>, AttachmentError> {
        let row = sqlx::query("SELECT id, mime, width, height FROM attachments WHERE id = ?")
            .bind(id.as_str())
            .fetch_optional(&self.pool)
            .await
            .map_err(|e| storage(&e))?;
        Ok(row.as_ref().and_then(info_of))
    }

    async fn size(&self, id: &AttachmentId) -> Result<Option<usize>, AttachmentError> {
        let row = sqlx::query("SELECT length(data) AS size FROM attachments WHERE id = ?")
            .bind(id.as_str())
            .fetch_optional(&self.pool)
            .await
            .map_err(|e| storage(&e))?;
        Ok(row.and_then(|row| usize::try_from(row.get::<i64, _>("size")).ok()))
    }

    async fn blob(&self, id: &AttachmentId) -> Result<Option<AttachmentBlob>, AttachmentError> {
        let row = sqlx::query("SELECT mime, data FROM attachments WHERE id = ?")
            .bind(id.as_str())
            .fetch_optional(&self.pool)
            .await
            .map_err(|e| storage(&e))?;
        Ok(row.map(|row| AttachmentBlob {
            mime: row.get("mime"),
            data: row.get("data"),
        }))
    }

    async fn ids_starting_with(&self, prefix: &str) -> Result<Vec<AttachmentId>, AttachmentError> {
        let rows = sqlx::query(
            "SELECT id FROM attachments WHERE substr(id, 1, length(?1)) = ?1 ORDER BY id",
        )
        .bind(prefix)
        .fetch_all(&self.pool)
        .await
        .map_err(|e| storage(&e))?;
        Ok(rows
            .iter()
            .filter_map(|row| AttachmentId::parse(row.get("id")).ok())
            .collect())
    }
}

#[cfg(test)]
#[path = "sqlite_attachment_store_tests.rs"]
mod sqlite_attachment_store_tests;
