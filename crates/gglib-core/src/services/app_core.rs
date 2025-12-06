//! Application core facade.
//!
//! `AppCore` is the unified entry point for all business logic in gglib.
//! It accepts trait objects for repositories and process runner, allowing
//! it to be used with any backend implementation.

use std::sync::Arc;

use crate::ports::{ProcessRunner, Repos};
use crate::services::{ModelService, ServerService, SettingsService};

/// Unified application core providing access to all services.
///
/// `AppCore` is the central facade for all business logic. It holds
/// references to services that operate on port traits, making it
/// independent of any specific infrastructure implementation.
///
/// # Example
///
/// ```ignore
/// // In adapter bootstrap:
/// let repos = gglib_db::CoreFactory::build_repos(pool);
/// let runner = Arc::new(gglib_runtime::LlamaServerRunner::new(...));
/// let core = AppCore::new(repos, runner);
///
/// // Use services
/// let models = core.models().list().await?;
/// ```
pub struct AppCore {
    model_service: ModelService,
    server_service: ServerService,
    settings_service: SettingsService,
}

impl AppCore {
    /// Create a new AppCore with the given repositories and process runner.
    pub fn new(repos: Repos, runner: Arc<dyn ProcessRunner>) -> Self {
        let model_service = ModelService::new(Arc::clone(&repos.models));
        let server_service = ServerService::new(runner, Arc::clone(&repos.models));
        let settings_service = SettingsService::new(Arc::clone(&repos.settings));

        Self {
            model_service,
            server_service,
            settings_service,
        }
    }

    /// Access the model service for CRUD operations on models.
    pub fn models(&self) -> &ModelService {
        &self.model_service
    }

    /// Access the server service for managing llama-server instances.
    pub fn servers(&self) -> &ServerService {
        &self.server_service
    }

    /// Access the settings service for application configuration.
    pub fn settings(&self) -> &SettingsService {
        &self.settings_service
    }
}
