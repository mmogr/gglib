//! Application settings management.
//!
//! MIGRATION SHIM: Re-exports settings types from gglib-core and provides
//! database functions that wrap the new SqliteSettingsRepository.

use anyhow::Result;
use sqlx::SqlitePool;

// Re-export domain types from gglib-core
pub use gglib_core::{Settings, SettingsError, SettingsUpdate, validate_settings};

use gglib_db::SqliteSettingsRepository;
use gglib_core::SettingsRepository;

/// Initialize the settings table in the database.
///
/// Creates the settings_kv table if it doesn't exist.
pub async fn init_settings_table(pool: &SqlitePool) -> Result<()> {
    let repo = SqliteSettingsRepository::new(pool.clone());
    repo.ensure_table().await?;
    Ok(())
}

/// Get current settings from the database.
pub async fn get_settings(pool: &SqlitePool) -> Result<Settings> {
    let repo = SqliteSettingsRepository::new(pool.clone());
    let settings = repo.load().await?;
    Ok(settings)
}

/// Save settings to the database.
pub async fn save_settings(pool: &SqlitePool, settings: &Settings) -> Result<()> {
    let repo = SqliteSettingsRepository::new(pool.clone());
    repo.save(settings).await?;
    Ok(())
}

/// Update settings with partial changes.
pub async fn update_settings(pool: &SqlitePool, update: SettingsUpdate) -> Result<Settings> {
    let repo = SqliteSettingsRepository::new(pool.clone());
    let mut current = repo.load().await?;
    current.merge(&update);
    repo.save(&current).await?;
    Ok(current)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_settings() {
        let settings = Settings::with_defaults();
        assert_eq!(settings.default_context_size, Some(4096));
        assert_eq!(settings.proxy_port, Some(8080));
        assert_eq!(settings.server_port, Some(9000));
        assert_eq!(settings.default_download_path, None);
        assert_eq!(settings.max_download_queue_size, Some(10));
        assert_eq!(settings.show_memory_fit_indicators, Some(true));
    }

    #[test]
    fn test_validate_settings_valid() {
        let settings = Settings::with_defaults();
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
        let pool = SqlitePool::connect(":memory:").await.unwrap();
        init_settings_table(&pool).await.unwrap();

        let settings = Settings {
            default_download_path: Some("/path/to/models".to_string()),
            default_context_size: Some(8192),
            proxy_port: Some(8081),
            server_port: Some(9001),
            max_download_queue_size: Some(10),
            show_memory_fit_indicators: Some(false),
        };
        save_settings(&pool, &settings).await.unwrap();

        let retrieved = get_settings(&pool).await.unwrap();
        assert_eq!(
            retrieved.default_download_path,
            Some("/path/to/models".to_string())
        );
        assert_eq!(retrieved.default_context_size, Some(8192));
        assert_eq!(retrieved.proxy_port, Some(8081));
        assert_eq!(retrieved.server_port, Some(9001));
        assert_eq!(retrieved.show_memory_fit_indicators, Some(false));
    }

    #[tokio::test]
    async fn test_partial_update() {
        let pool = SqlitePool::connect(":memory:").await.unwrap();
        init_settings_table(&pool).await.unwrap();

        // Save initial settings
        let initial = Settings::with_defaults();
        save_settings(&pool, &initial).await.unwrap();

        // Partial update
        let update = SettingsUpdate {
            default_context_size: Some(Some(16384)),
            default_download_path: None,
            proxy_port: None,
            server_port: None,
            max_download_queue_size: None,
            show_memory_fit_indicators: None,
        };
        let updated = update_settings(&pool, update).await.unwrap();

        assert_eq!(updated.default_context_size, Some(16384));
        assert_eq!(updated.proxy_port, Some(8080)); // Unchanged
        assert_eq!(updated.server_port, Some(9000)); // Unchanged
        assert_eq!(updated.max_download_queue_size, Some(10)); // Unchanged
        assert_eq!(updated.show_memory_fit_indicators, Some(true)); // Unchanged
    }
}
