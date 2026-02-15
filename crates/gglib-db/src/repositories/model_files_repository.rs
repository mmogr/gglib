//! Repository for managing model files in the database.
//!
//! This repository handles CRUD operations for model file entries,
//! which track per-shard metadata including OIDs for verification.

use anyhow::Result;
use chrono::{DateTime, Utc};
use gglib_core::domain::{ModelFile, NewModelFile};
use sqlx::SqlitePool;

use super::row_mappers::map_model_file_row;

/// Repository for model file database operations.
#[derive(Clone)]
pub struct ModelFilesRepository {
    pool: SqlitePool,
}

// Implement the trait from gglib_core
#[async_trait::async_trait]
impl gglib_core::services::ModelFilesRepositoryPort for ModelFilesRepository {
    async fn insert(&self, file: &NewModelFile) -> anyhow::Result<()> {
        sqlx::query(
            r#"
            INSERT INTO model_files 
                (model_id, file_path, file_index, expected_size, hf_oid)
            VALUES (?, ?, ?, ?, ?)
            "#,
        )
        .bind(file.model_id)
        .bind(&file.file_path)
        .bind(file.file_index)
        .bind(file.expected_size)
        .bind(&file.hf_oid)
        .execute(&self.pool)
        .await
        .map_err(|e: sqlx::Error| anyhow::Error::from(e))?;
        
        Ok(())
    }
}

// Implement the reader trait for verification service
#[async_trait::async_trait]
impl gglib_core::services::ModelFilesReaderPort for ModelFilesRepository {
    async fn get_by_model_id(&self, model_id: i64) -> anyhow::Result<Vec<ModelFile>> {
        self.get_by_model_id(model_id).await
    }
    
    async fn update_verification_time(
        &self,
        id: i64,
        verified_at: chrono::DateTime<chrono::Utc>,
    ) -> anyhow::Result<()> {
        self.update_verification_time(id, verified_at).await
    }
}

impl ModelFilesRepository {
    /// Create a new `ModelFilesRepository`.
    pub fn new(pool: SqlitePool) -> Self {
        Self { pool }
    }

    /// Insert a new model file entry.
    ///
    /// Returns the ID of the inserted row.
    pub async fn insert_with_id(&self, file: &NewModelFile) -> Result<i64> {
        let result = sqlx::query(
            r#"
            INSERT INTO model_files 
                (model_id, file_path, file_index, expected_size, hf_oid)
            VALUES (?, ?, ?, ?, ?)
            "#,
        )
        .bind(file.model_id)
        .bind(&file.file_path)
        .bind(file.file_index)
        .bind(file.expected_size)
        .bind(&file.hf_oid)
        .execute(&self.pool)
        .await?;

        Ok(result.last_insert_rowid())
    }

    /// Get all model files for a specific model.
    ///
    /// Returns files ordered by file_index.
    pub async fn get_by_model_id(&self, model_id: i64) -> Result<Vec<ModelFile>> {
        let rows = sqlx::query(
            r#"
            SELECT id, model_id, file_path, file_index, expected_size, hf_oid, last_verified_at
            FROM model_files
            WHERE model_id = ?
            ORDER BY file_index ASC
            "#,
        )
        .bind(model_id)
        .fetch_all(&self.pool)
        .await?;

        rows.iter()
            .map(|row| map_model_file_row(row).map_err(Into::into))
            .collect()
    }

    /// Update the last_verified_at timestamp for a model file.
    pub async fn update_verification_time(
        &self,
        id: i64,
        timestamp: DateTime<Utc>,
    ) -> Result<()> {
        sqlx::query(
            r#"
            UPDATE model_files
            SET last_verified_at = ?
            WHERE id = ?
            "#,
        )
        .bind(timestamp.to_rfc3339())
        .bind(id)
        .execute(&self.pool)
        .await?;

        Ok(())
    }

    /// Delete all model files for a specific model.
    ///
    /// This is typically called when a model is being deleted (cascade delete
    /// handles this automatically, but this method is provided for explicit cleanup).
    pub async fn delete_by_model_id(&self, model_id: i64) -> Result<()> {
        sqlx::query("DELETE FROM model_files WHERE model_id = ?")
            .bind(model_id)
            .execute(&self.pool)
            .await?;

        Ok(())
    }

    /// Get a specific model file by ID.
    pub async fn get_by_id(&self, id: i64) -> Result<Option<ModelFile>> {
        let row = sqlx::query(
            r#"
            SELECT id, model_id, file_path, file_index, expected_size, hf_oid, last_verified_at
            FROM model_files
            WHERE id = ?
            "#,
        )
        .bind(id)
        .fetch_optional(&self.pool)
        .await?;

        Ok(row
            .as_ref()
            .map(map_model_file_row)
            .transpose()
            .map_err(|e: sqlx::Error| anyhow::Error::from(e))?)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::setup::setup_test_database;
    use gglib_core::domain::NewModel;
    use gglib_core::services::ModelFilesRepositoryPort;
    use std::path::PathBuf;

    async fn setup_test_model(pool: &SqlitePool) -> Result<i64> {
        use crate::repositories::SqliteModelRepository;
        use gglib_core::ModelRepository;
        
        let model_repo = SqliteModelRepository::new(pool.clone());
        let new_model = NewModel::new(
            "Test Model".to_string(),
            PathBuf::from("/tmp/test.gguf"),
            7.0,
            Utc::now(),
        );
        
        let model = model_repo.insert(&new_model).await?;
        Ok(model.id)
    }

    #[tokio::test]
    async fn test_insert_and_get_model_files() {
        let pool = setup_test_database().await.unwrap();
        let model_id = setup_test_model(&pool).await.unwrap();
        let repo = ModelFilesRepository::new(pool);

        // Insert a model file
        let new_file = NewModelFile::new(
            model_id,
            "model.gguf".to_string(),
            0,
            1024 * 1024 * 100, // 100MB
            Some("abc123def456".to_string()),
        );

        repo.insert(&new_file).await.unwrap();

        // Get files by model ID
        let files = repo.get_by_model_id(model_id).await.unwrap();
        assert_eq!(files.len(), 1);
        assert_eq!(files[0].file_path, "model.gguf");
        assert_eq!(files[0].hf_oid, Some("abc123def456".to_string()));
    }

    #[tokio::test]
    async fn test_update_verification_time() {
        let pool = setup_test_database().await.unwrap();
        let model_id = setup_test_model(&pool).await.unwrap();
        let repo = ModelFilesRepository::new(pool);

        let new_file = NewModelFile::new(
            model_id,
            "model.gguf".to_string(),
            0,
            1024,
            None,
        );

        repo.insert(&new_file).await.unwrap();

        // Get the file ID from the database
        let files = repo.get_by_model_id(model_id).await.unwrap();
        assert_eq!(files.len(), 1);
        let file_id = files[0].id;

        // Update verification time
        let now = Utc::now();
        repo.update_verification_time(file_id, now).await.unwrap();

        // Verify it was updated
        let file = repo.get_by_id(file_id).await.unwrap().unwrap();
        assert!(file.last_verified_at.is_some());
    }

    #[tokio::test]
    async fn test_delete_by_model_id() {
        let pool = setup_test_database().await.unwrap();
        let model_id = setup_test_model(&pool).await.unwrap();
        let repo = ModelFilesRepository::new(pool);

        // Insert multiple files
        for i in 0..3 {
            let new_file = NewModelFile::new(
                model_id,
                format!("shard-{}.gguf", i),
                i as i32,
                1024,
                None,
            );
            repo.insert(&new_file).await.unwrap();
        }

        // Verify they exist
        let files = repo.get_by_model_id(model_id).await.unwrap();
        assert_eq!(files.len(), 3);

        // Delete all
        repo.delete_by_model_id(model_id).await.unwrap();

        // Verify they're gone
        let files = repo.get_by_model_id(model_id).await.unwrap();
        assert_eq!(files.len(), 0);
    }
}
