//! `SQLite` implementation of the `ModelRepository` trait.

use async_trait::async_trait;
use sqlx::SqlitePool;

use gglib_core::utils::shard_filename::base_shard_filename;
use gglib_core::{Model, ModelRepository, NewModel, RepositoryError};

use super::row_mappers::{
    BENCHMARK_SUMMARY_COLUMNS, MODEL_SELECT_COLUMNS, normalized_file_path_string, row_to_model,
};

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
        // ORDER BY id makes resolution deterministic when two rows share a
        // name (e.g. two quants of the same repo, or two repos that declare
        // the same general.name) instead of depending on SQLite storage order.
        let query = format!(
            "SELECT {} FROM models WHERE name = ? ORDER BY models.id LIMIT 1",
            MODEL_SELECT_COLUMNS
        );

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

        // Serialize defaults_origin if present
        let defaults_origin_str = model.defaults_origin.map(|o| o.to_string());

        // Serialize server_defaults if present
        let server_defaults_json = model
            .server_defaults
            .as_ref()
            .and_then(|cfg| serde_json::to_string(cfg).ok());

        // Serialize dialect_spec if present
        let dialect_spec_json = model
            .dialect_spec
            .as_ref()
            .and_then(|spec| serde_json::to_string(spec).ok());

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
                hf_filename, download_date, last_update_check, tags, model_key, file_paths_json, capabilities, inference_defaults, defaults_origin, server_defaults, dialect_spec
            ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
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
                dialect_spec = excluded.dialect_spec,
                inference_defaults = COALESCE(models.inference_defaults, excluded.inference_defaults),
                defaults_origin = COALESCE(models.defaults_origin, excluded.defaults_origin)
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
        .bind(&defaults_origin_str)
        .bind(&server_defaults_json)
        .bind(&dialect_spec_json)
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

        let defaults_origin_str = model.defaults_origin.map(|o| o.to_string());

        let server_defaults_json = model
            .server_defaults
            .as_ref()
            .and_then(|cfg| serde_json::to_string(cfg).ok());

        let dialect_spec_json = model
            .dialect_spec
            .as_ref()
            .and_then(|spec| serde_json::to_string(spec).ok());

        let result = sqlx::query(
            "UPDATE models SET name = ?, file_path = ?, param_count_b = ?, architecture = ?, quantization = ?, context_length = ?, metadata = ?, hf_repo_id = ?, hf_commit_sha = ?, hf_filename = ?, download_date = ?, last_update_check = ?, tags = ?, capabilities = ?, inference_defaults = ?, defaults_origin = ?, server_defaults = ?, dialect_spec = ? WHERE id = ?"
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
            .bind(&defaults_origin_str)
            .bind(&server_defaults_json)
            .bind(&dialect_spec_json)
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

    /// `insert` upserts on the model key: a second registration of the same
    /// model is not an error, it returns the same row. Recorded as a test
    /// because the port doc used to claim the opposite, and the paths that
    /// register a model after a download depend on this being retry-safe.
    #[tokio::test]
    async fn inserting_the_same_model_twice_upserts_rather_than_failing() {
        let repo = repo().await;

        let first = repo.insert(&make_model("Dup")).await.unwrap();
        let second = repo
            .insert(&make_model("Dup"))
            .await
            .expect("a repeat registration must not fail");

        assert_eq!(second.id, first.id, "same row, not a second one");
        assert_eq!(repo.list().await.unwrap().len(), 1);
    }

    /// The lookup `ModelService::import_from_file` asks before inserting, so
    /// that an explicit add of a file already present is a conflict instead of
    /// a silent overwrite.
    #[tokio::test]
    async fn find_by_path_locates_a_model_by_the_path_it_was_stored_under() {
        let repo = repo().await;
        let inserted = repo.insert(&make_model("Findable")).await.unwrap();

        let found = repo
            .find_by_path(&PathBuf::from("/models/Findable.gguf"))
            .await
            .unwrap()
            .expect("the model just inserted");
        assert_eq!(found.id, inserted.id);

        assert!(
            repo.find_by_path(&PathBuf::from("/models/Absent.gguf"))
                .await
                .unwrap()
                .is_none()
        );
    }

    /// The repository canonicalises on write, so a caller holding the path it
    /// was handed must still match. On macOS a `tempfile` path resolves
    /// through `/private`, which is exactly the mismatch that would make a
    /// duplicate look like a new model.
    #[tokio::test]
    async fn find_by_path_matches_across_canonicalisation() {
        let repo = repo().await;

        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("Real.gguf");
        std::fs::File::create(&path).unwrap();

        let inserted = repo
            .insert(&NewModel::new(
                "Real".to_string(),
                path.clone(),
                7.0,
                Utc::now(),
            ))
            .await
            .unwrap();

        let found = repo
            .find_by_path(&path)
            .await
            .unwrap()
            .expect("the uncanonicalised path must still find it");
        assert_eq!(found.id, inserted.id);
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
    async fn dialect_spec_round_trips_through_insert_and_read() {
        use gglib_core::domain::DialectSpec;
        let repo = repo().await;
        let mut model = make_model("Spec");
        model.dialect_spec = Some(DialectSpec::qwen_xml());

        let inserted = repo.insert(&model).await.unwrap();
        assert_eq!(inserted.dialect_spec, Some(DialectSpec::qwen_xml()));

        let fetched = repo.get_by_id(inserted.id).await.unwrap();
        assert_eq!(fetched.dialect_spec, Some(DialectSpec::qwen_xml()));
    }

    /// Re-registering the same model overwrites the spec, like tags and
    /// capabilities — a re-import re-derives, matching `retag --full`.
    #[tokio::test]
    async fn upsert_overwrites_the_dialect_spec() {
        use gglib_core::domain::DialectSpec;
        let repo = repo().await;

        let mut first = make_model("Zeta");
        first.dialect_spec = Some(DialectSpec::qwen_xml());
        let inserted = repo.insert(&first).await.unwrap();
        assert!(inserted.dialect_spec.is_some());

        let second = make_model("Zeta"); // same model_key, no spec
        let upserted = repo.insert(&second).await.unwrap();
        assert_eq!(upserted.id, inserted.id, "same row, not a new one");
        assert_eq!(upserted.dialect_spec, None, "spec overwritten on upsert");
    }

    /// Rows written before the column existed (or carrying garbage) read
    /// as "no spec" — the tag fallback applies downstream.
    #[tokio::test]
    async fn unreadable_dialect_spec_reads_as_none() {
        let repo = repo().await;
        let inserted = repo.insert(&make_model("Legacy")).await.unwrap();
        assert_eq!(inserted.dialect_spec, None);

        sqlx::query("UPDATE models SET dialect_spec = 'not json' WHERE id = ?")
            .bind(inserted.id)
            .execute(&repo.pool)
            .await
            .unwrap();
        let fetched = repo.get_by_id(inserted.id).await.unwrap();
        assert_eq!(fetched.dialect_spec, None);
    }

    #[tokio::test]
    async fn update_persists_the_dialect_spec() {
        use gglib_core::domain::DialectSpec;
        let repo = repo().await;
        let mut model = repo.insert(&make_model("Eta")).await.unwrap();

        model.dialect_spec = Some(DialectSpec::qwen_xml());
        repo.update(&model).await.unwrap();
        let fetched = repo.get_by_id(model.id).await.unwrap();
        assert_eq!(fetched.dialect_spec, Some(DialectSpec::qwen_xml()));

        model.dialect_spec = None;
        repo.update(&model).await.unwrap();
        let fetched = repo.get_by_id(model.id).await.unwrap();
        assert_eq!(fetched.dialect_spec, None, "update can clear the spec");
    }

    #[tokio::test]
    async fn upsert_deduplicates_same_model_key() {
        let repo = repo().await;
        // Two inserts of the same path → same local model_key → UPSERT updates in place.
        repo.insert(&make_model("Zeta")).await.unwrap();
        repo.insert(&make_model("Zeta")).await.unwrap();
        assert_eq!(repo.list().await.unwrap().len(), 1);
    }

    #[tokio::test]
    async fn upsert_preserves_user_curated_inference_defaults() {
        use gglib_core::domain::InferenceConfig;

        let repo = repo().await;
        let inserted = repo.insert(&make_model("Eta")).await.unwrap();

        // Simulate the user hand-tuning inference defaults after import.
        let mut curated = inserted.clone();
        curated.inference_defaults = Some(InferenceConfig {
            temperature: Some(0.42),
            ..Default::default()
        });
        repo.update(&curated).await.unwrap();

        // Re-registering the same model (same model_key) must not clobber
        // the curated defaults with the freshly-built NewModel's None.
        repo.insert(&make_model("Eta")).await.unwrap();

        let refetched = repo.get_by_id(inserted.id).await.unwrap();
        assert_eq!(
            refetched.inference_defaults.and_then(|c| c.temperature),
            Some(0.42)
        );
    }

    /// `defaults_origin` must survive a re-import exactly like
    /// `inference_defaults` does — it describes that field, so it has to
    /// move with it, not get silently reset to whatever the fresh import
    /// would have written.
    #[tokio::test]
    async fn upsert_preserves_defaults_origin_alongside_curated_defaults() {
        use gglib_core::domain::{DefaultsOrigin, InferenceConfig};

        let repo = repo().await;
        let inserted = repo.insert(&make_model("Theta2")).await.unwrap();

        let mut curated = inserted.clone();
        curated.inference_defaults = Some(InferenceConfig {
            temperature: Some(0.42),
            ..Default::default()
        });
        curated.defaults_origin = Some(DefaultsOrigin::User);
        repo.update(&curated).await.unwrap();

        // Re-registering the same model must not reset the origin either.
        repo.insert(&make_model("Theta2")).await.unwrap();

        let refetched = repo.get_by_id(inserted.id).await.unwrap();
        assert_eq!(refetched.defaults_origin, Some(DefaultsOrigin::User));
    }

    /// Rows written before the `defaults_origin` column existed have `NULL`
    /// there forever — there is no batch backfill (see `setup.rs`). This
    /// proves the read path derives the right answer anyway, through the
    /// real repository rather than the isolated `resolve_defaults_origin`
    /// unit tests in `row_mappers`.
    #[tokio::test]
    async fn a_legacy_row_with_the_reasoning_recipe_reads_back_as_auto_detected() {
        use gglib_core::domain::{DefaultsOrigin, InferenceConfig};

        let repo = repo().await;
        let inserted = repo.insert(&make_model("LegacyReasoning")).await.unwrap();

        // Simulate a row written before this column existed: set
        // inference_defaults directly via SQL, leaving defaults_origin NULL
        // — bypassing the repository's own (already-informed) write path.
        let recipe_json = serde_json::to_string(&InferenceConfig::reasoning_profile()).unwrap();
        sqlx::query("UPDATE models SET inference_defaults = ? WHERE id = ?")
            .bind(&recipe_json)
            .bind(inserted.id)
            .execute(repo.pool())
            .await
            .unwrap();

        let refetched = repo.get_by_id(inserted.id).await.unwrap();
        assert_eq!(
            refetched.inference_defaults,
            Some(InferenceConfig::reasoning_profile())
        );
        assert_eq!(
            refetched.defaults_origin,
            Some(DefaultsOrigin::AutoDetected)
        );
    }

    #[tokio::test]
    async fn get_by_name_is_deterministic_for_duplicate_names() {
        let repo = repo().await;
        // Two distinct model_keys (different file paths) sharing one name.
        let first = repo.insert(&make_model("Theta")).await.unwrap();
        let mut second_source = make_model("Theta");
        second_source.file_path = PathBuf::from("/models/Theta-2.gguf");
        repo.insert(&second_source).await.unwrap();

        // Deterministic: always resolves to the lowest id, not storage order.
        let resolved = repo.get_by_name("Theta").await.unwrap();
        assert_eq!(resolved.id, first.id);
    }
}
