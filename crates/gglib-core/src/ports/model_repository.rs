//! Model repository trait definition.
//!
//! This port defines the interface for model persistence operations.
//! Implementations must handle all storage details internally.

use std::path::Path;

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
/// - The one exception is [`ModelRepository::get_by_identifier`], a *provided*
///   method: identifier resolution is a lookup-key policy, and it lives here
///   precisely so that every facade over the repository shares one copy of it.
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

    /// Insert a new model into the repository, or update the existing row for
    /// the same model.
    ///
    /// **This upserts.** Registering the same model twice is not an error: it
    /// overwrites the mutable columns and returns the existing row, id
    /// included. That is deliberate — post-download registration has to be
    /// safe to retry, and a re-scan must not fail on what it already knows.
    ///
    /// It also means this method never reports a duplicate. A caller for whom
    /// "already there" *is* an error — an explicit "add this file to my
    /// library" rather than a registration — must ask [`Self::find_by_path`]
    /// first. `ModelService::import_from_file` does.
    ///
    /// This doc used to promise `Err(RepositoryError::AlreadyExists)` on a
    /// duplicate file path. No implementation ever did that, and the promise
    /// is what made the silent overwrite hard to see.
    async fn insert(&self, model: &NewModel) -> Result<Model, RepositoryError>;

    /// Find the model registered under `path`, if there is one.
    ///
    /// Both sides are canonicalised before comparing, falling back to the
    /// literal path when canonicalisation fails (a deleted file, a broken
    /// symlink). Doing it on both sides rather than trusting the stored form
    /// is what makes this agree with every implementation: the `SQLite`
    /// repository canonicalises on write, test doubles store what they are
    /// given, and on macOS a `tempfile` directory differs from its resolved
    /// path by a `/private` prefix — so a one-sided comparison finds nothing
    /// and reports "no duplicate" for a file that is plainly already there.
    ///
    /// Provided rather than required so implementors and test doubles inherit
    /// it automatically. The scan is over the whole library, which is a table
    /// of tens to hundreds of rows and is read once per explicit add.
    async fn find_by_path(&self, path: &Path) -> Result<Option<Model>, RepositoryError> {
        fn resolved(path: &Path) -> std::path::PathBuf {
            std::fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf())
        }

        let target = resolved(path);
        Ok(self
            .list()
            .await?
            .into_iter()
            .find(|model| resolved(&model.file_path) == target))
    }

    /// Update an existing model.
    ///
    /// Returns `Err(RepositoryError::NotFound)` if the model doesn't exist.
    async fn update(&self, model: &Model) -> Result<(), RepositoryError>;

    /// Delete a model by its database ID.
    ///
    /// Returns `Err(RepositoryError::NotFound)` if the model doesn't exist.
    async fn delete(&self, id: i64) -> Result<(), RepositoryError>;

    /// Resolve a model by user-facing identifier: numeric database id first,
    /// then exact name.
    ///
    /// This is the **single lookup-key policy** for the workspace — every
    /// facade over a repository (`ModelService`, the `ModelCatalogPort`
    /// adapter) delegates here rather than choosing its own key. Before this
    /// existed the two disagreed: the service resolved ids, the catalog port
    /// did not, so the same string resolved differently depending on which
    /// pipeline a request travelled down.
    ///
    /// Provided rather than required so implementors and test doubles inherit
    /// it automatically.
    ///
    /// Returns `Ok(None)` when nothing matches. A storage failure on the id
    /// lookup propagates rather than silently falling through to the name
    /// lookup — only a genuine `NotFound` continues.
    async fn get_by_identifier(&self, identifier: &str) -> Result<Option<Model>, RepositoryError> {
        if let Ok(id) = identifier.parse::<i64>() {
            match self.get_by_id(id).await {
                Ok(model) => return Ok(Some(model)),
                Err(RepositoryError::NotFound(_)) => {}
                Err(e) => return Err(e),
            }
        }
        match self.get_by_name(identifier).await {
            Ok(model) => Ok(Some(model)),
            Err(RepositoryError::NotFound(_)) => Ok(None),
            Err(e) => Err(e),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::ModelCapabilities;
    use chrono::Utc;
    use std::collections::HashMap;
    use std::path::PathBuf;

    /// The single model these tests resolve: id 7, name "qwen3".
    fn model() -> Model {
        Model {
            dialect_spec: None,
            id: 7,
            name: "qwen3".to_string(),
            model_key: String::new(),
            file_path: PathBuf::from("/models/qwen3.gguf"),
            param_count_b: 7.0,
            architecture: None,
            quantization: None,
            context_length: None,
            expert_count: None,
            expert_used_count: None,
            expert_shared_count: None,
            metadata: HashMap::new(),
            added_at: Utc::now(),
            hf_repo_id: None,
            hf_commit_sha: None,
            hf_filename: None,
            download_date: None,
            last_update_check: None,
            tags: vec![],
            capabilities: ModelCapabilities::default(),
            inference_defaults: None,
            defaults_origin: None,
            server_defaults: None,
            benchmark_summary: None,
        }
    }

    /// Serves [`model`] and can be told to fail the id lookup with a storage
    /// error instead of `NotFound`.
    struct OneModelRepo {
        storage_error_on_id: bool,
    }

    #[async_trait]
    impl ModelRepository for OneModelRepo {
        async fn list(&self) -> Result<Vec<Model>, RepositoryError> {
            Ok(vec![model()])
        }

        async fn get_by_id(&self, id: i64) -> Result<Model, RepositoryError> {
            if self.storage_error_on_id {
                return Err(RepositoryError::Storage("disk on fire".into()));
            }
            if id == 7 {
                Ok(model())
            } else {
                Err(RepositoryError::NotFound(format!("id={id}")))
            }
        }

        async fn get_by_name(&self, name: &str) -> Result<Model, RepositoryError> {
            if name == "qwen3" {
                Ok(model())
            } else {
                Err(RepositoryError::NotFound(format!("name={name}")))
            }
        }

        async fn insert(&self, _model: &NewModel) -> Result<Model, RepositoryError> {
            unimplemented!("not exercised by these tests")
        }

        async fn update(&self, _model: &Model) -> Result<(), RepositoryError> {
            unimplemented!("not exercised by these tests")
        }

        async fn delete(&self, _id: i64) -> Result<(), RepositoryError> {
            unimplemented!("not exercised by these tests")
        }
    }

    fn repo() -> OneModelRepo {
        OneModelRepo {
            storage_error_on_id: false,
        }
    }

    #[tokio::test]
    async fn resolves_a_numeric_identifier_by_id() {
        let found = repo().get_by_identifier("7").await.unwrap();
        assert_eq!(found.unwrap().name, "qwen3");
    }

    #[tokio::test]
    async fn resolves_a_non_numeric_identifier_by_name() {
        let found = repo().get_by_identifier("qwen3").await.unwrap();
        assert_eq!(found.unwrap().id, 7);
    }

    /// A numeric string that is not a known id must still get its name lookup
    /// — otherwise a model literally named "42" would be unreachable.
    #[tokio::test]
    async fn numeric_miss_falls_through_to_the_name_lookup() {
        assert!(repo().get_by_identifier("42").await.unwrap().is_none());
    }

    #[tokio::test]
    async fn unknown_identifier_is_none_not_an_error() {
        assert!(repo().get_by_identifier("ghost").await.unwrap().is_none());
    }

    /// The fall-through is for `NotFound` only. A real storage failure must
    /// surface rather than being masked by a name lookup that happens to miss.
    #[tokio::test]
    async fn storage_failure_on_the_id_lookup_propagates() {
        let repo = OneModelRepo {
            storage_error_on_id: true,
        };
        let err = repo.get_by_identifier("7").await.unwrap_err();
        assert!(matches!(err, RepositoryError::Storage(_)));
    }
}
