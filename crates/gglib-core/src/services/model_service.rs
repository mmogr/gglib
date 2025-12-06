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
            .ok_or_else(|| CoreError::Validation(format!("Model not found: {}", identifier)))
    }

    /// Find a model by name. Returns error if not found.
    pub async fn find_by_name(&self, name: &str) -> Result<Model, CoreError> {
        self.get_by_name(name)
            .await?
            .ok_or_else(|| CoreError::Validation(format!("Model not found: {}", name)))
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
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ports::{ModelRepository, RepositoryError};
    use async_trait::async_trait;
    use chrono::Utc;
    use std::collections::HashMap;
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
                .ok_or_else(|| RepositoryError::NotFound(format!("id={}", id)))
        }

        async fn get_by_name(&self, name: &str) -> Result<Model, RepositoryError> {
            self.models
                .lock()
                .unwrap()
                .iter()
                .find(|m| m.name == name)
                .cloned()
                .ok_or_else(|| RepositoryError::NotFound(format!("name={}", name)))
        }

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
            };
            models.push(created.clone());
            Ok(created)
        }

        async fn update(&self, model: &Model) -> Result<(), RepositoryError> {
            let mut models = self.models.lock().unwrap();
            if let Some(m) = models.iter_mut().find(|m| m.id == model.id) {
                *m = model.clone();
                Ok(())
            } else {
                Err(RepositoryError::NotFound(format!("id={}", model.id)))
            }
        }

        async fn delete(&self, id: i64) -> Result<(), RepositoryError> {
            let mut models = self.models.lock().unwrap();
            let len_before = models.len();
            models.retain(|m| m.id != id);
            if models.len() == len_before {
                Err(RepositoryError::NotFound(format!("id={}", id)))
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
}
