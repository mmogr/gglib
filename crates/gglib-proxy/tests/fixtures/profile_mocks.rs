//! Mock ports for `{model}:{profile}` routing tests.
//!
//! Richer than the single-profile `ProfileSettingsRepo` in [`super::common`]:
//! these tests need several profiles at once and a toggleable
//! `trust_client_sampling`, because the questions they ask are about which
//! rung of the sampling ladder won.

use std::sync::{Arc, Mutex};

use async_trait::async_trait;

use gglib_core::Settings;
use gglib_core::domain::{InferenceConfig, InferenceProfile};
use gglib_core::ports::{
    CatalogError, ModelCatalogPort, ModelLaunchSpec, ModelRuntimeError, ModelRuntimePort,
    ModelSummary, RepositoryError, RunningTarget, SettingsRepository,
};

pub(crate) const MODEL: &str = "qwen";

// ─── Mock ports ────────────────────────────────────────────────────────────

/// Runtime that always reports the mock upstream as running, and records the
/// model name it was asked to launch.
#[derive(Debug)]
pub(crate) struct RecordingRuntime {
    pub(crate) port: u16,
    pub(crate) launched: Arc<Mutex<Vec<String>>>,
}

#[async_trait]
impl ModelRuntimePort for RecordingRuntime {
    async fn admit(
        &self,
        model_name: &str,
        _num_ctx: Option<u64>,
        _default_ctx: Option<u64>,
        _overrides: gglib_core::ports::LaunchOverrides,
    ) -> Result<gglib_core::ports::Admission, ModelRuntimeError> {
        self.launched.lock().unwrap().push(model_name.to_owned());
        Ok(gglib_core::ports::Admission::detached(
            RunningTarget::local(self.port, 1, model_name.to_owned(), 4096, false),
        ))
    }
    async fn current_model(&self) -> Option<RunningTarget> {
        None
    }
    async fn stop_current(&self) -> Result<(), ModelRuntimeError> {
        Ok(())
    }
}

/// Catalog resolving an explicit set of names by exact match.
#[derive(Debug)]
pub(crate) struct NamedCatalog {
    pub(crate) names: Vec<String>,
    /// Per-model stored defaults, returned for every resolved model.
    pub(crate) inference_defaults: Option<InferenceConfig>,
}

impl NamedCatalog {
    fn summary(&self, name: &str) -> ModelSummary {
        ModelSummary {
            dialect: None,
            template_caps: None,
            id: 1,
            name: name.to_owned(),
            tags: Vec::new(),
            capabilities: gglib_core::domain::ModelCapabilities::empty(),
            param_count: "7B".into(),
            quantization: None,
            architecture: None,
            created_at: 0,
            file_size: 0,
            context_length: None,
            inference_defaults: self.inference_defaults.clone(),
            defaults_origin: None,
            server_defaults: None,
        }
    }
}

#[async_trait]
impl ModelCatalogPort for NamedCatalog {
    async fn list_models(&self) -> Result<Vec<ModelSummary>, CatalogError> {
        Ok(self.names.iter().map(|n| self.summary(n)).collect())
    }
    async fn resolve_model(&self, name: &str) -> Result<Option<ModelSummary>, CatalogError> {
        Ok(self
            .names
            .iter()
            .any(|n| n == name)
            .then(|| self.summary(name)))
    }
    async fn resolve_for_launch(
        &self,
        _name: &str,
    ) -> Result<Option<ModelLaunchSpec>, CatalogError> {
        Ok(None)
    }
}

/// Settings repository serving a fixed profile list.
pub(crate) struct ProfileSettings {
    pub(crate) profiles: Vec<InferenceProfile>,
    pub(crate) trust_client_sampling: bool,
}

#[async_trait]
impl SettingsRepository for ProfileSettings {
    async fn load(&self) -> Result<Settings, RepositoryError> {
        Ok(Settings {
            inference_profiles: Some(self.profiles.clone()),
            trust_client_sampling: Some(self.trust_client_sampling),
            ..Settings::with_defaults()
        })
    }
    async fn save(&self, _: &Settings) -> Result<(), RepositoryError> {
        Ok(())
    }
}
