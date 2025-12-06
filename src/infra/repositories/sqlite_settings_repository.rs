//! SQLite implementation of the SettingsRepository trait.
//!
//! This implementation provides a simple key-value store for settings.
//! It uses a separate table from the typed settings to allow gradual migration.

use async_trait::async_trait;
use sqlx::{Row, SqlitePool};

use crate::core::ports::RepositoryError;
use crate::core::ports::settings_repository::SettingsRepository;

/// SQLite implementation of the SettingsRepository trait.
///
/// This uses a key-value table structure for flexible settings storage.
pub struct SqliteSettingsRepository {
    pool: SqlitePool,
}

impl SqliteSettingsRepository {
    /// Create a new SQLite settings repository.
    pub fn new(pool: SqlitePool) -> Self {
        Self { pool }
    }

    /// Ensure the key-value settings table exists.
    ///
    /// Call this during initialization to set up the schema.
    pub async fn ensure_table(&self) -> Result<(), RepositoryError> {
        sqlx::query(
            r#"
            CREATE TABLE IF NOT EXISTS settings_kv (
                key TEXT PRIMARY KEY NOT NULL,
                value TEXT NOT NULL,
                updated_at TEXT NOT NULL
            )
            "#,
        )
        .execute(&self.pool)
        .await
        .map_err(|e| RepositoryError::Storage(e.to_string()))?;

        Ok(())
    }

    /// Get a reference to the underlying pool (for testing/migration only).
    #[cfg(test)]
    pub fn pool(&self) -> &SqlitePool {
        &self.pool
    }
}

#[async_trait]
impl SettingsRepository for SqliteSettingsRepository {
    async fn get(&self, key: &str) -> Result<Option<String>, RepositoryError> {
        let row = sqlx::query("SELECT value FROM settings_kv WHERE key = ?")
            .bind(key)
            .fetch_optional(&self.pool)
            .await
            .map_err(|e| RepositoryError::Storage(e.to_string()))?;

        Ok(row.map(|r| r.get("value")))
    }

    async fn set(&self, key: &str, value: &str) -> Result<(), RepositoryError> {
        let updated_at = chrono::Utc::now().format("%Y-%m-%d %H:%M:%S").to_string();

        // Use upsert pattern (INSERT OR REPLACE)
        sqlx::query("INSERT OR REPLACE INTO settings_kv (key, value, updated_at) VALUES (?, ?, ?)")
            .bind(key)
            .bind(value)
            .bind(&updated_at)
            .execute(&self.pool)
            .await
            .map_err(|e| RepositoryError::Storage(e.to_string()))?;

        Ok(())
    }

    async fn delete(&self, key: &str) -> Result<(), RepositoryError> {
        // Idempotent delete - don't error if key doesn't exist
        sqlx::query("DELETE FROM settings_kv WHERE key = ?")
            .bind(key)
            .execute(&self.pool)
            .await
            .map_err(|e| RepositoryError::Storage(e.to_string()))?;

        Ok(())
    }

    async fn list(&self) -> Result<Vec<(String, String)>, RepositoryError> {
        let rows = sqlx::query("SELECT key, value FROM settings_kv ORDER BY key")
            .fetch_all(&self.pool)
            .await
            .map_err(|e| RepositoryError::Storage(e.to_string()))?;

        Ok(rows
            .iter()
            .map(|r| (r.get("key"), r.get("value")))
            .collect())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    async fn create_test_repo() -> SqliteSettingsRepository {
        let pool = SqlitePool::connect("sqlite::memory:").await.unwrap();
        let repo = SqliteSettingsRepository::new(pool);
        repo.ensure_table().await.unwrap();
        repo
    }

    #[tokio::test]
    async fn test_set_and_get() {
        let repo = create_test_repo().await;

        repo.set("test_key", "test_value").await.unwrap();
        let value = repo.get("test_key").await.unwrap();

        assert_eq!(value, Some("test_value".to_string()));
    }

    #[tokio::test]
    async fn test_get_nonexistent() {
        let repo = create_test_repo().await;

        let value = repo.get("nonexistent").await.unwrap();
        assert_eq!(value, None);
    }

    #[tokio::test]
    async fn test_upsert() {
        let repo = create_test_repo().await;

        repo.set("key", "value1").await.unwrap();
        repo.set("key", "value2").await.unwrap();

        let value = repo.get("key").await.unwrap();
        assert_eq!(value, Some("value2".to_string()));
    }

    #[tokio::test]
    async fn test_delete() {
        let repo = create_test_repo().await;

        repo.set("key", "value").await.unwrap();
        repo.delete("key").await.unwrap();

        let value = repo.get("key").await.unwrap();
        assert_eq!(value, None);
    }

    #[tokio::test]
    async fn test_delete_nonexistent_is_ok() {
        let repo = create_test_repo().await;

        // Should not error even if key doesn't exist
        let result = repo.delete("nonexistent").await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_list() {
        let repo = create_test_repo().await;

        repo.set("key1", "value1").await.unwrap();
        repo.set("key2", "value2").await.unwrap();
        repo.set("key3", "value3").await.unwrap();

        let settings = repo.list().await.unwrap();

        assert_eq!(settings.len(), 3);
        assert!(settings.contains(&("key1".to_string(), "value1".to_string())));
        assert!(settings.contains(&("key2".to_string(), "value2".to_string())));
        assert!(settings.contains(&("key3".to_string(), "value3".to_string())));
    }
}
