use super::*;

use std::collections::HashSet;

use async_trait::async_trait;
use gglib_core::domain::InferenceConfig;
use gglib_core::ports::model_catalog::{CatalogError, ModelLaunchSpec, ModelSummary};

/// Catalog holding a fixed set of names, resolving by exact match — the
/// behaviour of the real SQLite repository (`WHERE name = ?`).
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
        Ok(vec![])
    }

    async fn resolve_model(&self, name: &str) -> Result<Option<ModelSummary>, CatalogError> {
        Ok(self.names.contains(name).then(|| summary(name)))
    }

    async fn resolve_for_launch(
        &self,
        _name: &str,
    ) -> Result<Option<ModelLaunchSpec>, CatalogError> {
        Ok(None)
    }
}

/// `ModelSummary` has no `Default`, and routing only ever reads presence.
fn summary(name: &str) -> ModelSummary {
    ModelSummary {
        id: 1,
        name: name.to_owned(),
        tags: vec![],
        capabilities: Default::default(),
        param_count: String::new(),
        quantization: None,
        architecture: None,
        created_at: 0,
        file_size: 0,
        context_length: None,
        inference_defaults: None,
        defaults_origin: None,
        server_defaults: None,
        dialect: None,
        template_caps: None,
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
async fn a_bare_identifier_selects_no_profile() {
    let selection = select(&NamedCatalog::new(&["qwen"]), &profiles(), "qwen", None)
        .await
        .unwrap();
    assert_eq!(selection.model, "qwen");
    assert!(selection.profile.is_none());
}

#[tokio::test]
async fn a_suffix_selects_the_profile_and_strips_itself() {
    let selection = select(
        &NamedCatalog::new(&["qwen"]),
        &profiles(),
        "qwen:coding",
        None,
    )
    .await
    .unwrap();
    assert_eq!(selection.model, "qwen", "the suffix must not reach lookup");
    assert_eq!(selection.profile.unwrap().name, "coding");
}

/// A numeric id carrying a suffix — `gglib chat 7:coding`. The proxy never
/// exercised this: HTTP clients name models, not database ids.
#[tokio::test]
async fn a_numeric_identifier_with_a_profile_suffix_routes() {
    let selection = select(&NamedCatalog::new(&["7"]), &profiles(), "7:coding", None)
        .await
        .unwrap();
    assert_eq!(selection.model, "7");
    assert_eq!(selection.profile.unwrap().name, "coding");
}

#[tokio::test]
async fn the_flag_alone_selects_the_profile() {
    let selection = select(
        &NamedCatalog::new(&["qwen"]),
        &profiles(),
        "qwen",
        Some("coding"),
    )
    .await
    .unwrap();
    assert_eq!(selection.model, "qwen");
    assert_eq!(selection.profile.unwrap().name, "coding");
}

#[tokio::test]
async fn both_a_flag_and_a_suffix_is_an_error() {
    let err = select(
        &NamedCatalog::new(&["qwen"]),
        &profiles(),
        "qwen:coding",
        Some("coding"),
    )
    .await
    .unwrap_err()
    .to_string();
    assert!(err.contains("--profile"), "unexpected message: {err}");
}

#[tokio::test]
async fn an_unknown_flag_lists_the_configured_ones() {
    let err = select(
        &NamedCatalog::new(&["qwen"]),
        &profiles(),
        "qwen",
        Some("codign"),
    )
    .await
    .unwrap_err()
    .to_string();
    assert!(err.contains("coding"), "unexpected message: {err}");
}

/// A renamed or deleted suffix must fail loudly rather than silently sampling
/// unprofiled — the same call `resolve_route` makes for an HTTP request.
#[tokio::test]
async fn an_unknown_suffix_on_a_real_model_is_an_error() {
    let err = select(
        &NamedCatalog::new(&["qwen"]),
        &profiles(),
        "qwen:codign",
        None,
    )
    .await
    .unwrap_err()
    .to_string();
    assert!(err.contains("coding"), "unexpected message: {err}");
}
