//! Database error types for model storage operations.

use thiserror::Error;

/// Domain-specific errors for model storage operations.
#[derive(Debug, Error)]
pub enum ModelStoreError {
    /// A model with the same file path already exists in the database.
    #[error(
        "Model '{model_name}' is already tracked (id {existing_id}) for file {file_path}. Remove it before downloading again."
    )]
    DuplicateModel {
        model_name: String,
        file_path: String,
        existing_id: u32,
    },

    /// The requested model was not found by ID.
    #[error("Model with ID {id} not found")]
    NotFound { id: u32 },

    /// Database operation failed.
    #[error("Database error: {0}")]
    Database(#[from] sqlx::Error),

    /// JSON serialization/deserialization failed.
    #[error("Serialization error: {0}")]
    Serialization(#[from] serde_json::Error),
}
