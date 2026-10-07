//! A [`SettingsRepository`] held in memory, for tests.

use std::sync::{Mutex, PoisonError};

use async_trait::async_trait;

use super::{RepositoryError, SettingsRepository};
use crate::settings::Settings;

/// Settings kept in memory: a load answers what was last saved.
///
/// The double for a test that only needs settings to be there. One that
/// counts calls, refuses a write or parks a read keeps a double of its own.
#[derive(Debug)]
pub struct InMemorySettings(Mutex<Settings>);

impl InMemorySettings {
    /// A store holding `settings`.
    #[must_use]
    pub const fn with(settings: Settings) -> Self {
        Self(Mutex::new(settings))
    }
}

impl Default for InMemorySettings {
    /// A store holding [`Settings::with_defaults`].
    fn default() -> Self {
        Self::with(Settings::with_defaults())
    }
}

#[async_trait]
impl SettingsRepository for InMemorySettings {
    async fn load(&self) -> Result<Settings, RepositoryError> {
        Ok(self
            .0
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .clone())
    }

    async fn save(&self, settings: &Settings) -> Result<(), RepositoryError> {
        *self.0.lock().unwrap_or_else(PoisonError::into_inner) = settings.clone();
        Ok(())
    }
}
