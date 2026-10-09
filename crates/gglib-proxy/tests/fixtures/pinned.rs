//! A catalog of fixed models and the runtimes `gglib serve` pins over it.
//!
//! [`PinnedRuntime`] only reports its pin, so a catalog test can tell "not
//! advertised" from "refused". [`EnforcingPinnedRuntime`] enforces it the way
//! `gglib-runtime` does — resolve the admitted id or name, then compare ids —
//! so the wire contract a client hits can be asserted over HTTP.

use std::collections::HashMap;
use std::path::PathBuf;
use std::time::UNIX_EPOCH;

use async_trait::async_trait;

use gglib_core::domain::{ComponentRole, ImageFamily, Model, ModelComponent};
use gglib_core::ports::{
    Admission, CatalogError, LaunchOverrides, ModelCatalogPort, ModelLaunchSpec, ModelRuntimeError,
    ModelRuntimePort, ModelSummary, PinnedSpec, RunningTarget,
};
use gglib_core::request_pipeline::resolve_summary;

/// A pin on model `id` named `name`, with no launch overrides of its own.
pub(crate) fn pin(id: u32, name: &str) -> PinnedSpec {
    PinnedSpec {
        id: i64::from(id),
        name: name.to_owned(),
        ..PinnedSpec::default()
    }
}

/// Catalog port over a fixed set of models, each with its catalog id.
///
/// Found as the real catalog finds them: a string that parses as a number is
/// an id first, then an exact name. Names and ids are all `/v1/models`
/// filtering, admission and the detail read care about, so everything else is
/// filled with plausible constants rather than made configurable.
#[derive(Debug, Clone)]
pub(crate) struct StaticCatalog(pub Vec<(u32, String)>);

impl StaticCatalog {
    /// A catalog of `names`, numbered from 1 in the order given.
    pub(crate) fn new(names: &[&str]) -> Self {
        Self::numbered(
            &names
                .iter()
                .zip(1..)
                .map(|(name, id)| (id, *name))
                .collect::<Vec<_>>(),
        )
    }

    /// A catalog of these models under these ids.
    pub(crate) fn numbered(models: &[(u32, &str)]) -> Self {
        Self(
            models
                .iter()
                .map(|(id, name)| (*id, (*name).to_owned()))
                .collect(),
        )
    }

    fn entry(&self, identifier: &str) -> Option<&(u32, String)> {
        let by_id = identifier
            .parse::<u32>()
            .ok()
            .and_then(|id| self.0.iter().find(|(own, _)| *own == id));
        by_id.or_else(|| self.0.iter().find(|(_, name)| name == identifier))
    }

    fn find(&self, identifier: &str) -> Option<ModelSummary> {
        self.entry(identifier)
            .map(|(id, name)| Self::summary(*id, name))
    }

    /// The stored row, for the detail read: a file under `/models/`, and the
    /// same constants [`Self::summary`] reports. A model whose name ends in
    /// `-vision` is linked to a projector there too, and one whose name ends
    /// in `-draws` is a Flux.1 model linked to a VAE there.
    fn row(id: u32, name: &str) -> Model {
        let draws = name.ends_with("-draws");
        Model {
            components: draws
                .then(|| ModelComponent {
                    role: ComponentRole::Vae,
                    path: PathBuf::from("/models/ae.safetensors"),
                })
                .into_iter()
                .collect(),
            image_family: draws.then_some(ImageFamily::Flux1),
            id: i64::from(id),
            name: name.to_owned(),
            model_key: format!("local:{id}"),
            file_path: PathBuf::from(format!("/models/{name}.gguf")),
            projector_path: name
                .ends_with("-vision")
                .then(|| PathBuf::from("/models/mmproj-F16.gguf")),
            param_count_b: 7.0,
            architecture: Some("llama".to_owned()),
            quantization: Some("Q4_K_M".to_owned()),
            context_length: Some(8192),
            expert_count: None,
            expert_used_count: None,
            expert_shared_count: None,
            metadata: HashMap::new(),
            added_at: UNIX_EPOCH.into(),
            hf_repo_id: None,
            hf_commit_sha: None,
            hf_filename: None,
            download_date: None,
            last_update_check: None,
            tags: Vec::new(),
            capabilities: gglib_core::domain::ModelCapabilities::empty(),
            inference_defaults: None,
            defaults_origin: None,
            server_defaults: None,
            dialect_spec: None,
            template_caps: None,
            benchmark_summary: None,
        }
    }

    fn summary(id: u32, name: &str) -> ModelSummary {
        ModelSummary {
            image_input: name.ends_with("-vision"),
            quantization: Some("Q4_K_M".to_string()),
            architecture: Some("llama".to_string()),
            context_length: Some(8192),
            ..ModelSummary::bare(id, name)
        }
    }
}

#[async_trait]
impl ModelCatalogPort for StaticCatalog {
    async fn list_models(&self) -> Result<Vec<ModelSummary>, CatalogError> {
        Ok(self
            .0
            .iter()
            .map(|(id, name)| Self::summary(*id, name))
            .collect())
    }

    async fn resolve_model(&self, name: &str) -> Result<Option<ModelSummary>, CatalogError> {
        Ok(self.find(name))
    }

    async fn resolve_for_launch(
        &self,
        _name: &str,
    ) -> Result<Option<ModelLaunchSpec>, CatalogError> {
        Ok(None)
    }

    async fn model(&self, identifier: &str) -> Result<Option<Model>, CatalogError> {
        Ok(self
            .entry(identifier)
            .map(|(id, name)| Self::row(*id, name)))
    }
}

/// Runtime port that reports itself pinned to one model, id 1.
///
/// The read side of `gglib serve`: what a caller sees when the manager was
/// pinned via `ProcessManager::set_pin`. It does not enforce the pin, so a
/// test can tell the difference between "not advertised" and "refused".
/// [`StaticCatalog::new`] numbers from 1, so list the pinned model first.
#[derive(Debug)]
pub(crate) struct PinnedRuntime(pub &'static str);

#[async_trait]
impl ModelRuntimePort for PinnedRuntime {
    async fn admit(
        &self,
        _model_name: &str,
        _num_ctx: Option<u64>,
        _default_ctx: Option<u64>,
        _overrides: LaunchOverrides,
    ) -> Result<Admission, ModelRuntimeError> {
        Ok(Admission::detached(RunningTarget::local(
            0,
            1,
            self.0.into(),
            4096,
            false,
        )))
    }

    async fn current_model(&self) -> Option<RunningTarget> {
        None
    }

    async fn stop_current(&self) -> Result<(), ModelRuntimeError> {
        Ok(())
    }

    fn pinned(&self) -> Option<PinnedSpec> {
        Some(pin(1, self.0))
    }
}

/// Runtime port that *enforces* the pin, rather than only reporting it.
///
/// Resolves what it is asked to admit through its own catalog — unknown is
/// `ModelNotFound` — and refuses any model whose id is not the pin's, naming
/// both models. Admits onto `port`, so a test with an upstream there gets a
/// real answer.
#[derive(Debug)]
pub(crate) struct EnforcingPinnedRuntime {
    catalog: StaticCatalog,
    pin: (u32, String),
    port: u16,
}

impl EnforcingPinnedRuntime {
    /// Pinned to `pinned`, over a catalog of `models` numbered from 1, with no
    /// upstream behind it.
    pub(crate) fn over(pinned: &str, models: &[&str]) -> Self {
        Self::serving(StaticCatalog::new(models), pinned, 0)
    }

    /// Pinned to the model `catalog` holds as `pinned`, serving on `port`.
    ///
    /// # Panics
    ///
    /// If `catalog` does not hold `pinned`.
    pub(crate) fn serving(catalog: StaticCatalog, pinned: &str, port: u16) -> Self {
        let held = catalog
            .find(pinned)
            .expect("the pinned model is catalogued");
        Self {
            catalog,
            pin: (held.id, held.name),
            port,
        }
    }
}

#[async_trait]
impl ModelRuntimePort for EnforcingPinnedRuntime {
    async fn admit(
        &self,
        model_name: &str,
        _num_ctx: Option<u64>,
        _default_ctx: Option<u64>,
        _overrides: LaunchOverrides,
    ) -> Result<Admission, ModelRuntimeError> {
        let model = resolve_summary(&self.catalog, model_name).await?;
        if model.id != self.pin.0 {
            return Err(ModelRuntimeError::PinnedModelMismatch {
                expected: self.pin.1.clone(),
                requested: model.name,
            });
        }
        Ok(Admission::detached(RunningTarget::local(
            self.port, model.id, model.name, 4096, false,
        )))
    }

    async fn current_model(&self) -> Option<RunningTarget> {
        None
    }

    async fn stop_current(&self) -> Result<(), ModelRuntimeError> {
        Ok(())
    }

    fn pinned(&self) -> Option<PinnedSpec> {
        Some(pin(self.pin.0, &self.pin.1))
    }
}
