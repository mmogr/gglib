//! `SQLite` implementation of the `SettingsRepository` trait.

use async_trait::async_trait;
use sqlx::{Row, SqlitePool};

use gglib_core::{RepositoryError, Settings, SettingsRepository};

/// `SQLite` implementation of the `SettingsRepository` trait.
///
/// Stores settings as a JSON blob in a key-value table for flexibility.
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

const SETTINGS_KEY: &str = "app_settings";

#[async_trait]
impl SettingsRepository for SqliteSettingsRepository {
    async fn load(&self) -> Result<Settings, RepositoryError> {
        let row = sqlx::query("SELECT value FROM settings_kv WHERE key = ?")
            .bind(SETTINGS_KEY)
            .fetch_optional(&self.pool)
            .await
            .map_err(|e| RepositoryError::Storage(e.to_string()))?;

        match row {
            Some(r) => {
                let json: String = r.get("value");
                serde_json::from_str(&json).map_err(|e| RepositoryError::Storage(e.to_string()))
            }
            None => Ok(Settings::with_defaults()),
        }
    }

    async fn save(&self, settings: &Settings) -> Result<(), RepositoryError> {
        let json =
            serde_json::to_string(settings).map_err(|e| RepositoryError::Storage(e.to_string()))?;
        let updated_at = chrono::Utc::now().format("%Y-%m-%d %H:%M:%S").to_string();

        sqlx::query("INSERT OR REPLACE INTO settings_kv (key, value, updated_at) VALUES (?, ?, ?)")
            .bind(SETTINGS_KEY)
            .bind(&json)
            .bind(&updated_at)
            .execute(&self.pool)
            .await
            .map_err(|e| RepositoryError::Storage(e.to_string()))?;

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_load_returns_defaults_when_empty() {
        let pool = SqlitePool::connect("sqlite::memory:").await.unwrap();
        let repo = SqliteSettingsRepository::new(pool);
        repo.ensure_table().await.unwrap();

        let settings = repo.load().await.unwrap();
        assert_eq!(settings, Settings::with_defaults());
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
}
