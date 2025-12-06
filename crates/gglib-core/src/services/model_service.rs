//! Model service for GGUF model operations.
//!
//! This service provides model-related operations using the `ModelRepository`
//! port trait, allowing it to work with any repository implementation.

use std::sync::Arc;

use crate::domain::{Model, NewModel};
use crate::ports::{CoreError, ModelRepository};

/// Service for managing GGUF models.
///
/// Provides CRUD operations and model queries without knowing
/// the underlying storage implementation.
#[derive(Clone)]
pub struct ModelService {
    repo: Arc<dyn ModelRepository>,
}

impl ModelService {
    /// Create a new ModelService with the given repository.
    pub fn new(repo: Arc<dyn ModelRepository>) -> Self {
        Self { repo }
    }

    /// List all models.
    pub async fn list(&self) -> Result<Vec<Model>, CoreError> {
        self.repo.list().await.map_err(CoreError::Repository)
    }

    /// Get a model by ID.
    pub async fn get(&self, id: i64) -> Result<Model, CoreError> {
        self.repo.get_by_id(id).await.map_err(CoreError::Repository)
    }

    /// Get a model by ID (alias for compatibility).
    pub async fn get_by_id(&self, id: i64) -> Result<Model, CoreError> {
        self.get(id).await
    }

    /// Get a model by name (exact match).
    pub async fn get_by_name(&self, name: &str) -> Result<Model, CoreError> {
        self.repo
            .get_by_name(name)
            .await
            .map_err(CoreError::Repository)
    }

    /// Find a model by identifier (ID or name).
    ///
    /// First tries to parse as numeric ID, then searches by exact name match.
    pub async fn find_by_identifier(&self, identifier: &str) -> Result<Option<Model>, CoreError> {
        // Try parsing as ID first
        if let Ok(id) = identifier.parse::<i64>() {
            match self.repo.get_by_id(id).await {
                Ok(model) => return Ok(Some(model)),
                Err(_) => {} // Fall through to name search
            }
        }

        // Try exact name match
        match self.repo.get_by_name(identifier).await {
            Ok(model) => Ok(Some(model)),
            Err(_) => Ok(None),
        }
    }

    /// Find models by partial name match.
    ///
    /// Returns all models whose names contain the search string (case-insensitive).
    pub async fn find_by_name(&self, name: &str) -> Result<Vec<Model>, CoreError> {
        // Get all models and filter by name containing the search string
        let all_models = self.repo.list().await.map_err(CoreError::Repository)?;
        let name_lower = name.to_lowercase();
        Ok(all_models
            .into_iter()
            .filter(|m| m.name.to_lowercase().contains(&name_lower))
            .collect())
    }

    /// Add a new model.
    pub async fn add(&self, model: &NewModel) -> Result<Model, CoreError> {
        self.repo.insert(model).await.map_err(CoreError::Repository)
    }

    /// Update an existing model.
    pub async fn update(&self, model: &Model) -> Result<(), CoreError> {
        self.repo.update(model).await.map_err(CoreError::Repository)
    }

    /// Delete a model by ID.
    pub async fn delete(&self, id: i64) -> Result<(), CoreError> {
        self.repo.delete(id).await.map_err(CoreError::Repository)
    }

    /// Remove a model by ID (alias for delete).
    pub async fn remove(&self, id: i64) -> Result<(), CoreError> {
        self.delete(id).await
    }
}
