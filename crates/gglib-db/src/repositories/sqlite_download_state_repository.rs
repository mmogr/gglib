//! `SQLite` implementation of the `DownloadStateRepositoryPort` trait.

use async_trait::async_trait;
use sqlx::SqlitePool;

use gglib_core::{
    DownloadId, DownloadStateRepositoryPort, DownloadStatus, Quantization, QueuedDownload,
    RepositoryError, ShardInfo,
};

/// `SQLite` implementation of the `DownloadStateRepositoryPort` trait.
///
/// Persists download queue state to `SQLite` for durability across restarts.
pub struct SqliteDownloadStateRepository {
    pool: SqlitePool,
}

impl SqliteDownloadStateRepository {
    /// Create a new `SQLite` download state repository.
    pub fn new(pool: SqlitePool) -> Self {
        Self { pool }
    }

    /// Get a reference to the underlying pool (for testing/migration only).
    #[cfg(test)]
    pub fn pool(&self) -> &SqlitePool {
        &self.pool
    }
}

#[async_trait]
impl DownloadStateRepositoryPort for SqliteDownloadStateRepository {
    async fn enqueue(&self, download: &QueuedDownload) -> Result<(), RepositoryError> {
        let quantization = download.quantization.as_ref().map(|q| q.to_string());
        let shard_info_json = download
            .shard_info
            .as_ref()
            .map(|s| serde_json::to_string(s).unwrap_or_default());
        let status_str = download.status.as_str();

        sqlx::query(
            r#"
            INSERT INTO download_queue (
                id, model_id, quantization, display_name, status,
                position, downloaded_bytes, total_bytes, queued_at,
                started_at, group_id, shard_info
            ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
            ON CONFLICT(id) DO UPDATE SET
                status = excluded.status,
                position = excluded.position,
                downloaded_bytes = excluded.downloaded_bytes,
                total_bytes = excluded.total_bytes
            "#,
        )
        .bind(&download.id)
        .bind(&download.model_id)
        .bind(&quantization)
        .bind(&download.display_name)
        .bind(status_str)
        .bind(download.position as i64)
        .bind(download.downloaded_bytes as i64)
        .bind(download.total_bytes as i64)
        .bind(download.queued_at as i64)
        .bind(download.started_at.map(|t| t as i64))
        .bind(&download.group_id)
        .bind(&shard_info_json)
        .execute(&self.pool)
        .await
        .map_err(|e| RepositoryError::Storage(e.to_string()))?;

        Ok(())
    }

    async fn update_status(
        &self,
        id: &DownloadId,
        status: DownloadStatus,
    ) -> Result<(), RepositoryError> {
        let status_str = status.as_str();
        let id_str = id.to_string();

        let result = sqlx::query(
            r#"
            UPDATE download_queue SET status = ? WHERE id = ?
            "#,
        )
        .bind(status_str)
        .bind(&id_str)
        .execute(&self.pool)
        .await
        .map_err(|e| RepositoryError::Storage(e.to_string()))?;

        if result.rows_affected() == 0 {
            return Err(RepositoryError::NotFound(format!(
                "Download with ID '{}'",
                id_str
            )));
        }

        Ok(())
    }

    async fn load_queue(&self) -> Result<Vec<QueuedDownload>, RepositoryError> {
        let rows = sqlx::query(
            r#"
            SELECT id, model_id, quantization, display_name, status,
                   position, downloaded_bytes, total_bytes, queued_at,
                   started_at, group_id, shard_info
            FROM download_queue
            WHERE status IN ('queued', 'downloading')
            ORDER BY position ASC
            "#,
        )
        .fetch_all(&self.pool)
        .await
        .map_err(|e| RepositoryError::Storage(e.to_string()))?;

        rows.iter().map(row_to_queued_download).collect()
    }

    async fn mark_failed(
        &self,
        id: &DownloadId,
        error_message: &str,
    ) -> Result<(), RepositoryError> {
        let id_str = id.to_string();

        let result = sqlx::query(
            r#"
            UPDATE download_queue 
            SET status = 'failed', error_message = ?, completed_at = datetime('now')
            WHERE id = ?
            "#,
        )
        .bind(error_message)
        .bind(&id_str)
        .execute(&self.pool)
        .await
        .map_err(|e| RepositoryError::Storage(e.to_string()))?;

        if result.rows_affected() == 0 {
            return Err(RepositoryError::NotFound(format!(
                "Download with ID '{}'",
                id_str
            )));
        }

        Ok(())
    }

    async fn remove(&self, id: &DownloadId) -> Result<(), RepositoryError> {
        sqlx::query(
            r#"
            DELETE FROM download_queue WHERE id = ?
            "#,
        )
        .bind(id.to_string())
        .execute(&self.pool)
        .await
        .map_err(|e| RepositoryError::Storage(e.to_string()))?;

        Ok(())
    }

    async fn prune_completed(&self, older_than_days: u32) -> Result<u32, RepositoryError> {
        let result = sqlx::query(
            r#"
            DELETE FROM download_queue 
            WHERE status IN ('completed', 'failed', 'cancelled')
            AND completed_at < datetime('now', ? || ' days')
            "#,
        )
        .bind(-(older_than_days as i32))
        .execute(&self.pool)
        .await
        .map_err(|e| RepositoryError::Storage(e.to_string()))?;

        Ok(result.rows_affected() as u32)
    }
}

/// Convert a database row to a `QueuedDownload`.
fn row_to_queued_download(
    row: &sqlx::sqlite::SqliteRow,
) -> Result<QueuedDownload, RepositoryError> {
    use sqlx::Row;

    let id: String = row.try_get("id").map_err(map_column_error)?;
    let model_id: String = row.try_get("model_id").map_err(map_column_error)?;
    let quantization_str: Option<String> = row.try_get("quantization").map_err(map_column_error)?;
    let display_name: String = row.try_get("display_name").map_err(map_column_error)?;
    let status_str: String = row.try_get("status").map_err(map_column_error)?;
    let position: i64 = row.try_get("position").map_err(map_column_error)?;
    let downloaded_bytes: i64 = row.try_get("downloaded_bytes").map_err(map_column_error)?;
    let total_bytes: i64 = row.try_get("total_bytes").map_err(map_column_error)?;
    let queued_at: i64 = row.try_get("queued_at").map_err(map_column_error)?;
    let started_at: Option<i64> = row.try_get("started_at").map_err(map_column_error)?;
    let group_id: Option<String> = row.try_get("group_id").map_err(map_column_error)?;
    let shard_info_json: Option<String> = row.try_get("shard_info").map_err(map_column_error)?;

    let quantization = quantization_str.and_then(|s| s.parse::<Quantization>().ok());

    let status = DownloadStatus::parse(&status_str);

    let shard_info: Option<ShardInfo> =
        shard_info_json.and_then(|json| serde_json::from_str(&json).ok());

    let progress_percent = if total_bytes > 0 {
        (downloaded_bytes as f64 / total_bytes as f64) * 100.0
    } else {
        0.0
    };

    Ok(QueuedDownload {
        id,
        model_id,
        quantization,
        display_name,
        status,
        position: position as u32,
        downloaded_bytes: downloaded_bytes as u64,
        total_bytes: total_bytes as u64,
        speed_bps: 0.0,    // Not persisted, calculated live
        eta_seconds: None, // Not persisted, calculated live
        progress_percent,
        queued_at: queued_at as u64,
        started_at: started_at.map(|t| t as u64),
        group_id,
        shard_info,
    })
}

fn map_column_error(e: sqlx::Error) -> RepositoryError {
    RepositoryError::Storage(format!("Column read error: {}", e))
}

#[cfg(test)]
mod tests {
    use super::*;

    async fn setup_test_db() -> SqlitePool {
        let pool = SqlitePool::connect("sqlite::memory:").await.unwrap();

        sqlx::query(
            r#"
            CREATE TABLE IF NOT EXISTS download_queue (
                id TEXT PRIMARY KEY NOT NULL,
                model_id TEXT NOT NULL,
                quantization TEXT,
                display_name TEXT NOT NULL,
                status TEXT NOT NULL DEFAULT 'queued',
                position INTEGER NOT NULL DEFAULT 0,
                downloaded_bytes INTEGER NOT NULL DEFAULT 0,
                total_bytes INTEGER NOT NULL DEFAULT 0,
                queued_at INTEGER NOT NULL,
                started_at INTEGER,
                completed_at TEXT,
                group_id TEXT,
                shard_info TEXT,
                error_message TEXT
            )
            "#,
        )
        .execute(&pool)
        .await
        .unwrap();

        pool
    }

    #[tokio::test]
    async fn test_enqueue_and_load() {
        let pool = setup_test_db().await;
        let repo = SqliteDownloadStateRepository::new(pool);

        let download = QueuedDownload::new(
            "test-id",
            "TheBloke/Llama-2-GGUF",
            "Llama 2 Q4_K_M",
            1,
            1234567890,
        );

        repo.enqueue(&download).await.unwrap();

        let queue = repo.load_queue().await.unwrap();
        assert_eq!(queue.len(), 1);
        assert_eq!(queue[0].id, "test-id");
        assert_eq!(queue[0].model_id, "TheBloke/Llama-2-GGUF");
    }

    #[tokio::test]
    async fn test_update_status() {
        let pool = setup_test_db().await;
        let repo = SqliteDownloadStateRepository::new(pool);

        let download = QueuedDownload::new("status-test", "org/model", "Model", 1, 1234567890);
        repo.enqueue(&download).await.unwrap();

        let id = DownloadId::from_model("status-test");
        repo.update_status(&id, DownloadStatus::Downloading)
            .await
            .unwrap();

        let queue = repo.load_queue().await.unwrap();
        assert_eq!(queue[0].status, DownloadStatus::Downloading);
    }

    #[tokio::test]
    async fn test_mark_failed() {
        let pool = setup_test_db().await;
        let repo = SqliteDownloadStateRepository::new(pool);

        let download = QueuedDownload::new("fail-test", "org/model", "Model", 1, 1234567890);
        repo.enqueue(&download).await.unwrap();

        let id = DownloadId::from_model("fail-test");
        repo.mark_failed(&id, "Network error").await.unwrap();

        // Failed downloads are filtered out of load_queue
        let queue = repo.load_queue().await.unwrap();
        assert!(queue.is_empty());
    }

    #[tokio::test]
    async fn test_remove() {
        let pool = setup_test_db().await;
        let repo = SqliteDownloadStateRepository::new(pool);

        let download = QueuedDownload::new("remove-test", "org/model", "Model", 1, 1234567890);
        repo.enqueue(&download).await.unwrap();

        let id = DownloadId::from_model("remove-test");
        repo.remove(&id).await.unwrap();

        let queue = repo.load_queue().await.unwrap();
        assert!(queue.is_empty());
    }
}
