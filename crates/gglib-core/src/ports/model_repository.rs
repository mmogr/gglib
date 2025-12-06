//! Model repository trait definition.
//!
//! This port defines the interface for model persistence operations.
//! Implementations must handle all storage details internally.

use async_trait::async_trait;

use super::RepositoryError;
use crate::domain::{Model, NewModel};

/// Repository for model persistence operations.
///
/// This trait defines CRUD operations for models. Implementations
/// are responsible for all storage details (SQL, filesystem, etc.).
///
/// # Design Rules
///
/// - No `sqlx` types in signatures
/// - CRUD-only: list, get, insert, update, delete
/// - Tags and search logic belong in `ModelService`, not here
#[async_trait]
pub trait ModelRepository: Send + Sync {
    /// List all models in the repository.
    async fn list(&self) -> Result<Vec<Model>, RepositoryError>;

    /// Get a model by its database ID.
    ///
    /// Returns `Err(RepositoryError::NotFound)` if the model doesn't exist.
    async fn get_by_id(&self, id: i64) -> Result<Model, RepositoryError>;

    /// Get a model by its name.
    ///
    /// Returns `Err(RepositoryError::NotFound)` if no model with that name exists.
    async fn get_by_name(&self, name: &str) -> Result<Model, RepositoryError>;

    /// Insert a new model into the repository.
    ///
    /// Returns the persisted model with its assigned ID.
    /// Returns `Err(RepositoryError::AlreadyExists)` if a model with the same
    /// file path already exists.
    async fn insert(&self, model: &NewModel) -> Result<Model, RepositoryError>;

    /// Update an existing model.
    ///
    /// Returns `Err(RepositoryError::NotFound)` if the model doesn't exist.
    async fn update(&self, model: &Model) -> Result<(), RepositoryError>;

    /// Delete a model by its database ID.
    ///
    /// Returns `Err(RepositoryError::NotFound)` if the model doesn't exist.
    async fn delete(&self, id: i64) -> Result<(), RepositoryError>;
}
