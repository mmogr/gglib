//! Mock ports for `{model}:{profile}` routing tests.
//!
//! Richer than the single-profile `settings_listing` in [`super::common`]:
//! these tests need several profiles at once and a toggleable
//! `trust_client_sampling`, because the questions they ask are about which
//! rung of the sampling ladder won.

use std::sync::{Arc, Mutex};

use async_trait::async_trait;

use gglib_core::Settings;
use gglib_core::domain::{InferenceConfig, InferenceProfile};
use gglib_core::ports::{
    CatalogError, InMemorySettings, ModelCatalogPort, ModelLaunchSpec, ModelRuntimeError,
    ModelRuntimePort, ModelSummary, RunningTarget,
};

pub(crate) const MODEL: &str = "qwen";

// ─── Mock ports ────────────────────────────────────────────────────────────

/// Runtime that always reports the mock upstream as running, and records the
/// name of the model it was asked to launch.
///
/// The proxy admits by catalog id, so the id is read back to a name through
/// `names`, numbered from 1 as [`NamedCatalog`] numbers them.
#[derive(Debug)]
pub(crate) struct RecordingRuntime {
    pub(crate) port: u16,
    pub(crate) names: Vec<String>,
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
        let (id, name) = (1..)
            .zip(&self.names)
            .find(|(id, name)| id.to_string() == model_name || *name == model_name)
            .ok_or_else(|| ModelRuntimeError::ModelNotFound(model_name.to_owned()))?;
        self.launched.lock().unwrap().push(name.clone());
        Ok(gglib_core::ports::Admission::detached(
            RunningTarget::local(self.port, id, name.clone(), 4096, false),
        ))
    }
    async fn current_model(&self) -> Option<RunningTarget> {
        None
    }
    async fn stop_current(&self) -> Result<(), ModelRuntimeError> {
        Ok(())
    }
}

/// Catalog over an explicit set of names, numbered from 1 in order, found by
/// id or by exact name.
#[derive(Debug)]
pub(crate) struct NamedCatalog {
    pub(crate) names: Vec<String>,
    /// Per-model stored defaults, returned for every resolved model.
    pub(crate) inference_defaults: Option<InferenceConfig>,
}

impl NamedCatalog {
    fn summary(&self, id: u32, name: &str) -> ModelSummary {
        ModelSummary {
            inference_defaults: self.inference_defaults.clone(),
            ..ModelSummary::bare(id, name)
        }
    }
}

#[async_trait]
impl ModelCatalogPort for NamedCatalog {
    async fn list_models(&self) -> Result<Vec<ModelSummary>, CatalogError> {
        Ok((1..)
            .zip(&self.names)
            .map(|(id, n)| self.summary(id, n))
            .collect())
    }
    async fn resolve_model(&self, name: &str) -> Result<Option<ModelSummary>, CatalogError> {
        Ok((1..)
            .zip(&self.names)
            .find(|(id, n)| id.to_string() == name || *n == name)
            .map(|(id, n)| self.summary(id, n)))
    }
    async fn resolve_for_launch(
        &self,
        _name: &str,
    ) -> Result<Option<ModelLaunchSpec>, CatalogError> {
        Ok(None)
    }
}

/// Settings serving a fixed profile list.
pub(crate) fn profile_settings(
    profiles: Vec<InferenceProfile>,
    trust_client_sampling: bool,
) -> InMemorySettings {
    InMemorySettings::with(Settings {
        inference_profiles: Some(profiles),
        trust_client_sampling: Some(trust_client_sampling),
        ..Settings::with_defaults()
    })
}
