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
        dialect: m.dialect_spec.clone(),
        template_caps: m.template_caps.clone(),
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
        defaults_origin: m.defaults_origin,
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
    // Third fact derived from the same stored GGUF map. See
    // `ModelSamplingDefaults` for why the proxy needs it.
    let model_sampling = gglib_core::domain::ModelSamplingDefaults::from_metadata(&m.metadata);

    ModelLaunchSpec {
        id: m.id as u32,
        name: m.name,
        file_path: m.file_path,
        tags: m.tags,
        architecture: m.architecture,
        quantization: m.quantization,
        context_length: m.context_length,
        server_defaults: m.server_defaults,
        file_size_bytes,
        kv_elems_per_token,
        kv_memory_is_partial,
        model_sampling,
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

    /// Persist a launch's `chat_template_caps` observation onto the model row.
    ///
    /// Read-compare-write, and the write is skipped when the stored value
    /// already matches: the caps are a fact about the binary–model pair
    /// (ADR 0007), so every launch of an unchanged pair re-observes the same
    /// value, and echoing it into the database each time would be churn with
    /// no information in it.
    async fn record_template_caps(
        &self,
        id: u32,
        caps: gglib_core::domain::TemplateCaps,
    ) -> Result<(), CatalogError> {
        let mut model = self
            .repo
            .get_by_id(i64::from(id))
            .await
            .map_err(|e| CatalogError::QueryFailed(e.to_string()))?;
        if model.template_caps.as_ref() == Some(&caps) {
            return Ok(());
        }
        model.template_caps = Some(caps);
        self.repo
            .update(&model)
            .await
            .map_err(|e| CatalogError::QueryFailed(e.to_string()))
    }
}

#[cfg(test)]
#[path = "model_catalog_tests.rs"]
mod model_catalog_tests;
