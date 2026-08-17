//! `SQLite` implementation of the `ModelRepository` trait.

use async_trait::async_trait;
use sqlx::SqlitePool;
use std::path::Path;

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
///
/// The local key hashes the *canonical* path — the very value bound to the
/// `file_path` column a few lines below — rather than the raw path it was
/// handed. Hashing the raw path made the key disagree with the column:
/// `gglib model add model.gguf`, run in two different directories over two
/// genuinely different files, produced one key and two stored paths. The
/// `ON CONFLICT(model_key)` clause then merged them, and because `name`,
/// `param_count_b` and `architecture` are absent from its `DO UPDATE SET`
/// list while `file_path` is present, the surviving row wore the first
/// model's identity over the second model's file.
///
/// It hashes a [`Path`], not the `String`, and that is load-bearing rather
/// than stylistic. `Path`'s `Hash` is defined over components while `str`'s
/// is defined over bytes, so the two disagree for a path they both consider
/// identical. Hashing the string would therefore have moved the key of *every*
/// local row already in a user's library, rather than only the rows this rule
/// genuinely re-keys — the ones registered under a spelling that was not
/// already canonical.
///
/// Those rows do move, and they are migrated rather than stranded:
/// `setup::backfill_local_model_keys` recomputes each `local:` key from the
/// stored `file_path` column on open. That column has been canonical on every
/// build that ever wrote it, so it is a sound source. The migration matters
/// most on Windows, where `canonicalize` returns an extended-length `\\?\`
/// path that no pre-change caller ever produced — so *no* Windows row was
/// "already canonical" and every one of them needs re-keying.
///
/// `canonical_path` is the already-resolved string the caller is about to bind
/// to `file_path`, passed in rather than recomputed. Resolving is a blocking
/// syscall and this runs inside an async `insert`; taking it as an argument
/// also makes it impossible for the key and the column to be derived from two
/// different resolutions of the same path.
fn compute_model_key(model: &NewModel, canonical_path: &str) -> String {
    match (&model.hf_repo_id, &model.hf_commit_sha, &model.hf_filename) {
        (Some(repo), Some(sha), Some(filename)) => {
            let base = base_shard_filename(filename);
            format!("hf:{}@{}#{}", repo, sha, base)
        }
        _ => local_model_key_for(canonical_path),
    }
}

/// The `local:` key for an already-canonical path string.
///
/// Split out so the startup backfill in `setup.rs` can recompute a stored
/// row's key from its `file_path` column using exactly this function, rather
/// than a second copy of the rule that could drift from it.
pub(crate) fn local_model_key_for(canonical_path: &str) -> String {
    use std::collections::hash_map::DefaultHasher;
    use std::hash::{Hash, Hasher};
    let mut hasher = DefaultHasher::new();
    Path::new(canonical_path).hash(&mut hasher);
    format!("local:{:x}", hasher.finish())
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

    async fn find_by_path(&self, path: &Path) -> Result<Option<Model>, RepositoryError> {
        // `path` arrives already resolved, and `file_path` was normalised
        // through the same function on write, so this is a plain equality
        // test rather than a scan that re-resolves every row. That matters
        // twice over: it keeps a blocking `canonicalize` syscall per library
        // row out of an async fn, and it removes the fallback that used to
        // turn an unresolvable path into "no duplicate found".
        //
        // The `json_each` arm catches sharded models: shard 2 of a group
        // already registered is the same duplicate as shard 1, but only
        // shard 1's path lives in `file_path` — the rest are in
        // `file_paths_json`. `json_valid` guards a column written before that
        // serialisation existed, since `json_each` errors on malformed input
        // rather than returning no rows.
        let query = format!(
            "SELECT {} FROM models \
             WHERE models.file_path = ?1 \
                OR (models.file_paths_json IS NOT NULL \
                    AND json_valid(models.file_paths_json) \
                    AND EXISTS (SELECT 1 FROM json_each(models.file_paths_json) \
                                WHERE json_each.value = ?1)) \
             ORDER BY models.id LIMIT 1",
            MODEL_SELECT_COLUMNS
        );

        let row = sqlx::query(&query)
            .bind(path.to_string_lossy().as_ref())
            .fetch_optional(&self.pool)
            .await
            .map_err(|e| RepositoryError::Storage(e.to_string()))?;

        row.as_ref().map(row_to_model).transpose()
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
        let model_key = compute_model_key(model, &file_path_string);

        // Serialize file_paths if present, normalised the same way as
        // `file_path` above. `find_by_path`'s sibling arm compares a resolved
        // query path against these entries, so storing them as handed in
        // would make that arm match only when the caller happened to pass
        // already-resolved siblings — which the download path does not.
        let file_paths_json = model.file_paths.as_ref().and_then(|paths| {
            let normalized: Vec<String> = paths
                .iter()
                .map(|path| normalized_file_path_string(path))
                .collect();
            serde_json::to_string(&normalized).ok()
        });

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
                -- Coalesced, not assigned. A re-registration that carries no
                -- shard list is a local re-import, which never populates one;
                -- assigning would erase the sibling paths a download recorded
                -- and silently disable the sharded-duplicate lookup for that
                -- model. A download always supplies the list, so it still
                -- wins when there is one.
                file_paths_json = COALESCE(excluded.file_paths_json, models.file_paths_json),
                quantization = COALESCE(excluded.quantization, models.quantization),
                context_length = COALESCE(excluded.context_length, models.context_length),
                expert_count = COALESCE(excluded.expert_count, models.expert_count),
                expert_used_count = COALESCE(excluded.expert_used_count, models.expert_used_count),
                expert_shared_count = COALESCE(excluded.expert_shared_count, models.expert_shared_count),
                -- Coalesced for the same reason: a local re-import sets
                -- neither, and erasing them would make a downloaded model
                -- read as never-downloaded and never-update-checked, which
                -- the update-check workflow keys on.
                download_date = COALESCE(excluded.download_date, models.download_date),
                last_update_check = COALESCE(excluded.last_update_check, models.last_update_check),
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

        // Written by `update` only: `insert` takes a `NewModel`, which has no
        // caps to carry — the observation exists only after a launch (ADR
        // 0007), and the upsert leaves the stored column alone so a re-import
        // cannot erase what a launch recorded.
        let template_caps_json = model
            .template_caps
            .as_ref()
            .and_then(|caps| serde_json::to_string(caps).ok());

        let result = sqlx::query(
            "UPDATE models SET name = ?, file_path = ?, param_count_b = ?, architecture = ?, quantization = ?, context_length = ?, metadata = ?, hf_repo_id = ?, hf_commit_sha = ?, hf_filename = ?, download_date = ?, last_update_check = ?, tags = ?, capabilities = ?, inference_defaults = ?, defaults_origin = ?, server_defaults = ?, dialect_spec = ?, template_caps = ? WHERE id = ?"
        )
            .bind(&model.name)
            // Normalised exactly as `insert` does. `find_by_path` is a plain
            // equality test against this column, so a writer that stores an
            // unresolved path here silently makes the row unfindable — and
            // `PATCH /api/models/{id}` reaches this with a caller-supplied
            // `file_path`.
            .bind(normalized_file_path_string(&model.file_path))
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
            .bind(&template_caps_json)
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

    /// The repository canonicalises on write, and callers resolve before
    /// asking, so a real file on disk round-trips. On macOS a `tempfile`
    /// directory resolves through `/private`, which is exactly the mismatch
    /// that would make a duplicate look like a new model.
    #[tokio::test]
    async fn find_by_path_matches_the_stored_canonical_path() {
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

        let resolved = std::fs::canonicalize(&path).expect("the file exists");
        let found = repo
            .find_by_path(&resolved)
            .await
            .unwrap()
            .expect("the resolved path must find the row stored under it");
        assert_eq!(found.id, inserted.id);
    }

    /// **Upgrade safety.** The key a previous build computed for an
    /// already-canonical path must not move.
    ///
    /// Nothing backfills `model_key` — it appears in `setup.rs` only as a
    /// column and a unique index. If this value shifts, every local row in
    /// every existing library becomes unreachable by `ON CONFLICT(model_key)`
    /// and the next registration of that file silently appends a second row,
    /// which is the exact failure this PR exists to prevent.
    ///
    /// The expectation is recomputed the old way rather than hardcoded,
    /// because `DefaultHasher`'s output is explicitly not guaranteed stable
    /// across Rust releases. What is pinned is the relationship: hashing the
    /// canonical path must agree with hashing the `PathBuf` a previous build
    /// hashed.
    #[test]
    fn the_local_key_for_a_canonical_path_survives_the_upgrade() {
        use std::collections::hash_map::DefaultHasher;
        use std::hash::{Hash, Hasher};

        // A real file, resolved: the point is that a genuine canonicalisation
        // runs and still lands on the previous value. A path that does not
        // exist would take the literal fallback, so no canonicalisation would
        // happen and the assertion would prove nothing about it — and would
        // break on any machine where that path did exist behind a symlink.
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("Qwen3-8B-Q4_K_M.gguf");
        std::fs::File::create(&file).unwrap();
        let path = std::fs::canonicalize(&file).expect("the file exists");
        assert_eq!(
            std::fs::canonicalize(&path).unwrap(),
            path,
            "the fixture must already be canonical"
        );

        // What builds up to and including c825c69d computed.
        let previous = {
            let mut hasher = DefaultHasher::new();
            path.hash(&mut hasher);
            format!("local:{:x}", hasher.finish())
        };

        let model = NewModel::new("Qwen3-8B-Q4_K_M".to_string(), path, 7.0, Utc::now());
        assert_eq!(
            compute_model_key(&model, &normalized_file_path_string(&model.file_path)),
            previous,
            "changing the local key strands every existing local row"
        );
    }

    /// **The asymmetry this follow-up exists for.** One file is one model,
    /// however its path was spelled on the way in.
    ///
    /// The local `model_key` hashes the path. While it hashed the *raw* path
    /// and the `file_path` column stored the *resolved* one, the two
    /// disagreed about identity: two spellings of a single file produced two
    /// keys and therefore two rows for one model on disk, and — the
    /// destructive direction — one raw spelling reaching two different files
    /// produced a single key, so `ON CONFLICT(model_key)` merged them. Because
    /// `name`, `param_count_b` and `architecture` are absent from the
    /// `DO UPDATE SET` list while `file_path` is present, that survivor wore
    /// the first model's identity over the second model's file.
    ///
    /// Hashing the stored string closes both directions at once. This test
    /// pins the spelling direction, which is the one reachable without
    /// changing the process working directory; it fails if the key goes back
    /// to hashing the path it was handed.
    ///
    /// The respelling uses `..` deliberately. `Path`'s `Eq` and `Hash` are
    /// defined over *components*, so a `.` or a doubled separator is already
    /// normalised away before any hashing happens and cannot demonstrate the
    /// difference. `..` survives as a real component — the filesystem is the
    /// only thing that can resolve it — which is exactly the gap between
    /// "the path as written" and "the file it names".
    #[tokio::test]
    async fn two_spellings_of_one_path_are_one_model() {
        let repo = repo().await;

        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("Solo.gguf");
        std::fs::File::create(&path).unwrap();
        std::fs::create_dir(dir.path().join("sub")).unwrap();
        // Same file, a path only the filesystem can equate: `dir/sub/../Solo.gguf`.
        let respelled = dir.path().join("sub").join("..").join("Solo.gguf");
        assert_ne!(path, respelled, "the two spellings must differ literally");

        let first = repo
            .insert(&NewModel::new("Solo".to_string(), path, 7.0, Utc::now()))
            .await
            .unwrap();
        let second = repo
            .insert(&NewModel::new(
                "Solo".to_string(),
                respelled,
                7.0,
                Utc::now(),
            ))
            .await
            .unwrap();

        assert_eq!(
            first.id, second.id,
            "one file must be one model whichever way its path was written"
        );
        assert_eq!(repo.list().await.unwrap().len(), 1);
    }

    /// Two genuinely different files that share a basename are two models.
    ///
    /// Companion to the test above, pinning the opposite direction: this one
    /// holds under either key rule, and exists so that a future "fix" which
    /// over-normalises — hashing only the filename, say — is caught.
    #[tokio::test]
    async fn two_files_sharing_a_basename_stay_two_models() {
        let repo = repo().await;

        let root = tempfile::tempdir().unwrap();
        let alpha_path = root.path().join("projA").join("model.gguf");
        let beta_path = root.path().join("projB").join("model.gguf");
        for path in [&alpha_path, &beta_path] {
            std::fs::create_dir_all(path.parent().unwrap()).unwrap();
            std::fs::File::create(path).unwrap();
        }

        let alpha = repo
            .insert(&NewModel::new(
                "ALPHA".to_string(),
                alpha_path.clone(),
                7.0,
                Utc::now(),
            ))
            .await
            .unwrap();
        let beta = repo
            .insert(&NewModel::new(
                "BETA".to_string(),
                beta_path.clone(),
                7.0,
                Utc::now(),
            ))
            .await
            .unwrap();

        assert_ne!(
            alpha.id, beta.id,
            "two different files must not collapse into one row"
        );
        assert_eq!(repo.list().await.unwrap().len(), 2);

        let at_beta = repo
            .find_by_path(&std::fs::canonicalize(&beta_path).unwrap())
            .await
            .unwrap()
            .expect("a row at B's path");
        assert_eq!(
            at_beta.name, "BETA",
            "the row standing at B's path must be B's model, not A's identity"
        );
    }

    /// A refresh must not erase what only a download knows.
    ///
    /// `file_paths_json`, `download_date` and `last_update_check` were plain
    /// assignments in `DO UPDATE SET`, and a local re-import populates none of
    /// them — so `--force` on a downloaded model wiped its shard list (taking
    /// the sibling lookup with it) and made it read as never-downloaded and
    /// never-update-checked, which the update-check workflow keys on.
    #[tokio::test]
    async fn a_refresh_keeps_the_shard_list_and_download_provenance() {
        let repo = repo().await;

        let dir = tempfile::tempdir().unwrap();
        let first = dir.path().join("m-00001-of-00002.gguf");
        let second = dir.path().join("m-00002-of-00002.gguf");
        std::fs::File::create(&first).unwrap();
        std::fs::File::create(&second).unwrap();

        let mut downloaded = NewModel::new("Sharded".to_string(), first.clone(), 7.0, Utc::now());
        downloaded.file_paths = Some(vec![first.clone(), second.clone()]);
        downloaded.download_date = Some(Utc::now());
        downloaded.last_update_check = Some(Utc::now());
        let original = repo.insert(&downloaded).await.unwrap();

        // A local re-import of the same file: no shard list, no dates.
        let reimport = NewModel::new("Sharded".to_string(), first.clone(), 7.0, Utc::now());
        let refreshed = repo.insert(&reimport).await.unwrap();

        assert_eq!(refreshed.id, original.id, "same row");
        assert!(
            refreshed.download_date.is_some(),
            "a refresh must not erase download_date"
        );
        assert!(
            refreshed.last_update_check.is_some(),
            "a refresh must not erase last_update_check"
        );
        assert!(
            repo.find_by_path(&std::fs::canonicalize(&second).unwrap())
                .await
                .unwrap()
                .is_some(),
            "the shard list must survive a refresh, or the sibling lookup dies with it"
        );
    }

    /// Adding shard 2 of a group already registered is the same duplicate as
    /// adding shard 1. Only the first shard's path reaches `file_path`; the
    /// siblings live in `file_paths_json`, so a lookup that reads only the
    /// column would wave the rest through.
    #[tokio::test]
    async fn find_by_path_matches_a_sharded_sibling() {
        let repo = repo().await;

        let dir = tempfile::tempdir().unwrap();
        let first = dir.path().join("model-00001-of-00002.gguf");
        let second = dir.path().join("model-00002-of-00002.gguf");
        std::fs::File::create(&first).unwrap();
        std::fs::File::create(&second).unwrap();

        // Handed in exactly as the download path hands them in: unresolved.
        // Canonicalising these here would have the test normalise on the
        // repository's behalf and pass whether or not `insert` does its job —
        // on macOS a tempdir sits behind `/private`, so raw and resolved
        // genuinely differ and the sibling arm would never match in
        // production.
        let mut model = NewModel::new("Sharded".to_string(), first.clone(), 7.0, Utc::now());
        model.file_paths = Some(vec![first.clone(), second.clone()]);
        let inserted = repo.insert(&model).await.unwrap();

        let found = repo
            .find_by_path(&std::fs::canonicalize(&second).unwrap())
            .await
            .unwrap()
            .expect("the sibling shard belongs to a model already present");
        assert_eq!(found.id, inserted.id);
    }

    /// `update` writes `file_path` too, and `find_by_path` is a plain equality
    /// test against that column. A writer that stores an unresolved path makes
    /// the row unfindable — and `PATCH /api/models/{id}` reaches this with a
    /// caller-supplied path.
    #[tokio::test]
    async fn update_stores_a_canonical_path_so_the_row_stays_findable() {
        let repo = repo().await;

        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("Updatable.gguf");
        std::fs::File::create(&path).unwrap();
        std::fs::create_dir(dir.path().join("sub")).unwrap();

        let inserted = repo
            .insert(&NewModel::new(
                "Updatable".to_string(),
                path.clone(),
                7.0,
                Utc::now(),
            ))
            .await
            .unwrap();

        // Write the same file back under a spelling only the filesystem can
        // equate, as an API caller might.
        let mut edited = inserted.clone();
        edited.file_path = dir.path().join("sub").join("..").join("Updatable.gguf");
        repo.update(&edited).await.unwrap();

        let found = repo
            .find_by_path(&std::fs::canonicalize(&path).unwrap())
            .await
            .unwrap()
            .expect("an updated row must still be findable by its real path");
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

    // ── template_caps (ADR 0007) ──────────────────────────────────────────

    fn measured_caps() -> gglib_core::domain::TemplateCaps {
        gglib_core::domain::TemplateCaps {
            supports_reasoning_effort: Some(true),
            supports_tools: Some(true),
            supports_typed_content: Some(false),
            ..Default::default()
        }
    }

    /// The row-mapper round trip: a launch's observation written by `update`
    /// is what the next read hands back, and a never-observed row reads as
    /// `None` — the tri-state's third state, straight from a NULL column.
    #[tokio::test]
    async fn update_persists_the_template_caps() {
        let repo = repo().await;
        let mut model = repo.insert(&make_model("Theta")).await.unwrap();
        assert_eq!(model.template_caps, None, "a fresh import is unobserved");

        model.template_caps = Some(measured_caps());
        repo.update(&model).await.unwrap();
        let fetched = repo.get_by_id(model.id).await.unwrap();
        assert_eq!(fetched.template_caps, Some(measured_caps()));
    }

    /// Same tolerance as `dialect_spec`: garbage in the column degrades to
    /// "never observed" rather than failing every read of the row.
    #[tokio::test]
    async fn unreadable_template_caps_reads_as_none() {
        let repo = repo().await;
        let inserted = repo.insert(&make_model("Iota")).await.unwrap();

        sqlx::query("UPDATE models SET template_caps = 'not json' WHERE id = ?")
            .bind(inserted.id)
            .execute(&repo.pool)
            .await
            .unwrap();

        let fetched = repo.get_by_id(inserted.id).await.unwrap();
        assert_eq!(fetched.template_caps, None);
    }

    /// A re-import must not erase what a launch observed: `insert`'s upsert
    /// leaves the column alone, the same bargain `inference_defaults` gets.
    #[tokio::test]
    async fn upsert_preserves_recorded_template_caps() {
        let repo = repo().await;
        let inserted = repo.insert(&make_model("Kappa")).await.unwrap();

        let mut observed = inserted.clone();
        observed.template_caps = Some(measured_caps());
        repo.update(&observed).await.unwrap();

        repo.insert(&make_model("Kappa")).await.unwrap();

        let refetched = repo.get_by_id(inserted.id).await.unwrap();
        assert_eq!(refetched.template_caps, Some(measured_caps()));
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
