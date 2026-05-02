//! `SQLite` implementation of the `SettingsRepository` trait.

use async_trait::async_trait;
use serde_json::{Map, Value};
use sqlx::{Row, SqlitePool};

use gglib_core::{RepositoryError, Settings, SettingsRepository};

/// `SQLite` implementation of the `SettingsRepository` trait.
///
/// Stores each setting as an individual row in the key-value table, using the
/// `serde` field name as the key and a compact JSON encoding as the value.
/// `None`-valued fields are not stored; an absent row means "use default".
pub struct SqliteSettingsRepository {
    pool: SqlitePool,
}

impl SqliteSettingsRepository {
    /// Create a new `SQLite` settings repository.
    pub fn new(pool: SqlitePool) -> Self {
        Self { pool }
    }

    /// Ensure the settings table exists.
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
}

#[async_trait]
impl SettingsRepository for SqliteSettingsRepository {
    async fn load(&self) -> Result<Settings, RepositoryError> {
        let rows = sqlx::query("SELECT key, value FROM settings_kv")
            .fetch_all(&self.pool)
            .await
            .map_err(|e| RepositoryError::Storage(e.to_string()))?;

        let mut map = Map::new();
        for row in rows {
            let key: String = row.get("key");
            let raw: String = row.get("value");
            let val: Value =
                serde_json::from_str(&raw).map_err(|e| RepositoryError::Storage(e.to_string()))?;
            map.insert(key, val);
        }

        serde_json::from_value(Value::Object(map))
            .map_err(|e| RepositoryError::Storage(e.to_string()))
    }

    async fn save(&self, settings: &Settings) -> Result<(), RepositoryError> {
        let map = match serde_json::to_value(settings)
            .map_err(|e| RepositoryError::Storage(e.to_string()))?
        {
            Value::Object(m) => m,
            other => {
                return Err(RepositoryError::Storage(format!(
                    "expected object, got {other}"
                )));
            }
        };

        let updated_at = chrono::Utc::now().format("%Y-%m-%d %H:%M:%S").to_string();

        let mut tx = self
            .pool
            .begin()
            .await
            .map_err(|e| RepositoryError::Storage(e.to_string()))?;

        for (key, value) in &map {
            if value.is_null() {
                sqlx::query("DELETE FROM settings_kv WHERE key = ?")
                    .bind(key)
                    .execute(&mut *tx)
                    .await
                    .map_err(|e| RepositoryError::Storage(e.to_string()))?;
            } else {
                sqlx::query(
                    "INSERT OR REPLACE INTO settings_kv (key, value, updated_at) VALUES (?, ?, ?)",
                )
                .bind(key)
                .bind(value.to_string())
                .bind(&updated_at)
                .execute(&mut *tx)
                .await
                .map_err(|e| RepositoryError::Storage(e.to_string()))?;
            }
        }

        tx.commit()
            .await
            .map_err(|e| RepositoryError::Storage(e.to_string()))?;

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_load_returns_all_none_when_empty() {
        let pool = SqlitePool::connect("sqlite::memory:").await.unwrap();
        let repo = SqliteSettingsRepository::new(pool);
        repo.ensure_table().await.unwrap();

        // Empty table → all fields are None; application layer supplies defaults.
        let settings = repo.load().await.unwrap();
        assert_eq!(settings, Settings::default());
    }

    #[tokio::test]
    async fn test_save_and_load() {
        let pool = SqlitePool::connect("sqlite::memory:").await.unwrap();
        let repo = SqliteSettingsRepository::new(pool);
        repo.ensure_table().await.unwrap();

        let settings = Settings {
            default_context_size: Some(8192),
            proxy_port: Some(9090),
            ..Settings::with_defaults()
        };

        repo.save(&settings).await.unwrap();
        let loaded = repo.load().await.unwrap();

        assert_eq!(loaded.default_context_size, Some(8192));
        assert_eq!(loaded.proxy_port, Some(9090));
    }

    #[tokio::test]
    async fn test_none_fields_are_deleted_from_db() {
        let pool = SqlitePool::connect("sqlite::memory:").await.unwrap();
        let repo = SqliteSettingsRepository::new(pool.clone());
        repo.ensure_table().await.unwrap();

        // Save with a Some value — should create a row for proxy_port.
        let mut settings = Settings {
            proxy_port: Some(9090),
            ..Settings::default()
        };
        repo.save(&settings).await.unwrap();

        let row: Option<(String,)> =
            sqlx::query_as("SELECT value FROM settings_kv WHERE key = 'proxy_port'")
                .fetch_optional(&pool)
                .await
                .unwrap();
        assert!(
            row.is_some(),
            "proxy_port row should exist after saving Some"
        );

        // Now set that field to None and save again — row should be gone.
        settings.proxy_port = None;
        repo.save(&settings).await.unwrap();

        let row: Option<(String,)> =
            sqlx::query_as("SELECT value FROM settings_kv WHERE key = 'proxy_port'")
                .fetch_optional(&pool)
                .await
                .unwrap();
        assert!(
            row.is_none(),
            "proxy_port row should be deleted after saving None"
        );
    }
}
