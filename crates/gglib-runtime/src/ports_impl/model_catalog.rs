//! ModelCatalogPort implementation using ModelRepository.
//!
//! This adapter wraps the ModelRepository to implement the ModelCatalogPort
//! interface from gglib-core. It queries the database for model information
//! and maps the results to domain types.
//!
//! Identifier resolution is **not** decided here — both `resolve_*` methods go
//! through [`ModelRepository::get_by_identifier`], the workspace's single
//! lookup-key policy, so this port and `ModelService` always agree on what a
//! given string means.

use async_trait::async_trait;
use gglib_core::domain::Model;
use gglib_core::ports::{
    CatalogError, ModelCatalogPort, ModelLaunchSpec, ModelRepository, ModelSummary,
};
use std::fmt;
use std::sync::Arc;

use super::model_shards::total_model_bytes;

/// Format param count (in billions) as a human-readable string.
fn format_param_count(param_b: f64) -> String {
    if param_b >= 1.0 {
        format!("{:.0}B", param_b)
    } else {
        format!("{:.1}B", param_b)
    }
}

/// Helper to convert Model to ModelSummary (for listing).
fn model_to_summary(m: &Model) -> ModelSummary {
    // Get file size from disk if possible, otherwise 0
    let file_size = m.file_path.metadata().map(|md| md.len()).unwrap_or(0);

    ModelSummary {
        id: m.id as u32,
        name: m.name.clone(),
        tags: m.tags.clone(),
        capabilities: m.capabilities,
        param_count: format_param_count(m.param_count_b),
        quantization: m.quantization.clone(),
        architecture: m.architecture.clone(),
        created_at: m.added_at.timestamp(),
        file_size,
        context_length: m.context_length,
        inference_defaults: m.inference_defaults.clone(),
        server_defaults: m.server_defaults.clone(),
    }
}

/// Helper to convert Model to ModelLaunchSpec (for launching).
fn model_to_launch_spec(m: Model) -> ModelLaunchSpec {
    let file_size_bytes = total_model_bytes(&m.file_path);
    let kv_elems_per_token =
        gglib_core::domain::estimate_kv_elems_per_token(&m.metadata, m.architecture.as_deref());
    let kv_memory_is_partial =
        gglib_core::domain::kv_memory_is_partial(&m.metadata, m.architecture.as_deref());

    ModelLaunchSpec {
        id: m.id as u32,
        name: m.name,
        file_path: m.file_path,
        tags: m.tags,
        architecture: m.architecture,
        context_length: m.context_length,
        server_defaults: m.server_defaults,
        file_size_bytes,
        kv_elems_per_token,
        kv_memory_is_partial,
    }
}

/// Implementation of ModelCatalogPort using ModelRepository.
///
/// Wraps the ModelRepository to provide catalog access for the proxy.
pub struct CatalogPortImpl {
    /// The underlying model repository.
    repo: Arc<dyn ModelRepository>,
}

impl CatalogPortImpl {
    /// Create a new CatalogPortImpl.
    ///
    /// # Arguments
    ///
    /// * `repo` - The model repository for database access
    pub fn new(repo: Arc<dyn ModelRepository>) -> Self {
        Self { repo }
    }

    /// Resolve `name` through the shared identifier policy (numeric id, then
    /// exact name), mapping storage failures into [`CatalogError`].
    ///
    /// Both `resolve_*` methods go through here so the port cannot end up
    /// resolving the same string two different ways.
    async fn lookup(&self, name: &str) -> Result<Option<Model>, CatalogError> {
        self.repo
            .get_by_identifier(name)
            .await
            .map_err(|e| CatalogError::QueryFailed(e.to_string()))
    }
}

impl fmt::Debug for CatalogPortImpl {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("CatalogPortImpl").finish()
    }
}

#[async_trait]
impl ModelCatalogPort for CatalogPortImpl {
    async fn list_models(&self) -> Result<Vec<ModelSummary>, CatalogError> {
        let models = self
            .repo
            .list()
            .await
            .map_err(|e| CatalogError::QueryFailed(e.to_string()))?;

        Ok(models.iter().map(model_to_summary).collect())
    }

    async fn resolve_model(&self, name: &str) -> Result<Option<ModelSummary>, CatalogError> {
        Ok(self.lookup(name).await?.as_ref().map(model_to_summary))
    }

    async fn resolve_for_launch(
        &self,
        name: &str,
    ) -> Result<Option<ModelLaunchSpec>, CatalogError> {
        Ok(self.lookup(name).await?.map(model_to_launch_spec))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;
    use gglib_core::domain::{ModelCapabilities, NewModel};
    use gglib_core::ports::RepositoryError;
    use std::collections::HashMap;
    use std::path::PathBuf;

    /// Serves one model: id 7, name "qwen3", tagged `format:qwen`.
    struct OneModelRepo;

    impl OneModelRepo {
        fn model() -> Model {
            Model {
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
                server_defaults: None,
                benchmark_summary: None,
            }
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
}
