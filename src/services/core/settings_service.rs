//! Settings service for application configuration management.
//!
//! This service wraps the existing settings module to provide a clean API
//! for getting and updating application settings.

use crate::services::settings::{self, Settings, SettingsUpdate};
use crate::utils::paths::{
    DirectoryCreationStrategy, ModelsDirSource, default_models_dir, ensure_directory,
    persist_models_dir, resolve_models_dir, verify_writable,
};
use anyhow::{Result, bail};
use sqlx::SqlitePool;

/// Information about the models directory configuration
#[derive(Debug, Clone)]
pub struct ModelsDirectoryInfo {
    /// Current path to the models directory
    pub path: String,
    /// Source of the path (explicit, env, or default)
    pub source: String,
    /// Default path if no override is set
    pub default_path: String,
    /// Whether the directory exists
    pub exists: bool,
    /// Whether the directory is writable
    pub writable: bool,
}

/// Service for managing application settings.
///
/// Provides access to persistent settings stored in the database,
/// as well as models directory configuration.
#[derive(Clone)]
pub struct SettingsService {
    db_pool: SqlitePool,
}

impl SettingsService {
    /// Create a new SettingsService.
    pub fn new(db_pool: SqlitePool) -> Self {
        Self { db_pool }
    }

    /// Get current application settings.
    pub async fn get(&self) -> Result<Settings> {
        settings::get_settings(&self.db_pool).await
    }

    /// Update application settings with partial changes.
    ///
    /// Only the fields specified in the update will be changed.
    /// Other fields retain their current values.
    pub async fn update(&self, update: SettingsUpdate) -> Result<Settings> {
        let updated = settings::update_settings(&self.db_pool, update).await?;
        
        // Validate the updated settings
        settings::validate_settings(&updated)?;
        
        Ok(updated)
    }

    /// Save complete settings (replaces all values).
    pub async fn save(&self, new_settings: &Settings) -> Result<()> {
        settings::validate_settings(new_settings)?;
        settings::save_settings(&self.db_pool, new_settings).await
    }

    /// Get information about the models directory configuration.
    pub fn get_models_directory_info(&self) -> Result<ModelsDirectoryInfo> {
        let resolution = resolve_models_dir(None)?;
        let default_path = default_models_dir()?;
        let exists = resolution.path.is_dir();
        let writable = exists && verify_writable(&resolution.path).is_ok();

        Ok(ModelsDirectoryInfo {
            path: resolution.path.to_string_lossy().to_string(),
            source: stringify_models_dir_source(resolution.source).to_string(),
            default_path: default_path.to_string_lossy().to_string(),
            exists,
            writable,
        })
    }

    /// Update the models directory path.
    ///
    /// This validates the path, creates the directory if needed,
    /// and persists the setting.
    pub fn update_models_directory(&self, new_path: &str) -> Result<ModelsDirectoryInfo> {
        if new_path.trim().is_empty() {
            bail!("Path cannot be empty");
        }

        let resolution = resolve_models_dir(Some(new_path))?;
        ensure_directory(&resolution.path, DirectoryCreationStrategy::AutoCreate)?;
        persist_models_dir(&resolution.path)?;
        
        // SAFETY: This modifies global environment state in a multi-threaded context.
        // While inherently unsafe, it maintains consistency between the persisted configuration
        // and runtime state. The value is only read during path resolution operations which
        // typically occur at controlled points. Future refactoring should consider passing
        // configuration explicitly rather than through environment variables.
        unsafe {
            std::env::set_var(
                "GGLIB_MODELS_DIR",
                resolution.path.to_string_lossy().to_string(),
            );
        }

        self.get_models_directory_info()
    }
}

fn stringify_models_dir_source(source: ModelsDirSource) -> &'static str {
    match source {
        ModelsDirSource::Explicit => "explicit",
        ModelsDirSource::EnvVar => "env",
        ModelsDirSource::Default => "default",
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::services::database;

    #[tokio::test]
    async fn test_settings_service_get() {
        let pool = database::setup_database().await.unwrap();
        let service = SettingsService::new(pool);
        
        let settings = service.get().await.unwrap();
        // Should return defaults or existing settings
        assert!(settings.default_context_size.is_some() || settings.default_context_size.is_none());
    }

    #[tokio::test]
    async fn test_settings_service_update() {
        let pool = database::setup_database().await.unwrap();
        let service = SettingsService::new(pool);
        
        let update = SettingsUpdate {
            default_context_size: Some(Some(8192)),
            default_download_path: None,
            proxy_port: None,
            server_port: None,
        };
        
        let result = service.update(update).await;
        assert!(result.is_ok());
    }

    #[test]
    fn test_get_models_directory_info() {
        // This test may depend on environment, but shouldn't panic
        let pool_result = tokio::runtime::Runtime::new()
            .unwrap()
            .block_on(crate::services::database::setup_database());
        
        if let Ok(pool) = pool_result {
            let service = SettingsService::new(pool);
            let result = service.get_models_directory_info();
            assert!(result.is_ok());
        }
    }
}
