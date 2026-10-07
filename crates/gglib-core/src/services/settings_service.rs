//! Settings service - orchestrates settings operations.

use crate::domain::builtin_templates;
use crate::ports::{CoreError, SettingsRepository};
use crate::settings::{Settings, SettingsUpdate, validate_settings};
use std::sync::{Arc, Mutex, PoisonError};

/// What installing the starter profiles did, by profile name.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct TemplateInstall {
    /// The templates stored: added, or put in place of a profile with `force`.
    pub installed: Vec<String>,
    /// The templates left out because a profile already had the name.
    pub kept: Vec<String>,
}

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

    /// Put every preference back to its default, keeping what
    /// [`Settings::reset_preferences`] keeps.
    ///
    /// One step in the store, as [`Self::update`] is: what a reset keeps is
    /// what is stored when it writes, not what an earlier read saw.
    pub async fn reset_preferences(&self) -> Result<Settings, CoreError> {
        self.repo
            .modify(&|settings: &mut Settings| {
                settings.reset_preferences();
                Ok(())
            })
            .await
    }

    /// Add the starter profiles ([`builtin_templates`]) to the stored list,
    /// and return the settings as stored with what was done.
    ///
    /// The one install, for `gglib config profile install-templates` and the
    /// settings page alike. A stored profile that has a template's name is
    /// kept as it is; with `force` the template takes its place. One step in
    /// the store, as [`Self::update`] is.
    pub async fn install_profile_templates(
        &self,
        force: bool,
    ) -> Result<(Settings, TemplateInstall), CoreError> {
        let outcome = Mutex::new(TemplateInstall::default());
        let settings = self
            .repo
            .modify(&|settings: &mut Settings| {
                let mut done = TemplateInstall::default();
                let profiles = settings.inference_profiles.get_or_insert_default();
                for template in builtin_templates() {
                    match profiles.iter().position(|p| p.name == template.name) {
                        Some(_) if !force => done.kept.push(template.name),
                        stored => {
                            done.installed.push(template.name.clone());
                            match stored {
                                Some(index) => profiles[index] = template,
                                None => profiles.push(template),
                            }
                        }
                    }
                }
                *outcome.lock().unwrap_or_else(PoisonError::into_inner) = done;
                validate_settings(settings)
            })
            .await?;
        let outcome = outcome.into_inner().unwrap_or_else(PoisonError::into_inner);
        Ok((settings, outcome))
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

    fn profile_names(settings: &Settings) -> Vec<String> {
        let profiles = settings.inference_profiles.iter().flatten();
        profiles.map(|p| p.name.clone()).collect()
    }

    /// The nine: three for sampling, six for reasoning effort.
    const TEMPLATES: [&str; 9] = [
        "coding", "chat", "creative", "minimal", "low", "medium", "high", "xhigh", "max",
    ];

    #[tokio::test]
    async fn installing_the_templates_stores_all_nine() {
        let service = SettingsService::new(Arc::new(MockSettingsRepo::new()));

        let (stored, done) = service.install_profile_templates(false).await.unwrap();

        assert_eq!(done.installed, TEMPLATES);
        assert_eq!(done.kept, Vec::<String>::new());
        assert_eq!(profile_names(&stored), TEMPLATES);
        assert_eq!(profile_names(&service.get().await.unwrap()), TEMPLATES);
        assert_eq!(stored.inference_profiles, Some(builtin_templates()));
    }

    /// A profile that already has a template's name is the user's: it is
    /// kept, where it is in the list, and the rest are added after it.
    #[tokio::test]
    async fn a_profile_with_a_templates_name_is_kept() {
        let service = SettingsService::new(Arc::new(MockSettingsRepo::new()));
        let mut mine = builtin_templates().remove(1);
        mine.config.temperature = Some(0.123);
        let update = SettingsUpdate {
            inference_profiles: Some(Some(vec![mine.clone()])),
            ..Default::default()
        };
        service.update(update).await.unwrap();

        let (stored, done) = service.install_profile_templates(false).await.unwrap();

        assert_eq!(done.kept, ["chat"]);
        assert_eq!(done.installed.len(), 8, "{done:?}");
        let profiles = stored.inference_profiles.unwrap();
        assert_eq!(profiles[0], mine, "the stored chat profile was changed");
        assert_eq!(profiles.len(), 9);

        let (_, again) = service.install_profile_templates(false).await.unwrap();
        assert_eq!(again.installed, Vec::<String>::new());
        assert_eq!(again.kept, TEMPLATES);
    }

    /// With `force` the template takes the stored profile's place.
    #[tokio::test]
    async fn force_puts_the_template_in_the_stored_profiles_place() {
        let service = SettingsService::new(Arc::new(MockSettingsRepo::new()));
        let mut mine = builtin_templates().remove(1);
        mine.config.temperature = Some(0.123);
        let update = SettingsUpdate {
            inference_profiles: Some(Some(vec![mine])),
            ..Default::default()
        };
        service.update(update).await.unwrap();

        let (stored, done) = service.install_profile_templates(true).await.unwrap();

        assert_eq!(done.installed, TEMPLATES);
        assert_eq!(done.kept, Vec::<String>::new());
        let profiles = stored.inference_profiles.unwrap();
        assert_eq!(profiles[0], builtin_templates()[1], "chat is the template");
        assert_eq!(profiles.len(), 9);
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
