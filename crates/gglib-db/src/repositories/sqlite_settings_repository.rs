//! SQLite implementation of the SettingsRepository trait.

use async_trait::async_trait;
use sqlx::{Row, SqlitePool};

use gglib_core::{RepositoryError, SettingsRepository};

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
