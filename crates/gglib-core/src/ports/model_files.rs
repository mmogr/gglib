//! Model files repository trait definition.
//!
//! This port defines the interface for the rows that record each file of a
//! model: its path, its size, its `HuggingFace` OID and when it was last
//! verified. Implementations must handle all storage details internally.

use async_trait::async_trait;
use chrono::{DateTime, Utc};

use super::RepositoryError;
use crate::domain::{ModelFile, NewModelFile};

/// Repository for the files of each model.
///
/// The registrar stores a row per downloaded file. Verification, the update
/// check and repair read those rows.
#[async_trait]
pub trait ModelFilesRepositoryPort: Send + Sync {
    /// Store a model file record, replacing the one held for that model and path.
    async fn insert(&self, model_file: &NewModelFile) -> Result<(), RepositoryError>;

    /// Get the files of model `model_id`, in file order.
    async fn get_by_model_id(&self, model_id: i64) -> Result<Vec<ModelFile>, RepositoryError>;

    /// Record `verified_at` as the time model file `id` was last verified.
    async fn update_verification_time(
        &self,
        id: i64,
        verified_at: DateTime<Utc>,
    ) -> Result<(), RepositoryError>;
}
