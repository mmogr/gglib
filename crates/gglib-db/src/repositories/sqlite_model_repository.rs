//! `SQLite` implementation of the `ModelRepository` trait.

use async_trait::async_trait;
use sqlx::SqlitePool;

use gglib_core::utils::shard_filename::base_shard_filename;
use gglib_core::{Model, ModelRepository, NewModel, RepositoryError};

use super::row_mappers::{MODEL_SELECT_COLUMNS, normalized_file_path_string, row_to_model};

/// Compute a canonical model key for deduplication.
///
/// For HuggingFace models: `hf:<repo_id>@<commit_sha>#<base_filename>`
/// For local models: `local:<file_path_hash>`
///
/// The filename is normalized to remove shard suffixes, ensuring all shards
/// in a group compute the same model_key for proper UPSERT deduplication.
fn compute_model_key(model: &NewModel) -> String {
    match (&model.hf_repo_id, &model.hf_commit_sha, &model.hf_filename) {
        (Some(repo), Some(sha), Some(filename)) => {
            let base = base_shard_filename(filename);
            format!("hf:{}@{}#{}", repo, sha, base)
        }
        _ => {
            // For local models without HF metadata, use file path
            use std::collections::hash_map::DefaultHasher;
            use std::hash::{Hash, Hasher};
            let mut hasher = DefaultHasher::new();
            model.file_path.hash(&mut hasher);
            format!("local:{:x}", hasher.finish())
        }
    }
}

/// `SQLite` implementation of the `ModelRepository` trait.
///
/// This struct holds a connection pool and implements all CRUD operations
/// for models using `SQLite`.
pub struct SqliteModelRepository {
    pool: SqlitePool,
}

impl SqliteModelRepository {
    /// Create a new `SQLite` model repository.
    pub fn new(pool: SqlitePool) -> Self {
        Self { pool }
    }

    /// Get a reference to the underlying pool (for testing/migration only).
    #[cfg(test)]
    pub fn pool(&self) -> &SqlitePool {
        &self.pool
    }
}

#[async_trait]
impl ModelRepository for SqliteModelRepository {
    async fn list(&self) -> Result<Vec<Model>, RepositoryError> {
        let query = format!(
            "SELECT {} FROM models ORDER BY added_at DESC",
            MODEL_SELECT_COLUMNS
        );

        let rows = sqlx::query(&query)
            .fetch_all(&self.pool)
            .await
            .map_err(|e| RepositoryError::Storage(e.to_string()))?;

        rows.iter().map(row_to_model).collect()
    }

    async fn get_by_id(&self, id: i64) -> Result<Model, RepositoryError> {
        let query = format!("SELECT {} FROM models WHERE id = ?", MODEL_SELECT_COLUMNS);

        let row = sqlx::query(&query)
            .bind(id)
            .fetch_optional(&self.pool)
            .await
            .map_err(|e| RepositoryError::Storage(e.to_string()))?
            .ok_or_else(|| RepositoryError::NotFound(format!("Model with ID {id}")))?;

        row_to_model(&row)
    }

    async fn get_by_name(&self, name: &str) -> Result<Model, RepositoryError> {
        let query = format!("SELECT {} FROM models WHERE name = ?", MODEL_SELECT_COLUMNS);

        let row = sqlx::query(&query)
            .bind(name)
            .fetch_optional(&self.pool)
            .await
            .map_err(|e| RepositoryError::Storage(e.to_string()))?
            .ok_or_else(|| RepositoryError::NotFound(format!("Model with name '{name}'")))?;

        row_to_model(&row)
    }

    async fn insert(&self, model: &NewModel) -> Result<Model, RepositoryError> {
        let metadata_json = serde_json::to_string(&model.metadata)
            .map_err(|e| RepositoryError::Serialization(e.to_string()))?;

        let file_path_string = normalized_file_path_string(&model.file_path);

        let tags_json = serde_json::to_string(&model.tags).unwrap_or_else(|_| "[]".to_string());

        // Compute model key for deduplication
        let model_key = compute_model_key(model);

        // Serialize file_paths if present
        let file_paths_json = model
            .file_paths
            .as_ref()
            .and_then(|paths| serde_json::to_string(paths).ok());

        // Use UPSERT to make registration idempotent
        let _result = sqlx::query(
            r#"INSERT INTO models (
                name, file_path, param_count_b, architecture, quantization, 
                context_length, metadata, added_at, hf_repo_id, hf_commit_sha, 
                hf_filename, download_date, last_update_check, tags, model_key, file_paths_json, capabilities
            ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
            ON CONFLICT(model_key) DO UPDATE SET
                file_path = excluded.file_path,
                file_paths_json = excluded.file_paths_json,
                quantization = COALESCE(excluded.quantization, models.quantization),
                download_date = excluded.download_date,
                last_update_check = excluded.last_update_check,
                tags = excluded.tags,
                capabilities = excluded.capabilities
            "#,
        )
        .bind(&model.name)
        .bind(&file_path_string)
        .bind(model.param_count_b)
        .bind(&model.architecture)
        .bind(&model.quantization)
        .bind(model.context_length.map(|c| c as i64))
        .bind(&metadata_json)
        .bind(model.added_at.to_string())
        .bind(&model.hf_repo_id)
        .bind(&model.hf_commit_sha)
        .bind(&model.hf_filename)
        .bind(model.download_date.as_ref().map(|d| d.to_string()))
        .bind(model.last_update_check.as_ref().map(|d| d.to_string()))
        .bind(&tags_json)
        .bind(&model_key)
        .bind(&file_paths_json)
        .bind(model.capabilities.bits() as i64)
        .execute(&self.pool)
        .await
        .map_err(|e| RepositoryError::Storage(e.to_string()))?;

        // Get the model by model_key (works for both insert and update)
        let row = sqlx::query(&format!(
            "SELECT {} FROM models WHERE model_key = ? LIMIT 1",
            MODEL_SELECT_COLUMNS
        ))
        .bind(&model_key)
        .fetch_one(&self.pool)
        .await
        .map_err(|e| RepositoryError::Storage(e.to_string()))?;

        row_to_model(&row)
    }

    async fn update(&self, model: &Model) -> Result<(), RepositoryError> {
        let metadata_json = serde_json::to_string(&model.metadata)
            .map_err(|e| RepositoryError::Serialization(e.to_string()))?;

        let tags_json = serde_json::to_string(&model.tags)
            .map_err(|e| RepositoryError::Serialization(e.to_string()))?;

        let result = sqlx::query(
            "UPDATE models SET name = ?, file_path = ?, param_count_b = ?, architecture = ?, quantization = ?, context_length = ?, metadata = ?, hf_repo_id = ?, hf_commit_sha = ?, hf_filename = ?, download_date = ?, last_update_check = ?, tags = ?, capabilities = ? WHERE id = ?"
        )
            .bind(&model.name)
            .bind(model.file_path.to_string_lossy().as_ref())
            .bind(model.param_count_b)
            .bind(&model.architecture)
            .bind(&model.quantization)
            .bind(model.context_length.map(|c| c as i64))
            .bind(&metadata_json)
            .bind(&model.hf_repo_id)
            .bind(&model.hf_commit_sha)
            .bind(&model.hf_filename)
            .bind(model.download_date.as_ref().map(|dt| dt.to_string()))
            .bind(model.last_update_check.as_ref().map(|dt| dt.to_string()))
            .bind(&tags_json)
            .bind(model.capabilities.bits() as i64)
            .bind(model.id)
            .execute(&self.pool)
            .await
            .map_err(|e| RepositoryError::Storage(e.to_string()))?;

        if result.rows_affected() == 0 {
            return Err(RepositoryError::NotFound(format!(
                "Model with ID {}",
                model.id
            )));
        }

        Ok(())
    }

    async fn delete(&self, id: i64) -> Result<(), RepositoryError> {
        let result = sqlx::query("DELETE FROM models WHERE id = ?")
            .bind(id)
            .execute(&self.pool)
            .await
            .map_err(|e| RepositoryError::Storage(e.to_string()))?;

        if result.rows_affected() == 0 {
            return Err(RepositoryError::NotFound(format!("Model with ID {id}")));
        }

        Ok(())
    }
}
