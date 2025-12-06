//! Settings service for application configuration.
//!
//! This service provides settings operations using the `SettingsRepository`
//! port trait.

use std::sync::Arc;

use crate::ports::{CoreError, SettingsRepository};
use crate::settings::{Settings, SettingsUpdate, validate_settings};

/// Service for managing application settings.
#[derive(Clone)]
pub struct SettingsService {
    repo: Arc<dyn SettingsRepository>,
}

impl SettingsService {
    /// Create a new SettingsService with the given repository.
    pub fn new(repo: Arc<dyn SettingsRepository>) -> Self {
        Self { repo }
    }

    /// Get current settings.
    pub async fn get(&self) -> Result<Settings, CoreError> {
        self.repo.load().await.map_err(CoreError::Repository)
    }

    /// Update settings by applying a partial update.
    ///
    /// Loads current settings, applies the update, then saves the result.
    pub async fn update(&self, update: SettingsUpdate) -> Result<Settings, CoreError> {
        let mut settings = self.repo.load().await.map_err(CoreError::Repository)?;
        settings.merge(&update);
        validate_settings(&settings).map_err(CoreError::Settings)?;
        self.repo.save(&settings).await.map_err(CoreError::Repository)?;
        Ok(settings)
    }

    /// Save complete settings (replaces all values).
    pub async fn save(&self, settings: &Settings) -> Result<(), CoreError> {
        validate_settings(settings).map_err(CoreError::Settings)?;
        self.repo.save(settings).await.map_err(CoreError::Repository)
    }
}
