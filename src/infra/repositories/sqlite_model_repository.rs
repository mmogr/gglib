//! SQLite implementation of the ModelRepository trait.
//!
//! This implementation encapsulates all SQL queries for model persistence.
//! The `SqlitePool` is kept private and never exposed through the trait.

use async_trait::async_trait;
use chrono::{DateTime, NaiveDateTime, Utc};
use sqlx::{Row, SqlitePool};
use std::path::Path;

use crate::core::domain::{Model, NewModel};
use crate::core::ports::RepositoryError;
use crate::core::ports::model_repository::ModelRepository;

/// Shared SELECT column list for model queries.
const MODEL_SELECT_COLUMNS: &str = "id, name, file_path, param_count_b, architecture, quantization, context_length, metadata, added_at, hf_repo_id, hf_commit_sha, hf_filename, download_date, last_update_check, tags";

/// SQLite implementation of the ModelRepository trait.
///
/// This struct holds a connection pool and implements all CRUD operations
/// for models using SQLite.
pub struct SqliteModelRepository {
    pool: SqlitePool,
}

impl SqliteModelRepository {
    /// Create a new SQLite model repository.
    ///
    /// The pool is owned by this repository and used for all operations.
    pub fn new(pool: SqlitePool) -> Self {
        Self { pool }
    }

    /// Get a reference to the underlying pool (for testing/migration only).
    #[cfg(test)]
    pub fn pool(&self) -> &SqlitePool {
        &self.pool
    }
}

/// Helper to parse datetime strings that may have "UTC" suffix.
fn parse_datetime(datetime_str: Option<String>) -> Option<DateTime<Utc>> {
    datetime_str.and_then(|s| {
        let trimmed = s.trim_end_matches(" UTC");
        NaiveDateTime::parse_from_str(trimmed, "%Y-%m-%d %H:%M:%S%.f")
            .map(|dt| DateTime::<Utc>::from_naive_utc_and_offset(dt, Utc))
            .ok()
    })
}

/// Parse a database row into a Model.
fn row_to_model(row: &sqlx::sqlite::SqliteRow) -> Result<Model, RepositoryError> {
    let context_length: Option<u64> = row
        .try_get::<Option<i64>, _>("context_length")
        .map_err(|e| RepositoryError::Storage(e.to_string()))?
        .map(|v| v as u64);

    let metadata_json: String = row
        .try_get("metadata")
        .map_err(|e| RepositoryError::Storage(e.to_string()))?;

    let tags_json: String = row
        .try_get("tags")
        .map_err(|e| RepositoryError::Storage(e.to_string()))?;

    let added_at_str: Option<String> = row
        .try_get("added_at")
        .map_err(|e| RepositoryError::Storage(e.to_string()))?;

    let download_date_str: Option<String> = row
        .try_get("download_date")
        .map_err(|e| RepositoryError::Storage(e.to_string()))?;

    let last_update_check_str: Option<String> = row
        .try_get("last_update_check")
        .map_err(|e| RepositoryError::Storage(e.to_string()))?;

    Ok(Model {
        id: row
            .try_get::<i64, _>("id")
            .map_err(|e| RepositoryError::Storage(e.to_string()))?,
        name: row
            .try_get("name")
            .map_err(|e| RepositoryError::Storage(e.to_string()))?,
        file_path: row
            .try_get::<String, _>("file_path")
            .map_err(|e| RepositoryError::Storage(e.to_string()))?
            .into(),
        param_count_b: row
            .try_get("param_count_b")
            .map_err(|e| RepositoryError::Storage(e.to_string()))?,
        architecture: row
            .try_get("architecture")
            .map_err(|e| RepositoryError::Storage(e.to_string()))?,
        quantization: row
            .try_get("quantization")
            .map_err(|e| RepositoryError::Storage(e.to_string()))?,
        context_length,
        metadata: serde_json::from_str(&metadata_json).unwrap_or_default(),
        added_at: parse_datetime(added_at_str).unwrap_or_else(Utc::now),
        hf_repo_id: row
            .try_get("hf_repo_id")
            .map_err(|e| RepositoryError::Storage(e.to_string()))?,
        hf_commit_sha: row
            .try_get("hf_commit_sha")
            .map_err(|e| RepositoryError::Storage(e.to_string()))?,
        hf_filename: row
            .try_get("hf_filename")
            .map_err(|e| RepositoryError::Storage(e.to_string()))?,
        download_date: parse_datetime(download_date_str),
        last_update_check: parse_datetime(last_update_check_str),
        tags: serde_json::from_str(&tags_json).unwrap_or_default(),
    })
}

/// Normalizes a file path to a canonical string representation.
fn normalized_file_path_string(path: &Path) -> String {
    std::fs::canonicalize(path)
        .map(|p| p.to_string_lossy().to_string())
        .unwrap_or_else(|_| path.to_string_lossy().to_string())
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

        // Check for existing model with same file path
        let existing =
            sqlx::query("SELECT id, name, file_path FROM models WHERE file_path = ? LIMIT 1")
                .bind(&file_path_string)
                .fetch_optional(&self.pool)
                .await
                .map_err(|e| RepositoryError::Storage(e.to_string()))?;

        if let Some(row) = existing {
            let existing_name: String = row.get("name");
            return Err(RepositoryError::AlreadyExists(format!(
                "Model '{}' already exists at path {}",
                existing_name, file_path_string
            )));
        }

        let tags_json = serde_json::to_string(&model.tags).unwrap_or_else(|_| "[]".to_string());

        let result = sqlx::query(
            "INSERT INTO models (name, file_path, param_count_b, architecture, quantization, context_length, metadata, added_at, hf_repo_id, hf_commit_sha, hf_filename, download_date, last_update_check, tags) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)"
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
            .execute(&self.pool)
            .await
            .map_err(|e| RepositoryError::Storage(e.to_string()))?;

        let id = result.last_insert_rowid();

        // Fetch and return the inserted model
        self.get_by_id(id).await
    }

    async fn update(&self, model: &Model) -> Result<(), RepositoryError> {
        let metadata_json = serde_json::to_string(&model.metadata)
            .map_err(|e| RepositoryError::Serialization(e.to_string()))?;

        let tags_json = serde_json::to_string(&model.tags)
            .map_err(|e| RepositoryError::Serialization(e.to_string()))?;

        let result = sqlx::query(
            "UPDATE models SET name = ?, file_path = ?, param_count_b = ?, architecture = ?, quantization = ?, context_length = ?, metadata = ?, hf_repo_id = ?, hf_commit_sha = ?, hf_filename = ?, download_date = ?, last_update_check = ?, tags = ? WHERE id = ?"
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
    use super::*;
    use std::collections::HashMap;
    use std::path::PathBuf;

    async fn create_test_pool() -> SqlitePool {
        let pool = SqlitePool::connect("sqlite::memory:").await.unwrap();

        // Create schema
        sqlx::query(
            r#"
            CREATE TABLE IF NOT EXISTS models (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                name TEXT NOT NULL,
                file_path TEXT NOT NULL UNIQUE,
                param_count_b REAL NOT NULL,
                architecture TEXT,
                quantization TEXT,
                context_length INTEGER,
                metadata TEXT NOT NULL DEFAULT '{}',
                added_at TEXT NOT NULL,
                hf_repo_id TEXT,
                hf_commit_sha TEXT,
                hf_filename TEXT,
                download_date TEXT,
                last_update_check TEXT,
                tags TEXT NOT NULL DEFAULT '[]'
            )
            "#,
        )
        .execute(&pool)
        .await
        .unwrap();

        pool
    }

    fn create_test_new_model(name: &str) -> NewModel {
        NewModel {
            name: name.to_string(),
            file_path: PathBuf::from(format!("/path/to/{}.gguf", name)),
            param_count_b: 7.0,
            architecture: Some("llama".to_string()),
            quantization: Some("Q4_0".to_string()),
            context_length: Some(4096),
            metadata: HashMap::new(),
            added_at: Utc::now(),
            hf_repo_id: None,
            hf_commit_sha: None,
            hf_filename: None,
            download_date: None,
            last_update_check: None,
            tags: vec!["test".to_string()],
        }
    }

    #[tokio::test]
    async fn test_insert_and_get_by_id() {
        let pool = create_test_pool().await;
        let repo = SqliteModelRepository::new(pool);

        let new_model = create_test_new_model("test-model");
        let model = repo.insert(&new_model).await.unwrap();

        assert_eq!(model.name, "test-model");
        assert!(model.id > 0);

        let fetched = repo.get_by_id(model.id).await.unwrap();
        assert_eq!(fetched.name, "test-model");
    }

    #[tokio::test]
    async fn test_get_by_name() {
        let pool = create_test_pool().await;
        let repo = SqliteModelRepository::new(pool);

        let new_model = create_test_new_model("named-model");
        repo.insert(&new_model).await.unwrap();

        let fetched = repo.get_by_name("named-model").await.unwrap();
        assert_eq!(fetched.name, "named-model");
    }

    #[tokio::test]
    async fn test_list() {
        let pool = create_test_pool().await;
        let repo = SqliteModelRepository::new(pool);

        repo.insert(&create_test_new_model("model-1"))
            .await
            .unwrap();
        repo.insert(&create_test_new_model("model-2"))
            .await
            .unwrap();

        let models = repo.list().await.unwrap();
        assert_eq!(models.len(), 2);
    }

    #[tokio::test]
    async fn test_update() {
        let pool = create_test_pool().await;
        let repo = SqliteModelRepository::new(pool);

        let new_model = create_test_new_model("update-test");
        let mut model = repo.insert(&new_model).await.unwrap();

        model.name = "updated-name".to_string();
        repo.update(&model).await.unwrap();

        let fetched = repo.get_by_id(model.id).await.unwrap();
        assert_eq!(fetched.name, "updated-name");
    }

    #[tokio::test]
    async fn test_delete() {
        let pool = create_test_pool().await;
        let repo = SqliteModelRepository::new(pool);

        let new_model = create_test_new_model("delete-test");
        let model = repo.insert(&new_model).await.unwrap();

        repo.delete(model.id).await.unwrap();

        let result = repo.get_by_id(model.id).await;
        assert!(matches!(result, Err(RepositoryError::NotFound(_))));
    }

    #[tokio::test]
    async fn test_duplicate_insert_fails() {
        let pool = create_test_pool().await;
        let repo = SqliteModelRepository::new(pool);

        let new_model = create_test_new_model("duplicate-test");
        repo.insert(&new_model).await.unwrap();

        let result = repo.insert(&new_model).await;
        assert!(matches!(result, Err(RepositoryError::AlreadyExists(_))));
    }
}
