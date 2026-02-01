//! Model service - orchestrates model CRUD operations.

use crate::domain::{Model, NewModel};
use crate::ports::{CoreError, ModelRepository, RepositoryError};
use std::sync::Arc;

/// Service for model operations.
///
/// This service provides high-level model management by delegating
/// to the injected `ModelRepository`. It adds no business logic
/// beyond what the repository provides - it's a thin facade.
pub struct ModelService {
    repo: Arc<dyn ModelRepository>,
}

impl ModelService {
    /// Create a new model service with the given repository.
    pub fn new(repo: Arc<dyn ModelRepository>) -> Self {
        Self { repo }
    }

    /// List all models.
    pub async fn list(&self) -> Result<Vec<Model>, CoreError> {
        self.repo.list().await.map_err(CoreError::from)
    }

    /// Get a model by its identifier (id, name, or HF ID).
    pub async fn get(&self, identifier: &str) -> Result<Option<Model>, CoreError> {
        // Try by ID first
        if let Ok(id) = identifier.parse::<i64>() {
            match self.repo.get_by_id(id).await {
                Ok(model) => return Ok(Some(model)),
                Err(RepositoryError::NotFound(_)) => {}
                Err(e) => return Err(CoreError::from(e)),
            }
        }
        // Try by name
        match self.repo.get_by_name(identifier).await {
            Ok(model) => Ok(Some(model)),
            Err(RepositoryError::NotFound(_)) => Ok(None),
            Err(e) => Err(CoreError::from(e)),
        }
    }

    /// Get a model by its database ID.
    pub async fn get_by_id(&self, id: i64) -> Result<Option<Model>, CoreError> {
        match self.repo.get_by_id(id).await {
            Ok(model) => Ok(Some(model)),
            Err(RepositoryError::NotFound(_)) => Ok(None),
            Err(e) => Err(CoreError::from(e)),
        }
    }

    /// Get a model by name.
    pub async fn get_by_name(&self, name: &str) -> Result<Option<Model>, CoreError> {
        match self.repo.get_by_name(name).await {
            Ok(model) => Ok(Some(model)),
            Err(RepositoryError::NotFound(_)) => Ok(None),
            Err(e) => Err(CoreError::from(e)),
        }
    }

    /// Find a model by identifier (id, name, or HF ID).
    /// Returns error if not found.
    pub async fn find_by_identifier(&self, identifier: &str) -> Result<Model, CoreError> {
        self.get(identifier)
            .await?
            .ok_or_else(|| CoreError::Validation(format!("Model not found: {identifier}")))
    }

    /// Find a model by name. Returns error if not found.
    pub async fn find_by_name(&self, name: &str) -> Result<Model, CoreError> {
        self.get_by_name(name)
            .await?
            .ok_or_else(|| CoreError::Validation(format!("Model not found: {name}")))
    }

    /// Add a new model.
    pub async fn add(&self, model: NewModel) -> Result<Model, CoreError> {
        self.repo.insert(&model).await.map_err(CoreError::from)
    }

    /// Update a model.
    pub async fn update(&self, model: &Model) -> Result<(), CoreError> {
        self.repo.update(model).await.map_err(CoreError::from)
    }

    /// Delete a model by ID.
    pub async fn delete(&self, id: i64) -> Result<(), CoreError> {
        self.repo.delete(id).await.map_err(CoreError::from)
    }

    /// Remove a model by identifier. Returns the removed model.
    pub async fn remove(&self, identifier: &str) -> Result<Model, CoreError> {
        let model = self.find_by_identifier(identifier).await?;
        self.repo.delete(model.id).await.map_err(CoreError::from)?;
        Ok(model)
    }

    // ─────────────────────────────────────────────────────────────────────────
    // Tag Operations
    // ─────────────────────────────────────────────────────────────────────────

    /// List all unique tags used across all models.
    pub async fn list_tags(&self) -> Result<Vec<String>, CoreError> {
        let models = self.repo.list().await.map_err(CoreError::from)?;
        let mut all_tags = std::collections::HashSet::new();
        for model in models {
            for tag in model.tags {
                all_tags.insert(tag);
            }
        }
        let mut tags: Vec<String> = all_tags.into_iter().collect();
        tags.sort();
        Ok(tags)
    }

    /// Add a tag to a model.
    ///
    /// If the tag already exists on the model, this is a no-op.
    pub async fn add_tag(&self, model_id: i64, tag: String) -> Result<(), CoreError> {
        let mut model = self
            .repo
            .get_by_id(model_id)
            .await
            .map_err(CoreError::from)?;
        if !model.tags.contains(&tag) {
            model.tags.push(tag);
            model.tags.sort();
            self.repo.update(&model).await.map_err(CoreError::from)?;
        }
        Ok(())
    }

    /// Remove a tag from a model.
    ///
    /// If the tag doesn't exist on the model, this is a no-op.
    pub async fn remove_tag(&self, model_id: i64, tag: &str) -> Result<(), CoreError> {
        let mut model = self
            .repo
            .get_by_id(model_id)
            .await
            .map_err(CoreError::from)?;
        model.tags.retain(|t| t != tag);
        self.repo.update(&model).await.map_err(CoreError::from)?;
        Ok(())
    }

    /// Get all tags for a specific model.
    pub async fn get_tags(&self, model_id: i64) -> Result<Vec<String>, CoreError> {
        let model = self
            .repo
            .get_by_id(model_id)
            .await
            .map_err(CoreError::from)?;
        Ok(model.tags)
    }

    /// Get all models that have a specific tag.
    pub async fn get_by_tag(&self, tag: &str) -> Result<Vec<Model>, CoreError> {
        let models = self.repo.list().await.map_err(CoreError::from)?;
        Ok(models
            .into_iter()
            .filter(|m| m.tags.contains(&tag.to_string()))
            .collect())
    }

    // ─────────────────────────────────────────────────────────────────────────
    // Filter/Aggregate Operations
    // ─────────────────────────────────────────────────────────────────────────

    /// Get filter options aggregated from all models.
    ///
    /// Returns distinct quantizations, parameter count range, and context length range
    /// for use in the GUI filter popover.
    ///
    /// Note: Uses in-memory aggregation for simplicity. This is acceptable for typical
    /// model libraries (<100 models). Revisit if libraries grow large.
    pub async fn get_filter_options(&self) -> Result<crate::domain::ModelFilterOptions, CoreError> {
        use crate::domain::{ModelFilterOptions, RangeValues};
        use std::collections::HashSet;

        let models = self.repo.list().await.map_err(CoreError::from)?;

        // Collect distinct quantizations
        let mut quantizations: Vec<String> = models
            .iter()
            .filter_map(|m| m.quantization.clone())
            .filter(|q| !q.is_empty())
            .collect::<HashSet<_>>()
            .into_iter()
            .collect();
        quantizations.sort();

        // Compute param_count_b range
        let param_range = if models.is_empty() {
            None
        } else {
            let min = models
                .iter()
                .map(|m| m.param_count_b)
                .fold(f64::INFINITY, f64::min);
            let max = models
                .iter()
                .map(|m| m.param_count_b)
                .fold(f64::NEG_INFINITY, f64::max);
            if min.is_finite() && max.is_finite() {
                Some(RangeValues { min, max })
            } else {
                None
            }
        };

        // Compute context_length range (only models with context_length set)
        let context_lengths: Vec<u64> = models.iter().filter_map(|m| m.context_length).collect();
        #[allow(clippy::cast_precision_loss)]
        let context_range = if context_lengths.is_empty() {
            None
        } else {
            let min = *context_lengths.iter().min().unwrap() as f64;
            let max = *context_lengths.iter().max().unwrap() as f64;
            Some(RangeValues { min, max })
        };

        Ok(ModelFilterOptions {
            quantizations,
            param_range,
            context_range,
        })
    }

    // ─────────────────────────────────────────────────────────────────────────
    // Capability Bootstrap
    // ─────────────────────────────────────────────────────────────────────────

    /// Backfill capabilities for models that don't have them set.
    ///
    /// This runs on startup to handle models with unknown capabilities.
    /// Only infers if capabilities are empty (0/unknown).
    ///
    /// # INVARIANT
    ///
    /// Never overwrite explicitly-set capabilities. Only infer when unknown.
    pub async fn bootstrap_capabilities(&self) -> Result<(), CoreError> {
        use crate::domain::infer_from_chat_template;

        let models = self.repo.list().await.map_err(CoreError::from)?;

        for mut model in models {
            // Only infer if capabilities are unknown (empty)
            if model.capabilities.is_empty() {
                let template = model.metadata.get("tokenizer.chat_template");
                let name = model.metadata.get("general.name");
                let inferred = infer_from_chat_template(
                    template.map(String::as_str),
                    name.map(String::as_str),
                );

                model.capabilities = inferred;
                self.repo.update(&model).await.map_err(CoreError::from)?;
            }
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ports::{ModelRepository, RepositoryError};
    use async_trait::async_trait;
    use chrono::Utc;

    use std::path::PathBuf;
    use std::sync::Mutex;

    struct MockRepo {
        models: Mutex<Vec<Model>>,
    }

    impl MockRepo {
        fn new() -> Self {
            Self {
                models: Mutex::new(vec![]),
            }
        }
    }

    #[async_trait]
    impl ModelRepository for MockRepo {
        async fn list(&self) -> Result<Vec<Model>, RepositoryError> {
            Ok(self.models.lock().unwrap().clone())
        }

        async fn get_by_id(&self, id: i64) -> Result<Model, RepositoryError> {
            self.models
                .lock()
                .unwrap()
                .iter()
                .find(|m| m.id == id)
                .cloned()
                .ok_or_else(|| RepositoryError::NotFound(format!("id={id}")))
        }

        async fn get_by_name(&self, name: &str) -> Result<Model, RepositoryError> {
            self.models
                .lock()
                .unwrap()
                .iter()
                .find(|m| m.name == name)
                .cloned()
                .ok_or_else(|| RepositoryError::NotFound(format!("name={name}")))
        }

        #[allow(clippy::cast_possible_wrap, clippy::significant_drop_tightening)]
        async fn insert(&self, model: &NewModel) -> Result<Model, RepositoryError> {
            let mut models = self.models.lock().unwrap();
            let id = models.len() as i64 + 1;
            let created = Model {
                id,
                name: model.name.clone(),
                file_path: model.file_path.clone(),
                param_count_b: model.param_count_b,
                architecture: model.architecture.clone(),
                quantization: model.quantization.clone(),
                context_length: model.context_length,
                metadata: model.metadata.clone(),
                added_at: model.added_at,
                hf_repo_id: model.hf_repo_id.clone(),
                hf_commit_sha: model.hf_commit_sha.clone(),
                hf_filename: model.hf_filename.clone(),
                download_date: model.download_date,
                last_update_check: model.last_update_check,
                tags: model.tags.clone(),
                capabilities: model.capabilities,
            };
            models.push(created.clone());
            Ok(created)
        }

        async fn update(&self, model: &Model) -> Result<(), RepositoryError> {
            let mut models = self.models.lock().unwrap();
            models.iter_mut().find(|m| m.id == model.id).map_or_else(
                || Err(RepositoryError::NotFound(format!("id={}", model.id))),
                |m| {
                    m.clone_from(model);
                    Ok(())
                },
            )
        }

        async fn delete(&self, id: i64) -> Result<(), RepositoryError> {
            let mut models = self.models.lock().unwrap();
            let len_before = models.len();
            models.retain(|m| m.id != id);
            if models.len() == len_before {
                Err(RepositoryError::NotFound(format!("id={id}")))
            } else {
                Ok(())
            }
        }
    }

    #[tokio::test]
    async fn test_list_empty() {
        let repo = Arc::new(MockRepo::new());
        let service = ModelService::new(repo);
        let models = service.list().await.unwrap();
        assert!(models.is_empty());
    }

    #[tokio::test]
    async fn test_add_and_get() {
        let repo = Arc::new(MockRepo::new());
        let service = ModelService::new(repo);

        let new_model = NewModel::new(
            "test-model".to_string(),
            PathBuf::from("/path/to/model.gguf"),
            7.0,
            Utc::now(),
        );

        let created = service.add(new_model).await.unwrap();
        assert_eq!(created.name, "test-model");

        let found = service.get_by_name("test-model").await.unwrap();
        assert!(found.is_some());
        assert_eq!(found.unwrap().id, created.id);
    }

    #[tokio::test]
    async fn test_find_by_identifier_not_found() {
        let repo = Arc::new(MockRepo::new());
        let service = ModelService::new(repo);

        let result = service.find_by_identifier("nonexistent").await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_get_filter_options_empty() {
        let repo = Arc::new(MockRepo::new());
        let service = ModelService::new(repo);

        let options = service.get_filter_options().await.unwrap();
        assert!(options.quantizations.is_empty());
        assert!(options.param_range.is_none());
        assert!(options.context_range.is_none());
    }

    #[tokio::test]
    async fn test_get_filter_options_with_models() {
        let repo = Arc::new(MockRepo::new());
        let service = ModelService::new(repo);

        // Add models with different characteristics
        let mut model1 = NewModel::new(
            "model-1".to_string(),
            PathBuf::from("/path/to/model1.gguf"),
            7.0,
            Utc::now(),
        );
        model1.quantization = Some("Q4_K_M".to_string());
        model1.context_length = Some(4096);

        let mut model2 = NewModel::new(
            "model-2".to_string(),
            PathBuf::from("/path/to/model2.gguf"),
            13.0,
            Utc::now(),
        );
        model2.quantization = Some("Q8_0".to_string());
        model2.context_length = Some(8192);

        let mut model3 = NewModel::new(
            "model-3".to_string(),
            PathBuf::from("/path/to/model3.gguf"),
            70.0,
            Utc::now(),
        );
        model3.quantization = Some("Q4_K_M".to_string()); // Duplicate quant
        // No context_length set

        service.add(model1).await.unwrap();
        service.add(model2).await.unwrap();
        service.add(model3).await.unwrap();

        let options = service.get_filter_options().await.unwrap();

        // Should have 2 distinct quantizations, sorted
        assert_eq!(options.quantizations, vec!["Q4_K_M", "Q8_0"]);

        // Param range: 7.0 to 70.0
        let param_range = options.param_range.unwrap();
        assert!((param_range.min - 7.0).abs() < 0.001);
        assert!((param_range.max - 70.0).abs() < 0.001);

        // Context range: 4096 to 8192 (model3 has no context)
        let context_range = options.context_range.unwrap();
        assert!((context_range.min - 4096.0).abs() < 0.001);
        assert!((context_range.max - 8192.0).abs() < 0.001);
    }
}
