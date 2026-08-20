use super::*;

use std::collections::HashSet;

use async_trait::async_trait;

use crate::domain::InferenceConfig;
use crate::ports::model_catalog::{CatalogError, ModelLaunchSpec, ModelSummary};

/// behaviour of the real `SQLite` repository (`WHERE name = ?`).
#[derive(Debug)]
struct NamedCatalog {
    names: HashSet<String>,
}

impl NamedCatalog {
    fn new(names: &[&str]) -> Self {
        Self {
            names: names.iter().map(|n| (*n).to_owned()).collect(),
        }
    }
}

#[async_trait]
impl ModelCatalogPort for NamedCatalog {
    async fn list_models(&self) -> Result<Vec<ModelSummary>, CatalogError> {
        Ok(Vec::new())
    }

    async fn resolve_model(&self, name: &str) -> Result<Option<ModelSummary>, CatalogError> {
        Ok(self.names.contains(name).then(|| ModelSummary {
            dialect: None,
            template_caps: None,
            id: 1,
            name: name.to_owned(),
            tags: Vec::new(),
            capabilities: crate::domain::ModelCapabilities::empty(),
            param_count: "7B".to_owned(),
            quantization: None,
            architecture: None,
            created_at: 0,
            file_size: 0,
            context_length: None,
            inference_defaults: None,
            defaults_origin: None,
            server_defaults: None,
        }))
    }

    async fn resolve_for_launch(
        &self,
        _name: &str,
    ) -> Result<Option<ModelLaunchSpec>, CatalogError> {
        Ok(None)
    }
}

/// Catalog whose every query fails, to pin the fail-open behaviour.
#[derive(Debug)]
struct BrokenCatalog;

#[async_trait]
impl ModelCatalogPort for BrokenCatalog {
    async fn list_models(&self) -> Result<Vec<ModelSummary>, CatalogError> {
        Err(CatalogError::QueryFailed("boom".to_owned()))
    }

    async fn resolve_model(&self, _name: &str) -> Result<Option<ModelSummary>, CatalogError> {
        Err(CatalogError::QueryFailed("boom".to_owned()))
    }

    async fn resolve_for_launch(
        &self,
        _name: &str,
    ) -> Result<Option<ModelLaunchSpec>, CatalogError> {
        Err(CatalogError::QueryFailed("boom".to_owned()))
    }
}

fn profiles() -> Vec<InferenceProfile> {
    vec![InferenceProfile {
        name: "coding".to_owned(),
        description: None,
        config: InferenceConfig {
            temperature: Some(0.2),
            ..Default::default()
        },
        list_in_models: false,
    }]
}

#[tokio::test]
async fn plain_model_name_is_bare() {
    let catalog = NamedCatalog::new(&["qwen"]);
    assert_eq!(
        resolve_route("qwen", &profiles(), &catalog).await,
        ModelRoute::Bare("qwen")
    );
}

#[tokio::test]
async fn known_profile_suffix_resolves_to_profiled() {
    let catalog = NamedCatalog::new(&["qwen"]);
    let profiles = profiles();
    match resolve_route("qwen:coding", &profiles, &catalog).await {
        ModelRoute::Profiled { model, profile } => {
            assert_eq!(model, "qwen");
            assert_eq!(profile.name, "coding");
        }
        other => panic!("expected Profiled, got {other:?}"),
    }
}

/// A model that genuinely owns a colon-bearing name must win over any
/// profile reading of its suffix — otherwise adding a profile could
/// shadow an existing model.
#[tokio::test]
async fn real_colon_bearing_model_name_wins_over_profile_suffix() {
    let catalog = NamedCatalog::new(&["qwen:coding", "qwen"]);
    assert_eq!(
        resolve_route("qwen:coding", &profiles(), &catalog).await,
        ModelRoute::Bare("qwen:coding")
    );
}

/// The reason this module exists: a renamed or deleted profile must fail
/// loudly rather than quietly sampling at the model's own temperature.
#[tokio::test]
async fn unknown_suffix_on_a_real_model_is_profile_not_found() {
    let catalog = NamedCatalog::new(&["qwen"]);
    assert_eq!(
        resolve_route("qwen:deleted", &profiles(), &catalog).await,
        ModelRoute::ProfileNotFound {
            requested: "qwen:deleted",
            suffix: "deleted",
        }
    );
}

/// Nothing resolves — stay out of the way and let the existing
/// model-not-found path report it.
#[tokio::test]
async fn unknown_base_falls_through_to_bare() {
    let catalog = NamedCatalog::new(&["qwen"]);
    assert_eq!(
        resolve_route("mistral:27b", &profiles(), &catalog).await,
        ModelRoute::Bare("mistral:27b")
    );
}

/// Splitting on the last colon lets a colon-bearing model still take a
/// profile.
#[tokio::test]
async fn splits_on_the_last_colon() {
    let catalog = NamedCatalog::new(&["qwen:27b"]);
    match resolve_route("qwen:27b:coding", &profiles(), &catalog).await {
        ModelRoute::Profiled { model, profile } => {
            assert_eq!(model, "qwen:27b");
            assert_eq!(profile.name, "coding");
        }
        other => panic!("expected Profiled, got {other:?}"),
    }
}

/// With no profiles configured at all, a colon-bearing id must not become
/// a hard error when it is simply an unknown model.
#[tokio::test]
async fn no_profiles_configured_leaves_unknown_ids_bare() {
    let catalog = NamedCatalog::new(&[]);
    assert_eq!(
        resolve_route("qwen:27b", &[], &catalog).await,
        ModelRoute::Bare("qwen:27b")
    );
}

/// A broken catalog must not escalate into a request failure.
#[tokio::test]
async fn catalog_errors_fail_open_to_bare() {
    assert_eq!(
        resolve_route("qwen:unknown", &profiles(), &BrokenCatalog).await,
        ModelRoute::Bare("qwen:unknown")
    );
}

/// A catalog error must not suppress a profile that is configured.
#[tokio::test]
async fn catalog_errors_still_allow_a_known_profile() {
    let profiles = profiles();
    match resolve_route("qwen:coding", &profiles, &BrokenCatalog).await {
        ModelRoute::Profiled { model, .. } => assert_eq!(model, "qwen"),
        other => panic!("expected Profiled, got {other:?}"),
    }
}
