//! Tests for [`super::CatalogPortImpl`] and its `Model` projections.
//!
//! Split out via `#[path]` so the module itself stays inside the file budget.

use super::*;
use async_trait::async_trait;
use chrono::Utc;
use gglib_core::domain::{ModelCapabilities, ModelSamplingDefaults, NewModel, TemplateCaps};
use gglib_core::ports::RepositoryError;
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Mutex;

fn base_model() -> Model {
    Model {
        dialect_spec: None,
        id: 7,
        name: "qwen3".to_string(),
        model_key: String::new(),
        file_path: PathBuf::from("/models/qwen3.gguf"),
        param_count_b: 7.0,
        architecture: None,
        quantization: None,
        context_length: None,
        expert_count: None,
        expert_used_count: None,
        expert_shared_count: None,
        metadata: HashMap::new(),
        added_at: Utc::now(),
        hf_repo_id: None,
        hf_commit_sha: None,
        hf_filename: None,
        download_date: None,
        last_update_check: None,
        tags: vec!["format:qwen".to_string()],
        capabilities: ModelCapabilities::default(),
        inference_defaults: None,
        defaults_origin: None,
        server_defaults: None,
        template_caps: None,
        benchmark_summary: None,
    }
}

/// Serves one model: id 7, name "qwen3", tagged `format:qwen`.
struct OneModelRepo;

impl OneModelRepo {
    fn model() -> Model {
        base_model()
    }
}

#[async_trait]
impl ModelRepository for OneModelRepo {
    async fn list(&self) -> Result<Vec<Model>, RepositoryError> {
        Ok(vec![Self::model()])
    }

    async fn get_by_id(&self, id: i64) -> Result<Model, RepositoryError> {
        if id == 7 {
            Ok(Self::model())
        } else {
            Err(RepositoryError::NotFound(format!("id={id}")))
        }
    }

    async fn get_by_name(&self, name: &str) -> Result<Model, RepositoryError> {
        if name == "qwen3" {
            Ok(Self::model())
        } else {
            Err(RepositoryError::NotFound(format!("name={name}")))
        }
    }

    async fn find_by_path(&self, path: &std::path::Path) -> Result<Option<Model>, RepositoryError> {
        Ok(self
            .list()
            .await?
            .into_iter()
            .find(|m| m.file_path.as_path() == path))
    }

    async fn insert(&self, _m: &NewModel) -> Result<Model, RepositoryError> {
        unimplemented!("not exercised by these tests")
    }

    async fn update(&self, _m: &Model) -> Result<(), RepositoryError> {
        unimplemented!("not exercised by these tests")
    }

    async fn delete(&self, _id: i64) -> Result<(), RepositoryError> {
        unimplemented!("not exercised by these tests")
    }
}

fn port() -> CatalogPortImpl {
    CatalogPortImpl::new(Arc::new(OneModelRepo))
}

#[tokio::test]
async fn resolve_model_finds_by_name() {
    let found = port().resolve_model("qwen3").await.unwrap().unwrap();
    assert_eq!(found.tags, vec!["format:qwen".to_string()]);
}

/// The catalog port used to be name-only, so a numeric identifier resolved
/// to nothing here while `ModelService` resolved it fine. Both now share
/// `ModelRepository::get_by_identifier`; this asserts the port really does
/// delegate rather than keeping its own key.
#[tokio::test]
async fn resolve_model_finds_by_numeric_id() {
    let found = port().resolve_model("7").await.unwrap().unwrap();
    assert_eq!(found.name, "qwen3");
}

#[tokio::test]
async fn resolve_for_launch_finds_by_numeric_id() {
    let spec = port().resolve_for_launch("7").await.unwrap().unwrap();
    assert_eq!(spec.name, "qwen3");
}

#[tokio::test]
async fn unknown_model_resolves_to_none() {
    assert!(port().resolve_model("ghost").await.unwrap().is_none());
}

/// The launch spec derives what the model declares about sampling from the
/// metadata already on the catalog row — the same trip
/// `kv_elems_per_token` and `kv_memory_is_partial` make. Without it the
/// proxy's baseline check has no way to tell a model's own recommendation
/// from a pin bump.
#[test]
fn a_launch_spec_carries_what_the_models_gguf_declares() {
    let mut m = OneModelRepo::model();
    m.metadata
        .insert("general.sampling.temp".to_string(), "0.33".to_string());

    let spec = model_to_launch_spec(m);

    assert_eq!(
        spec.model_sampling.temperature,
        gglib_core::domain::ModelSamplingDefault::Declared(0.33)
    );
}

/// The ordinary model says nothing, and must arrive saying nothing rather
/// than arriving as "unknown" — the build's own table shows through.
#[test]
fn a_model_with_no_sampling_metadata_declares_nothing() {
    let spec = model_to_launch_spec(OneModelRepo::model());
    assert_eq!(spec.model_sampling, ModelSamplingDefaults::default());
}

// ── record_template_caps (ADR 0007) ───────────────────────────────────────

/// A repo that actually holds its one model, so the read-compare-write in
/// `record_template_caps` can be observed rather than stubbed away.
struct RecordingRepo {
    stored: Mutex<Model>,
    updates: Mutex<u32>,
}

impl RecordingRepo {
    fn with_caps(caps: Option<TemplateCaps>) -> Self {
        let mut model = base_model();
        model.template_caps = caps;
        Self {
            stored: Mutex::new(model),
            updates: Mutex::new(0),
        }
    }

    fn update_count(&self) -> u32 {
        *self.updates.lock().unwrap()
    }
}

#[async_trait]
impl ModelRepository for RecordingRepo {
    async fn list(&self) -> Result<Vec<Model>, RepositoryError> {
        Ok(vec![self.stored.lock().unwrap().clone()])
    }

    async fn get_by_id(&self, id: i64) -> Result<Model, RepositoryError> {
        let model = self.stored.lock().unwrap().clone();
        if id == model.id {
            Ok(model)
        } else {
            Err(RepositoryError::NotFound(format!("id={id}")))
        }
    }

    async fn get_by_name(&self, _name: &str) -> Result<Model, RepositoryError> {
        Ok(self.stored.lock().unwrap().clone())
    }

    async fn find_by_path(
        &self,
        _path: &std::path::Path,
    ) -> Result<Option<Model>, RepositoryError> {
        Ok(None)
    }

    async fn insert(&self, _m: &NewModel) -> Result<Model, RepositoryError> {
        unimplemented!("not exercised by these tests")
    }

    async fn update(&self, m: &Model) -> Result<(), RepositoryError> {
        *self.stored.lock().unwrap() = m.clone();
        *self.updates.lock().unwrap() += 1;
        Ok(())
    }

    async fn delete(&self, _id: i64) -> Result<(), RepositoryError> {
        unimplemented!("not exercised by these tests")
    }
}

fn effort_caps(supported: bool) -> TemplateCaps {
    TemplateCaps {
        supports_reasoning_effort: Some(supported),
        ..TemplateCaps::default()
    }
}

/// A fresh observation lands on the row.
#[tokio::test]
async fn a_new_observation_is_persisted() {
    let repo = Arc::new(RecordingRepo::with_caps(None));
    let port = CatalogPortImpl::new(Arc::clone(&repo) as Arc<dyn ModelRepository>);

    port.record_template_caps(7, effort_caps(true))
        .await
        .unwrap();

    assert_eq!(repo.update_count(), 1);
    assert_eq!(
        repo.stored.lock().unwrap().template_caps,
        Some(effort_caps(true))
    );
}

/// A repeat launch of an unchanged binary–model pair re-observes the same
/// caps, and the port must not echo them into the database again.
#[tokio::test]
async fn an_unchanged_observation_writes_nothing() {
    let repo = Arc::new(RecordingRepo::with_caps(Some(effort_caps(true))));
    let port = CatalogPortImpl::new(Arc::clone(&repo) as Arc<dyn ModelRepository>);

    port.record_template_caps(7, effort_caps(true))
        .await
        .unwrap();

    assert_eq!(repo.update_count(), 0, "matching caps must skip the write");
}

/// A pin bump (or a re-derived template) can genuinely change the answer,
/// and the changed answer must win over the stored one.
#[tokio::test]
async fn a_changed_observation_overwrites_the_stored_one() {
    let repo = Arc::new(RecordingRepo::with_caps(Some(effort_caps(true))));
    let port = CatalogPortImpl::new(Arc::clone(&repo) as Arc<dyn ModelRepository>);

    port.record_template_caps(7, effort_caps(false))
        .await
        .unwrap();

    assert_eq!(repo.update_count(), 1);
    assert_eq!(
        repo.stored.lock().unwrap().template_caps,
        Some(effort_caps(false))
    );
}

/// The summary projection carries the recorded caps through, so every
/// consumer of `ModelSummary` sees the same tri-state the row holds.
#[test]
fn a_summary_carries_the_recorded_caps() {
    let mut m = base_model();
    m.template_caps = Some(effort_caps(true));
    assert_eq!(model_to_summary(&m).template_caps, Some(effort_caps(true)));

    assert_eq!(
        model_to_summary(&base_model()).template_caps,
        None,
        "never observed stays None, not a manufactured negative"
    );
}
