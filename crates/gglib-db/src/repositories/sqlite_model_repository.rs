//! `SQLite` implementation of the `ModelRepository` trait.

use async_trait::async_trait;
use sqlx::SqlitePool;

use gglib_core::utils::shard_filename::base_shard_filename;
use gglib_core::{Model, ModelRepository, NewModel, RepositoryError};

use super::row_mappers::{BENCHMARK_SUMMARY_COLUMNS, MODEL_SELECT_COLUMNS, normalized_file_path_string, row_to_model};

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
        // Include benchmark summary via LEFT JOIN so model cards can show
        // speed badges without a separate round-trip.
        let query = format!(
            "SELECT {}, {} FROM models \
             LEFT JOIN model_benchmark_summaries s ON s.model_id = models.id \
             ORDER BY models.added_at DESC",
            MODEL_SELECT_COLUMNS, BENCHMARK_SUMMARY_COLUMNS
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

        // Serialize inference_defaults if present
        let inference_defaults_json = model
            .inference_defaults
            .as_ref()
            .and_then(|cfg| serde_json::to_string(cfg).ok());

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
                context_length, expert_count, expert_used_count, expert_shared_count,
                metadata, added_at, hf_repo_id, hf_commit_sha, 
                hf_filename, download_date, last_update_check, tags, model_key, file_paths_json, capabilities, inference_defaults
            ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
            ON CONFLICT(model_key) DO UPDATE SET
                file_path = excluded.file_path,
                file_paths_json = excluded.file_paths_json,
                quantization = COALESCE(excluded.quantization, models.quantization),
                context_length = COALESCE(excluded.context_length, models.context_length),
                expert_count = COALESCE(excluded.expert_count, models.expert_count),
                expert_used_count = COALESCE(excluded.expert_used_count, models.expert_used_count),
                expert_shared_count = COALESCE(excluded.expert_shared_count, models.expert_shared_count),
                download_date = excluded.download_date,
                last_update_check = excluded.last_update_check,
                tags = excluded.tags,
                capabilities = excluded.capabilities,
                inference_defaults = excluded.inference_defaults
            "#,
        )
        .bind(&model.name)
        .bind(&file_path_string)
        .bind(model.param_count_b)
        .bind(&model.architecture)
        .bind(&model.quantization)
        .bind(model.context_length.map(|c| c as i64))
        .bind(model.expert_count.map(|c| c as i64))
        .bind(model.expert_used_count.map(|c| c as i64))
        .bind(model.expert_shared_count.map(|c| c as i64))
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
        .bind(&inference_defaults_json)
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

        let inference_defaults_json = model
            .inference_defaults
            .as_ref()
            .and_then(|cfg| serde_json::to_string(cfg).ok());

        let result = sqlx::query(
            "UPDATE models SET name = ?, file_path = ?, param_count_b = ?, architecture = ?, quantization = ?, context_length = ?, metadata = ?, hf_repo_id = ?, hf_commit_sha = ?, hf_filename = ?, download_date = ?, last_update_check = ?, tags = ?, capabilities = ?, inference_defaults = ? WHERE id = ?"
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
            .bind(&inference_defaults_json)
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

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use chrono::Utc;
    use gglib_core::{NewModel, RepositoryError};

    use crate::setup::setup_test_database;

    use super::*;

    // ── Helpers ───────────────────────────────────────────────────────────────

    /// Return a minimal valid [`NewModel`] with a unique-by-`name` path so each
    /// test has an independent model key.
    fn make_model(name: &str) -> NewModel {
        NewModel::new(
            name.to_string(),
            PathBuf::from(format!("/models/{name}.gguf")),
            7.0,
            Utc::now(),
        )
    }

    async fn repo() -> SqliteModelRepository {
        let pool = setup_test_database().await.expect("setup_test_database");
        SqliteModelRepository::new(pool)
    }

    // ── Tests ─────────────────────────────────────────────────────────────────

    #[tokio::test]
    async fn insert_and_list() {
        let repo = repo().await;
        repo.insert(&make_model("Alpha")).await.unwrap();
        assert_eq!(repo.list().await.unwrap().len(), 1);
    }

    #[tokio::test]
    async fn get_by_id_returns_inserted_model() {
        let repo = repo().await;
        let inserted = repo.insert(&make_model("Beta")).await.unwrap();
        let fetched = repo.get_by_id(inserted.id).await.unwrap();
        assert_eq!(fetched.name, "Beta");
    }

    #[tokio::test]
    async fn get_by_id_not_found_returns_error() {
        let repo = repo().await;
        let err = repo.get_by_id(999).await.unwrap_err();
        assert!(matches!(err, RepositoryError::NotFound(_)));
    }

    #[tokio::test]
    async fn get_by_name_returns_inserted_model() {
        let repo = repo().await;
        repo.insert(&make_model("Gamma")).await.unwrap();
        let fetched = repo.get_by_name("Gamma").await.unwrap();
        assert_eq!(fetched.name, "Gamma");
    }

    #[tokio::test]
    async fn get_by_name_not_found_returns_error() {
        let repo = repo().await;
        let err = repo.get_by_name("ghost").await.unwrap_err();
        assert!(matches!(err, RepositoryError::NotFound(_)));
    }

    #[tokio::test]
    async fn update_changes_model_fields() {
        let repo = repo().await;
        let mut model = repo.insert(&make_model("Delta")).await.unwrap();
        model.name = "Delta-v2".to_string();
        repo.update(&model).await.unwrap();
        assert_eq!(repo.get_by_id(model.id).await.unwrap().name, "Delta-v2");
    }

    #[tokio::test]
    async fn delete_removes_model_from_list() {
        let repo = repo().await;
        let model = repo.insert(&make_model("Epsilon")).await.unwrap();
        repo.delete(model.id).await.unwrap();
        assert!(repo.list().await.unwrap().is_empty());
    }

    #[tokio::test]
    async fn delete_not_found_returns_error() {
        let repo = repo().await;
        let err = repo.delete(999).await.unwrap_err();
        assert!(matches!(err, RepositoryError::NotFound(_)));
    }

    #[tokio::test]
    async fn upsert_deduplicates_same_model_key() {
        let repo = repo().await;
        // Two inserts of the same path → same local model_key → UPSERT updates in place.
        repo.insert(&make_model("Zeta")).await.unwrap();
        repo.insert(&make_model("Zeta")).await.unwrap();
        assert_eq!(repo.list().await.unwrap().len(), 1);
    }
}
