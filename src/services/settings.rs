//! Application settings management.
//!
//! This module provides persistent storage and retrieval of user-configurable
//! application settings. Settings are stored in the SQLite database and can be
//! accessed by both the GUI and CLI interfaces.

use anyhow::{Result, anyhow};
use serde::{Deserialize, Serialize};
use sqlx::{Row, SqlitePool};

/// Application settings structure
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Settings {
    /// Default directory for downloading models
    pub default_download_path: Option<String>,

    /// Default context size for models (e.g., 4096, 8192)
    pub default_context_size: Option<u64>,

    /// Port for the OpenAI-compatible proxy server
    pub proxy_port: Option<u16>,

    /// Base port for llama-server instances
    pub server_port: Option<u16>,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            default_download_path: None,
            default_context_size: Some(4096),
            proxy_port: Some(8080),
            server_port: Some(9000),
        }
    }
}

impl Settings {
    /// Create a new Settings instance with default values
    pub fn new() -> Self {
        Self::default()
    }
}

/// Initialize the settings table in the database
pub async fn init_settings_table(pool: &SqlitePool) -> Result<()> {
    sqlx::query(
        "CREATE TABLE IF NOT EXISTS settings (
            id INTEGER PRIMARY KEY CHECK (id = 1),
            default_download_path TEXT,
            default_context_size INTEGER,
            proxy_port INTEGER,
            server_port INTEGER,
            updated_at TEXT NOT NULL
        )",
    )
    .execute(pool)
    .await?;

    Ok(())
}

/// Get current settings from the database
pub async fn get_settings(pool: &SqlitePool) -> Result<Settings> {
    let row = sqlx::query(
        "SELECT default_download_path, default_context_size, proxy_port, server_port 
         FROM settings WHERE id = 1",
    )
    .fetch_optional(pool)
    .await?;

    match row {
        Some(row) => {
            let default_download_path: Option<String> = row.try_get(0)?;
            let default_context_size: Option<i64> = row.try_get(1)?;
            let proxy_port: Option<i64> = row.try_get(2)?;
            let server_port: Option<i64> = row.try_get(3)?;

            Ok(Settings {
                default_download_path,
                default_context_size: default_context_size.map(|v| v as u64),
                proxy_port: proxy_port.map(|v| v as u16),
                server_port: server_port.map(|v| v as u16),
            })
        }
        None => {
            // No settings exist yet, return defaults
            Ok(Settings::default())
        }
    }
}

/// Save settings to the database
pub async fn save_settings(pool: &SqlitePool, settings: &Settings) -> Result<()> {
    let updated_at = chrono::Utc::now().format("%Y-%m-%d %H:%M:%S").to_string();

    // Try to update first
    let rows_affected = sqlx::query(
        "UPDATE settings SET 
            default_download_path = ?,
            default_context_size = ?,
            proxy_port = ?,
            server_port = ?,
            updated_at = ?
         WHERE id = 1",
    )
    .bind(&settings.default_download_path)
    .bind(settings.default_context_size.map(|v| v as i64))
    .bind(settings.proxy_port.map(|v| v as i64))
    .bind(settings.server_port.map(|v| v as i64))
    .bind(&updated_at)
    .execute(pool)
    .await?
    .rows_affected();

    // If no rows were updated, insert a new row
    if rows_affected == 0 {
        sqlx::query(
            "INSERT INTO settings (id, default_download_path, default_context_size, proxy_port, server_port, updated_at)
             VALUES (1, ?, ?, ?, ?, ?)",
        )
        .bind(&settings.default_download_path)
        .bind(settings.default_context_size.map(|v| v as i64))
        .bind(settings.proxy_port.map(|v| v as i64))
        .bind(settings.server_port.map(|v| v as i64))
        .bind(&updated_at)
        .execute(pool)
        .await?;
    }

    Ok(())
}

/// Update specific settings fields (partial update)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SettingsUpdate {
    pub default_download_path: Option<Option<String>>,
    pub default_context_size: Option<Option<u64>>,
    pub proxy_port: Option<Option<u16>>,
    pub server_port: Option<Option<u16>>,
}

/// Update settings with partial changes
pub async fn update_settings(pool: &SqlitePool, update: SettingsUpdate) -> Result<Settings> {
    let mut current = get_settings(pool).await?;

    if let Some(path) = update.default_download_path {
        current.default_download_path = path;
    }
    if let Some(ctx_size) = update.default_context_size {
        current.default_context_size = ctx_size;
    }
    if let Some(port) = update.proxy_port {
        current.proxy_port = port;
    }
    if let Some(port) = update.server_port {
        current.server_port = port;
    }

    save_settings(pool, &current).await?;
    Ok(current)
}

/// Validate settings values
pub fn validate_settings(settings: &Settings) -> Result<()> {
    // Validate context size
    if let Some(ctx_size) = settings
        .default_context_size
        .filter(|&s| !(512..=1_000_000).contains(&s))
    {
        return Err(anyhow!(
            "Context size must be between 512 and 1,000,000, got {}",
            ctx_size
        ));
    }

    // Validate proxy port
    if let Some(port) = settings.proxy_port.filter(|&p| p < 1024) {
        return Err(anyhow!(
            "Proxy port should be >= 1024 (privileged ports require root), got {}",
            port
        ));
    }

    // Validate server port
    if let Some(port) = settings.server_port.filter(|&p| p < 1024) {
        return Err(anyhow!(
            "Server port should be >= 1024 (privileged ports require root), got {}",
            port
        ));
    }

    // Validate download path if specified
    if settings
        .default_download_path
        .as_ref()
        .is_some_and(|p| p.trim().is_empty())
    {
        return Err(anyhow!("Download path cannot be empty"));
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_settings() {
        let settings = Settings::default();
        assert_eq!(settings.default_context_size, Some(4096));
        assert_eq!(settings.proxy_port, Some(8080));
        assert_eq!(settings.server_port, Some(9000));
        assert_eq!(settings.default_download_path, None);
    }

    #[test]
    fn test_validate_settings_valid() {
        let settings = Settings::default();
        assert!(validate_settings(&settings).is_ok());
    }

    #[test]
    fn test_validate_context_size_too_small() {
        let settings = Settings {
            default_context_size: Some(100),
            ..Default::default()
        };
        assert!(validate_settings(&settings).is_err());
    }

    #[test]
    fn test_validate_context_size_too_large() {
        let settings = Settings {
            default_context_size: Some(2_000_000),
            ..Default::default()
        };
        assert!(validate_settings(&settings).is_err());
    }

    #[test]
    fn test_validate_port_too_low() {
        let settings = Settings {
            proxy_port: Some(80),
            ..Default::default()
        };
        assert!(validate_settings(&settings).is_err());
    }

    #[test]
    fn test_validate_empty_path() {
        let settings = Settings {
            default_download_path: Some("".to_string()),
            ..Default::default()
        };
        assert!(validate_settings(&settings).is_err());
    }

    #[tokio::test]
    async fn test_settings_persistence() {
        // Create an in-memory database for testing
        let pool = SqlitePool::connect(":memory:").await.unwrap();
        init_settings_table(&pool).await.unwrap();

        // Save settings
        let settings = Settings {
            default_download_path: Some("/path/to/models".to_string()),
            default_context_size: Some(8192),
            proxy_port: Some(8081),
            server_port: Some(9001),
        };
        save_settings(&pool, &settings).await.unwrap();

        // Retrieve and verify
        let retrieved = get_settings(&pool).await.unwrap();
        assert_eq!(
            retrieved.default_download_path,
            Some("/path/to/models".to_string())
        );
        assert_eq!(retrieved.default_context_size, Some(8192));
        assert_eq!(retrieved.proxy_port, Some(8081));
        assert_eq!(retrieved.server_port, Some(9001));
    }

    #[tokio::test]
    async fn test_partial_update() {
        let pool = SqlitePool::connect(":memory:").await.unwrap();
        init_settings_table(&pool).await.unwrap();

        // Save initial settings
        let initial = Settings::default();
        save_settings(&pool, &initial).await.unwrap();

        // Partial update
        let update = SettingsUpdate {
            default_context_size: Some(Some(16384)),
            default_download_path: None,
            proxy_port: None,
            server_port: None,
        };
        let updated = update_settings(&pool, update).await.unwrap();

        assert_eq!(updated.default_context_size, Some(16384));
        assert_eq!(updated.proxy_port, Some(8080)); // Unchanged
        assert_eq!(updated.server_port, Some(9000)); // Unchanged
    }
}
