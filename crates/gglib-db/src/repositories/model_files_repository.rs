//! Repository for managing model files in the database.
//!
//! This repository handles CRUD operations for model file entries,
//! which track per-shard metadata including OIDs for verification.

use anyhow::Result;
use chrono::{DateTime, Utc};
use gglib_core::domain::{ModelFile, NewModelFile};
use gglib_core::ports::{ModelFilesRepositoryPort, RepositoryError};
use sqlx::SqlitePool;

use super::row_mappers::map_model_file_row;

/// Repository for model file database operations.
#[derive(Clone)]
pub struct ModelFilesRepository {
    pool: SqlitePool,
}

/// A failure of the database, in its own words.
fn storage(e: &sqlx::Error) -> RepositoryError {
    RepositoryError::Storage(e.to_string())
}

#[async_trait::async_trait]
impl ModelFilesRepositoryPort for ModelFilesRepository {
    /// A model downloaded again, by a repair or an update, already has a row
    /// per file. That row takes the new index, size and OID, and the time it
    /// was verified is kept only while the OID is the one verified.
    async fn insert(&self, file: &NewModelFile) -> Result<(), RepositoryError> {
        sqlx::query(
            r"
            INSERT INTO model_files 
                (model_id, file_path, file_index, expected_size, hf_oid)
            VALUES (?, ?, ?, ?, ?)
            ON CONFLICT(model_id, file_path) DO UPDATE SET
                file_index = excluded.file_index,
                expected_size = excluded.expected_size,
                last_verified_at = CASE
                    WHEN model_files.hf_oid IS excluded.hf_oid THEN model_files.last_verified_at
                END,
                hf_oid = excluded.hf_oid
            ",
        )
        .bind(file.model_id)
        .bind(&file.file_path)
        .bind(file.file_index)
        .bind(file.expected_size)
        .bind(&file.hf_oid)
        .execute(&self.pool)
        .await
        .map_err(|e| storage(&e))?;

        Ok(())
    }

    /// Returns files ordered by `file_index`.
    async fn get_by_model_id(&self, model_id: i64) -> Result<Vec<ModelFile>, RepositoryError> {
        let rows = sqlx::query(
            r"
            SELECT id, model_id, file_path, file_index, expected_size, hf_oid, last_verified_at
            FROM model_files
            WHERE model_id = ?
            ORDER BY file_index ASC
            ",
        )
        .bind(model_id)
        .fetch_all(&self.pool)
        .await
        .map_err(|e| storage(&e))?;

        rows.iter()
            .map(|row| map_model_file_row(row).map_err(|e| storage(&e)))
            .collect()
    }

    async fn update_verification_time(
        &self,
        id: i64,
        verified_at: DateTime<Utc>,
    ) -> Result<(), RepositoryError> {
        sqlx::query(
            r"
            UPDATE model_files
            SET last_verified_at = ?
            WHERE id = ?
            ",
        )
        .bind(verified_at.to_rfc3339())
        .bind(id)
        .execute(&self.pool)
        .await
        .map_err(|e| storage(&e))?;

        Ok(())
    }
}

impl ModelFilesRepository {
    /// Create a new `ModelFilesRepository`.
    pub fn new(pool: SqlitePool) -> Self {
        Self { pool }
    }

    /// Get a specific model file by ID.
    pub async fn get_by_id(&self, id: i64) -> Result<Option<ModelFile>> {
        let row = sqlx::query(
            r"
            SELECT id, model_id, file_path, file_index, expected_size, hf_oid, last_verified_at
            FROM model_files
            WHERE id = ?
            ",
        )
        .bind(id)
        .fetch_optional(&self.pool)
        .await?;

        row.as_ref()
            .map(map_model_file_row)
            .transpose()
            .map_err(|e: sqlx::Error| anyhow::Error::from(e))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::setup::setup_test_database;
    use gglib_core::domain::NewModel;
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

        let new_file = NewModelFile::new(model_id, "model.gguf".to_string(), 0, 1024, None);

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

    /// A repair or an update registers the model's files again: the row is
    /// the same row, with the OID and size of what was fetched, and it is no
    /// longer verified.
    #[tokio::test]
    async fn a_file_inserted_again_takes_the_new_oid_and_is_unverified() {
        let pool = setup_test_database().await.unwrap();
        let model_id = setup_test_model(&pool).await.unwrap();
        let repo = ModelFilesRepository::new(pool);
        let file = |index, size, oid: &str| {
            NewModelFile::new(
                model_id,
                "mmproj-F16.gguf".to_string(),
                index,
                size,
                Some(oid.to_string()),
            )
        };
        repo.insert(&file(1, 100, "old")).await.unwrap();
        let first = repo.get_by_model_id(model_id).await.unwrap().remove(0);
        repo.update_verification_time(first.id, Utc::now())
            .await
            .unwrap();

        repo.insert(&file(2, 120, "new")).await.unwrap();

        let rows = repo.get_by_model_id(model_id).await.unwrap();
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].id, first.id);
        assert_eq!(rows[0].hf_oid.as_deref(), Some("new"));
        assert_eq!((rows[0].file_index, rows[0].expected_size), (2, 120));
        assert!(rows[0].last_verified_at.is_none());
    }

    #[tokio::test]
    async fn a_file_inserted_again_unchanged_stays_verified() {
        let pool = setup_test_database().await.unwrap();
        let model_id = setup_test_model(&pool).await.unwrap();
        let repo = ModelFilesRepository::new(pool);
        let file = NewModelFile::new(model_id, "m.gguf".to_string(), 0, 100, Some("oid".into()));
        repo.insert(&file).await.unwrap();
        let first = repo.get_by_model_id(model_id).await.unwrap().remove(0);
        repo.update_verification_time(first.id, Utc::now())
            .await
            .unwrap();

        repo.insert(&file).await.unwrap();

        let rows = repo.get_by_model_id(model_id).await.unwrap();
        assert_eq!(rows.len(), 1);
        assert!(rows[0].last_verified_at.is_some());
    }

    /// What the database says of a failure is the whole of the error: a
    /// caller that reports it adds only the `Storage` label.
    #[tokio::test]
    async fn a_failure_of_the_database_is_a_storage_error_in_its_words() {
        let pool = setup_test_database().await.unwrap();
        let model_id = setup_test_model(&pool).await.unwrap();
        sqlx::query("DROP TABLE model_files")
            .execute(&pool)
            .await
            .unwrap();
        let repo = ModelFilesRepository::new(pool);
        let file = NewModelFile::new(model_id, "m.gguf".to_string(), 0, 100, None);

        let failures = [
            repo.insert(&file).await.unwrap_err(),
            repo.get_by_model_id(model_id).await.unwrap_err(),
            repo.update_verification_time(1, Utc::now())
                .await
                .unwrap_err(),
        ];

        let said = "error returned from database: (code: 1) no such table: model_files";
        for failure in failures {
            assert!(
                matches!(&failure, RepositoryError::Storage(why) if why == said),
                "{failure:?}"
            );
        }
    }

    /// The table is not `STRICT`: a column can hold what a row cannot take.
    #[tokio::test]
    async fn a_row_that_cannot_be_decoded_is_a_storage_error_in_its_words() {
        let pool = setup_test_database().await.unwrap();
        let model_id = setup_test_model(&pool).await.unwrap();
        sqlx::query(
            "INSERT INTO model_files (model_id, file_path, file_index, expected_size)
             VALUES (?, 'm.gguf', 'not a number', 1)",
        )
        .bind(model_id)
        .execute(&pool)
        .await
        .unwrap();
        let repo = ModelFilesRepository::new(pool);

        let failure = repo.get_by_model_id(model_id).await.unwrap_err();

        let said = "error occurred while decoding column \"file_index\": mismatched types; \
                    Rust type `i32` (as SQL type `INTEGER`) is not compatible with SQL type `TEXT`";
        assert_eq!(failure.to_string(), format!("Storage error: {said}"));
    }
}
