//! ModelCatalogPort implementation using ModelRepository.
//!
//! This adapter wraps the ModelRepository to implement the ModelCatalogPort
//! interface from gglib-core. It queries the database for model information
//! and maps the results to domain types.

use async_trait::async_trait;
use gglib_core::domain::Model;
use gglib_core::ports::{
    CatalogError, ModelCatalogPort, ModelLaunchSpec, ModelRepository, ModelSummary, RepositoryError,
};
use std::fmt;
use std::sync::Arc;

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
        param_count: format_param_count(m.param_count_b),
        quantization: m.quantization.clone(),
        architecture: m.architecture.clone(),
        created_at: m.added_at.timestamp(),
        file_size,
    }
}

/// Helper to convert Model to ModelLaunchSpec (for launching).
fn model_to_launch_spec(m: Model) -> ModelLaunchSpec {
    ModelLaunchSpec {
        id: m.id as u32,
        name: m.name,
        file_path: m.file_path,
        tags: m.tags,
        architecture: m.architecture,
        context_length: m.context_length,
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
        match self.repo.get_by_name(name).await {
            Ok(model) => Ok(Some(model_to_summary(&model))),
            Err(RepositoryError::NotFound(_)) => Ok(None),
            Err(e) => Err(CatalogError::QueryFailed(e.to_string())),
        }
    }

    async fn resolve_for_launch(
        &self,
        name: &str,
    ) -> Result<Option<ModelLaunchSpec>, CatalogError> {
        match self.repo.get_by_name(name).await {
            Ok(model) => Ok(Some(model_to_launch_spec(model))),
            Err(RepositoryError::NotFound(_)) => Ok(None),
            Err(e) => Err(CatalogError::QueryFailed(e.to_string())),
        }
    }
}

#[cfg(test)]
mod tests {
    // Tests would require mocking ModelRepository
}
