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

    /// Return the underlying settings repository.
    pub fn repo(&self) -> Arc<dyn SettingsRepository> {
        Arc::clone(&self.repo)
    }

    /// Get current settings.
    pub async fn get(&self) -> Result<Settings, CoreError> {
        self.repo.load().await.map_err(CoreError::from)
    }

    /// Update settings with partial changes.
    ///
    /// The merge and the validation run on the settings as they stand when
    /// the update is written, not as they stood at some earlier read: the
    /// repository reads, applies and stores in one step
    /// ([`SettingsRepository::modify`]). A field this update does not set is
    /// left as its last writer left it, whichever process that was.
    pub async fn update(&self, update: SettingsUpdate) -> Result<Settings, CoreError> {
        self.repo
            .modify(&|settings: &mut Settings| {
                settings.merge(&update);
                validate_settings(settings)
            })
            .await
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
        // Unset, not the floor: nothing has chosen a global default here.
        assert_eq!(settings.default_context_size, None);
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

    /// An update that fails validation stores nothing, not even the part of
    /// it that was valid.
    #[tokio::test]
    async fn an_update_that_fails_validation_stores_nothing() {
        let repo = Arc::new(MockSettingsRepo::new());
        let service = SettingsService::new(repo);

        let refused = service
            .update(SettingsUpdate {
                proxy_port: Some(Some(9191)),
                default_context_size: Some(Some(1)),
                ..Default::default()
            })
            .await;

        assert!(
            matches!(refused, Err(CoreError::Settings(_))),
            "{refused:?}"
        );
        assert_eq!(
            service.get().await.unwrap().proxy_port,
            Settings::with_defaults().proxy_port,
            "the valid half of a refused update was not stored"
        );
    }
}
