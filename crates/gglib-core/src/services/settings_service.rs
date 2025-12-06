//! Settings service - orchestrates settings operations.

use crate::ports::{CoreError, SettingsRepository};
use crate::settings::{Settings, SettingsUpdate, validate_settings};
use std::sync::Arc;

/// Service for settings operations.
pub struct SettingsService {
    repo: Arc<dyn SettingsRepository>,
}

impl SettingsService {
    /// Create a new settings service.
    pub fn new(repo: Arc<dyn SettingsRepository>) -> Self {
        Self { repo }
    }

    /// Get current settings.
    pub async fn get(&self) -> Result<Settings, CoreError> {
        self.repo.load().await.map_err(CoreError::from)
    }

    /// Update settings with partial changes.
    pub async fn update(&self, update: SettingsUpdate) -> Result<Settings, CoreError> {
        let mut current = self.repo.load().await.map_err(CoreError::from)?;
        current.merge(&update);
        validate_settings(&current)?;
        self.repo.save(&current).await.map_err(CoreError::from)?;
        Ok(current)
    }

    /// Save complete settings (validates first).
    pub async fn save(&self, settings: &Settings) -> Result<(), CoreError> {
        validate_settings(settings)?;
        self.repo.save(settings).await.map_err(CoreError::from)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ports::RepositoryError;
    use async_trait::async_trait;
    use std::sync::Mutex;

    struct MockSettingsRepo {
        settings: Mutex<Settings>,
    }

    impl MockSettingsRepo {
        fn new() -> Self {
            Self {
                settings: Mutex::new(Settings::with_defaults()),
            }
        }
    }

    #[async_trait]
    impl SettingsRepository for MockSettingsRepo {
        async fn load(&self) -> Result<Settings, RepositoryError> {
            Ok(self.settings.lock().unwrap().clone())
        }

        async fn save(&self, settings: &Settings) -> Result<(), RepositoryError> {
            *self.settings.lock().unwrap() = settings.clone();
            Ok(())
        }
    }

    #[tokio::test]
    async fn test_get_default_settings() {
        let repo = Arc::new(MockSettingsRepo::new());
        let service = SettingsService::new(repo);

        let settings = service.get().await.unwrap();
        assert_eq!(settings.default_context_size, Some(4096));
    }

    #[tokio::test]
    async fn test_update_settings() {
        let repo = Arc::new(MockSettingsRepo::new());
        let service = SettingsService::new(repo);

        let update = SettingsUpdate {
            default_context_size: Some(Some(8192)),
            ..Default::default()
        };

        let updated = service.update(update).await.unwrap();
        assert_eq!(updated.default_context_size, Some(8192));

        // Verify persisted
        let fetched = service.get().await.unwrap();
        assert_eq!(fetched.default_context_size, Some(8192));
    }
}
