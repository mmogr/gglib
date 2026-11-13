//! Model service - orchestrates model CRUD operations.

use super::{ModelOrigin, build_new_model};
use crate::domain::{Model, NewModel};
use crate::ports::{CoreError, GgufParserPort, ModelRepository, RepositoryError};
use std::path::Path;
use std::sync::Arc;

/// Whether an explicit import may overwrite a model already in the library.
///
/// Named rather than a bare `bool` because the call sites read very
/// differently — `ImportMode::Refresh` says what it does, `true` does not,
/// and the wrong value here silently rewrites a row's tags and capabilities.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ImportMode {
    /// Refuse when the file is already registered, reporting
    /// [`RepositoryError::AlreadyExists`].
    ///
    /// The default, and what an "add this file to my library" means: a
    /// duplicate is a conflict the caller should hear about, not an
    /// overwrite performed on their behalf.
    #[default]
    Fresh,
    /// Re-derive the model's *detected* metadata from the file, updating the
    /// stored row in place and keeping its database id.
    ///
    /// This is what `gglib model add --reimport` asks for, and what re-importing
    /// did unconditionally before [`ModelService::import_from_file`] began
    /// guarding it. It refreshes the stored header, tags, capabilities,
    /// quantization, context length and the expert counts — wider than
    /// `model retag`, which rebuilds only tags.
    ///
    /// It is narrower than "overwrite the row", and the columns do not all
    /// behave alike:
    ///
    /// - `metadata` (the header) is **merged**: each key the file has is read
    ///   afresh, and a key added with `model update --metadata` stays.
    /// - `tags` and `capabilities` are **assigned**. A refresh
    ///   replaces them with whatever was just derived, including with nothing
    ///   — these can be cleared.
    /// - `quantization`, `context_length` and the expert counts are
    ///   **coalesced**. A detector that now reads no value leaves the stored
    ///   one standing rather than emptying it.
    /// - `name`, `param_count_b` and `architecture` are **absent** from the
    ///   upsert entirely, so a name the user chose survives untouched.
    Refresh,
}

/// The diff produced by [`ModelService::retag_model`] when something changed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RetagDiff {
    /// Tags that were newly added.
    pub added: Vec<String>,
    /// Tags that were removed (only non-empty on a `full = true` rebuild).
    pub removed: Vec<String>,
    /// The image family read from the file for a model that had none.
    pub family_found: Option<crate::domain::ImageFamily>,
    /// Whether the type, the bits per weight or the parameter count the model
    /// lacked were read from its file's header.
    pub facts_read: bool,
}

impl RetagDiff {
    /// Returns `true` if any tag was added or removed, a family was found, or
    /// a header fact was read.
    pub const fn is_changed(&self) -> bool {
        !self.added.is_empty()
            || !self.removed.is_empty()
            || self.family_found.is_some()
            || self.facts_read
    }
}

/// Service for model operations.
///
/// This service provides high-level model management by delegating
/// to the injected `ModelRepository`. It adds no business logic
/// beyond what the repository provides - it's a thin facade.
pub struct ModelService {
    pub(super) repo: Arc<dyn ModelRepository>,
}

impl ModelService {
    /// Create a new model service with the given repository.
    pub fn new(repo: Arc<dyn ModelRepository>) -> Self {
        Self { repo }
    }

    /// List all models.
    pub async fn list(&self) -> Result<Vec<Model>, CoreError> {
        self.repo.list().await.map_err(CoreError::from)
    }

    /// Get a model by its identifier: id, exact name or listed `name@…` id.
    ///
    /// Thin wrapper over [`ModelRepository::get_by_identifier`], which owns the
    /// lookup-key policy so this service and the `ModelCatalogPort` adapter
    /// cannot disagree about what a given string resolves to.
    pub async fn get(&self, identifier: &str) -> Result<Option<Model>, CoreError> {
        self.repo
            .get_by_identifier(identifier)
            .await
            .map_err(CoreError::from)
    }

    /// Get a model by its database ID.
    pub async fn get_by_id(&self, id: i64) -> Result<Option<Model>, CoreError> {
        match self.repo.get_by_id(id).await {
            Ok(model) => Ok(Some(model)),
            Err(RepositoryError::NotFound(_)) => Ok(None),
            Err(e) => Err(CoreError::from(e)),
        }
    }

    /// Add a new model from an already-built row.
    ///
    /// This is the raw registration door: it inherits
    /// [`ModelRepository::insert`]'s upsert, so a row whose key already exists
    /// is overwritten rather than reported. That is right for
    /// registration-after-download, which has to be safe to retry, and wrong
    /// for "add this file to my library" — which is
    /// [`Self::import_from_file`], the door that checks first.
    pub async fn add(&self, model: NewModel) -> Result<Model, CoreError> {
        self.repo.insert(&model).await.map_err(CoreError::from)
    }

    /// Import a model from a local GGUF file with full metadata extraction.
    ///
    /// Validates file, parses GGUF metadata, detects capabilities, and registers
    /// with rich metadata. This is the canonical way to add local models.
    ///
    /// # Arguments
    ///
    /// * `file_path` - Absolute path to the GGUF file
    /// * `gguf_parser` - Parser implementation for metadata extraction
    /// * `param_count_override` - Optional user override for parameter count
    /// * `mode` - Whether a file already in the library is a conflict
    ///   ([`ImportMode::Fresh`]) or should be re-derived in place
    ///   ([`ImportMode::Refresh`])
    ///
    /// # Returns
    ///
    /// Returns the registered `Model` with full metadata, or validation error.
    ///
    /// # Errors
    ///
    /// [`CoreError::Validation`] if the file is missing, is not a readable
    /// GGUF, or cannot be resolved to a canonical path;
    /// [`RepositoryError::AlreadyExists`] under [`ImportMode::Fresh`] when the
    /// file is already registered.
    ///
    /// # Design
    ///
    /// This method validates and parses the GGUF file, then delegates
    /// naming, capability detection, and tag generation to
    /// [`build_new_model`] — the construction path shared with the
    /// `HuggingFace` download path — before persisting the result.
    ///
    /// The path is resolved once here, immediately after validation has
    /// established that the file exists, and the resolved form is what every
    /// later step sees: the duplicate lookup, the row that gets built, and
    /// the `model_key` derived from it. Normalising at the entry point rather
    /// than at each consumer is what keeps those three from disagreeing about
    /// which file is which.
    ///
    /// # Concurrency
    ///
    /// The duplicate check and the insert are separate statements with no
    /// transaction around them, and `file_path` carries no unique index — only
    /// `model_key` does. Two simultaneous adds of one file can therefore both
    /// pass the check; the `ON CONFLICT(model_key)` clause still collapses
    /// them onto a single row, so the library stays correct, but the loser is
    /// told it added a model that in fact already existed rather than
    /// receiving a conflict. Closing that would take a unique index on the
    /// path or a transaction spanning both statements, and is not something
    /// this guard attempts.
    pub async fn import_from_file(
        &self,
        file_path: &Path,
        gguf_parser: &dyn GgufParserPort,
        param_count_override: Option<f64>,
        mode: ImportMode,
    ) -> Result<Model, CoreError> {
        // 1. Validate and parse GGUF file
        let gguf_metadata = crate::utils::validation::validate_and_parse_gguf(
            gguf_parser,
            file_path
                .to_str()
                .ok_or_else(|| CoreError::Validation("Invalid file path encoding".to_string()))?,
        )
        .map_err(|e| CoreError::Validation(format!("GGUF validation failed: {e}")))?;

        // 2. Resolve the path exactly once, now that validation has
        //    established the file is there. Everything downstream uses the
        //    resolved form, so no later step has to re-resolve — and none can
        //    quietly disagree about what "the same file" means.
        let resolved = crate::paths::canonical_model_path(file_path).map_err(|e| {
            CoreError::Validation(format!(
                "Cannot resolve '{}' to a canonical path: {e}",
                file_path.display()
            ))
        })?;

        // 3. A file already in the library is a conflict, not a silent
        //    overwrite. `insert` upserts so that registration after a download
        //    can be retried; an explicit "add this file" is the opposite
        //    intent, and without this check the caller gets a success response
        //    for a model it did not add, with the existing row's tags and
        //    capabilities rewritten underneath it.
        //
        //    `ImportMode::Refresh` is the caller stating that the overwrite is
        //    what they came for — re-deriving a model's tags and facts from the file
        //    with newer detection logic. It is opt-in because it is
        //    destructive, and unreachable by accident.
        let existing = self
            .repo
            .find_by_path(&resolved)
            .await
            .map_err(CoreError::from)?;

        if mode == ImportMode::Fresh
            && let Some(existing) = &existing
        {
            return Err(CoreError::Repository(RepositoryError::AlreadyExists(
                format!(
                    "'{}' is already in the library as \"{}\"",
                    file_path.display(),
                    existing.name
                ),
            )));
        }

        // A refresh has to be asked for by the model's *own* primary file.
        //
        // `find_by_path` also matches a sharded model through its sibling
        // paths, and the row it hands back is the one keyed to shard 1. Left
        // unchecked, refreshing from shard 2 would land on that row and
        // `file_path = excluded.file_path` would repoint it at the shard-2
        // file — which llama.cpp cannot open a split GGUF from, so the model
        // would stop launching.
        //
        // Both sides are resolved before comparing. Comparing the stored
        // column directly would assume the column is already canonical, and a
        // row still holding an unresolved path would then refuse a refresh of
        // its *own* first shard, and say so by printing the same path twice.
        if let Some(existing) = &existing {
            let existing_primary = crate::paths::canonical_model_path_string(&existing.file_path);
            if existing_primary != resolved.to_string_lossy() {
                return Err(CoreError::Validation(format!(
                    "'{}' belongs to \"{}\", which is registered under '{}'. \
                     Re-import that path instead — refreshing a sharded model from \
                     anything but its first shard would repoint it at a file it \
                     cannot be loaded from.",
                    file_path.display(),
                    existing.name,
                    existing_primary
                )));
            }
        }

        // 4. Build the model row via the naming/capability/tag policy shared
        //    with the HuggingFace download path.
        //
        //    Built from the path the caller gave, not the resolved one. The
        //    derived name falls back to the file stem, so building from the
        //    resolved path would silently rename a symlinked model to its
        //    target's stem — `current.gguf -> Qwen3-8B.gguf` would stop being
        //    "current" — and would disagree with the preview the CLI printed
        //    from the path the user typed. Only the *stored* path is
        //    canonical; the *derived* name still comes from what was asked
        //    for.
        let origin = ModelOrigin::LocalFile {
            param_count_override,
        };
        let mut new_model = build_new_model(
            file_path,
            Some(&gguf_metadata),
            gguf_parser,
            &origin,
            chrono::Utc::now(),
        );

        // 5. Store and key the row by the resolved path, whatever spelling
        //    reached it.
        new_model.file_path = resolved;

        // 6. A refresh has to land on the row it is refreshing.
        //
        //    `build_new_model` was handed `ModelOrigin::LocalFile`, so it
        //    carries no HuggingFace metadata and the row would be keyed
        //    `local:<hash>`. A downloaded model is keyed `hf:<repo>@<sha>#<file>`.
        //    Nothing would conflict, `file_path` carries no unique index, and
        //    `--reimport` on a downloaded model would append a *second* row for
        //    one file.
        //
        //    Carrying the stored provenance forward keeps the computed key
        //    equal to the existing row's, so the upsert updates it.
        if let Some(existing) = &existing {
            new_model.hf_repo_id.clone_from(&existing.hf_repo_id);
            new_model.hf_commit_sha.clone_from(&existing.hf_commit_sha);
            new_model.hf_filename.clone_from(&existing.hf_filename);
        }

        // 7. Persist to repository
        self.repo.insert(&new_model).await.map_err(CoreError::from)
    }

    /// Look up the model registered under `file_path`, if any.
    ///
    /// Resolves the path the same way [`Self::import_from_file`] does, so a
    /// caller can ask "is this already here?" before doing expensive or
    /// interactive work and get the same answer the import would.
    ///
    /// # Errors
    ///
    /// [`CoreError::Validation`] if the path cannot be resolved, and
    /// [`CoreError::Repository`] if the lookup itself fails. Neither is
    /// reported as "no duplicate".
    pub async fn find_by_path(&self, file_path: &Path) -> Result<Option<Model>, CoreError> {
        let resolved = crate::paths::canonical_model_path(file_path).map_err(|e| {
            CoreError::Validation(format!(
                "Cannot resolve '{}' to a canonical path: {e}",
                file_path.display()
            ))
        })?;
        self.repo
            .find_by_path(&resolved)
            .await
            .map_err(CoreError::from)
    }

    /// Update a model.
    pub async fn update(&self, model: &Model) -> Result<(), CoreError> {
        self.repo.update(model).await.map_err(CoreError::from)
    }

    /// Delete a model by ID.
    pub async fn delete(&self, id: i64) -> Result<(), CoreError> {
        self.repo.delete(id).await.map_err(CoreError::from)
    }

    // ─────────────────────────────────────────────────────────────────────────
    // Tag Operations
    // ─────────────────────────────────────────────────────────────────────────

    /// List all unique tags used across all models.
    pub async fn list_tags(&self) -> Result<Vec<String>, CoreError> {
        let models = self.repo.list().await.map_err(CoreError::from)?;
        let mut all_tags = std::collections::HashSet::new();
        for model in models {
            for tag in model.tags {
                all_tags.insert(tag);
            }
        }
        let mut tags: Vec<String> = all_tags.into_iter().collect();
        tags.sort();
        Ok(tags)
    }

    /// Add a tag to a model.
    ///
    /// If the tag already exists on the model, this is a no-op.
    pub async fn add_tag(&self, model_id: i64, tag: String) -> Result<(), CoreError> {
        let mut model = self
            .repo
            .get_by_id(model_id)
            .await
            .map_err(CoreError::from)?;
        if !model.tags.contains(&tag) {
            model.tags.push(tag);
            model.tags.sort();
            self.repo.update(&model).await.map_err(CoreError::from)?;
        }
        Ok(())
    }

    /// Remove a tag from a model.
    ///
    /// If the tag doesn't exist on the model, this is a no-op.
    pub async fn remove_tag(&self, model_id: i64, tag: &str) -> Result<(), CoreError> {
        let mut model = self
            .repo
            .get_by_id(model_id)
            .await
            .map_err(CoreError::from)?;
        model.tags.retain(|t| t != tag);
        self.repo.update(&model).await.map_err(CoreError::from)?;
        Ok(())
    }

    /// Get all tags for a specific model.
    pub async fn get_tags(&self, model_id: i64) -> Result<Vec<String>, CoreError> {
        let model = self
            .repo
            .get_by_id(model_id)
            .await
            .map_err(CoreError::from)?;
        Ok(model.tags)
    }

    // ─────────────────────────────────────────────────────────────────────────
    // Capability Bootstrap
    // ─────────────────────────────────────────────────────────────────────────

    /// Backfill capabilities for models that don't have them set.
    ///
    /// This runs on startup to handle models with unknown capabilities.
    /// Only infers if capabilities are empty (0/unknown).
    ///
    /// # INVARIANT
    ///
    /// Never overwrite explicitly-set capabilities. Only infer when unknown.
    pub async fn bootstrap_capabilities(&self) -> Result<(), CoreError> {
        use crate::domain::{capabilities_from_architecture, infer_from_chat_template};

        let models = self.repo.list().await.map_err(CoreError::from)?;

        for mut model in models {
            // Only infer if capabilities are unknown (empty)
            if model.capabilities.is_empty() {
                let template = model.metadata.get("tokenizer.chat_template");
                let name = model.metadata.get("general.name");
                let arch = model.metadata.get("general.architecture");
                let from_template = infer_from_chat_template(
                    template.map(String::as_str),
                    name.map(String::as_str),
                );
                let from_arch = capabilities_from_architecture(arch.map(String::as_str));
                model.capabilities = from_template | from_arch;
                self.repo.update(&model).await.map_err(CoreError::from)?;
            }
        }

        Ok(())
    }

    // ─────────────────────────────────────────────────────────────────────────
    // Retag
    // ─────────────────────────────────────────────────────────────────────────

    /// Re-derive auto-tags for a single model from its persisted GGUF metadata.
    ///
    /// `full = false` (default) is **additive**: any newly-detected tag that
    /// isn't already present is appended; nothing is ever removed.
    ///
    /// `full = true` performs a full rebuild: every auto-generated tag (the
    /// predefined capability tag namespace) is dropped and the
    /// freshly-detected set is added in its place.
    /// User-curated tags outside that namespace are preserved.
    ///
    /// A model with no image family has one read from its file's tensor
    /// table, when the file is there and names one; a family already set is
    /// never changed, in either mode. A model without its type or bits per
    /// weight has them read from its file's header the same way, every shard
    /// of a split summed.
    ///
    /// Returns `None` when the tag set is unchanged (no write occurred) and
    /// `Some(diff)` when the model was updated, carrying the full added/removed
    /// delta.
    pub async fn retag_model(
        &self,
        model_id: i64,
        gguf_parser: &dyn GgufParserPort,
        full: bool,
    ) -> Result<Option<RetagDiff>, CoreError> {
        let mut model = self
            .repo
            .get_by_id(model_id)
            .await
            .map_err(CoreError::from)?;

        // Re-derive capabilities from the persisted metadata blob; the file
        // doesn't have to exist on disk.
        let gguf_metadata = crate::domain::gguf::GgufMetadata {
            metadata: model.metadata.clone(),
            ..Default::default()
        };
        let caps = gguf_parser.detect_capabilities(&gguf_metadata);
        let new_tags = caps.to_tags();

        let family_found = super::model_components::sniff_missing_family(&mut model, gguf_parser);
        let facts_read = super::model_header_facts::fill_missing_facts(&mut model, gguf_parser);

        let before: std::collections::BTreeSet<String> = model.tags.iter().cloned().collect();

        if full {
            // Drop every tag in the auto-generated namespace, then re-add.
            // The list lives with the constants that produce it: a tag missing
            // from it survives a refresh forever, silently keeping a
            // capability the model does not have.
            model
                .tags
                .retain(|t| !crate::domain::capability_tags::ALL.contains(&t.as_str()));
        }

        for t in &new_tags {
            if !model.tags.contains(t) {
                model.tags.push(t.clone());
            }
        }
        model.tags.sort();

        let after: std::collections::BTreeSet<String> = model.tags.iter().cloned().collect();
        if after == before && family_found.is_none() && !facts_read {
            return Ok(None);
        }

        self.repo.update(&model).await.map_err(CoreError::from)?;
        Ok(Some(RetagDiff {
            added: after.difference(&before).cloned().collect(),
            removed: before.difference(&after).cloned().collect(),
            family_found,
            facts_read,
        }))
    }
}

#[cfg(test)]
#[allow(clippy::float_cmp)] // exact literal round-trip through param_count_b, no lossy conversion
mod tests {
    use super::*;
    use crate::ports::{ModelRepository, RepositoryError};
    use async_trait::async_trait;
    use chrono::Utc;

    use std::path::PathBuf;
    use std::sync::Mutex;

    struct MockRepo {
        models: Mutex<Vec<Model>>,
    }

    impl MockRepo {
        fn new() -> Self {
            Self {
                models: Mutex::new(vec![]),
            }
        }
    }

    #[async_trait]
    impl ModelRepository for MockRepo {
        async fn list(&self) -> Result<Vec<Model>, RepositoryError> {
            Ok(self.models.lock().unwrap().clone())
        }

        async fn get_by_id(&self, id: i64) -> Result<Model, RepositoryError> {
            self.models
                .lock()
                .unwrap()
                .iter()
                .find(|m| m.id == id)
                .cloned()
                .ok_or_else(|| RepositoryError::NotFound(format!("id={id}")))
        }

        async fn get_by_name(&self, name: &str) -> Result<Model, RepositoryError> {
            self.models
                .lock()
                .unwrap()
                .iter()
                .find(|m| m.name == name)
                .cloned()
                .ok_or_else(|| RepositoryError::NotFound(format!("name={name}")))
        }

        async fn find_by_path(&self, path: &Path) -> Result<Option<Model>, RepositoryError> {
            Ok(self
                .models
                .lock()
                .unwrap()
                .iter()
                .find(|m| m.file_path.as_path() == path)
                .cloned())
        }

        #[allow(clippy::cast_possible_wrap, clippy::significant_drop_tightening)]
        async fn insert(&self, model: &NewModel) -> Result<Model, RepositoryError> {
            let mut models = self.models.lock().unwrap();
            // NOTE: this double keys on `file_path`; `SqliteModelRepository`
            // keys on `model_key`. They agree for a plain local add and
            // diverge whenever the key is derived from something else — a
            // downloaded model keyed `hf:<repo>@<sha>#<file>`, most of all. A
            // test written here will therefore report "one row, id kept" for
            // input the real repository would have duplicated, so anything
            // turning on key identity belongs in a test against the real
            // repository (see `gglib-app-services`'s
            // `forcing_a_downloaded_model_refreshes_it_rather_than_duplicating_it`).
            //
            // Mirror the `SQLite` repository: registering the same file twice
            // updates that row and keeps its id rather than appending a second
            // one. A double that appends contradicts the trait doc and makes
            // the silent-overwrite bug this guard exists for invisible to
            // every test written against it.
            let existing = models.iter().position(|m| m.file_path == model.file_path);
            let id = existing.map_or(models.len() as i64 + 1, |i| models[i].id);
            let created = Model::stored(id, model);
            if let Some(index) = existing {
                models[index] = created.clone();
            } else {
                models.push(created.clone());
            }
            Ok(created)
        }

        async fn update(&self, model: &Model) -> Result<(), RepositoryError> {
            let mut models = self.models.lock().unwrap();
            models.iter_mut().find(|m| m.id == model.id).map_or_else(
                || Err(RepositoryError::NotFound(format!("id={}", model.id))),
                |m| {
                    m.clone_from(model);
                    Ok(())
                },
            )
        }

        async fn delete(&self, id: i64) -> Result<(), RepositoryError> {
            let mut models = self.models.lock().unwrap();
            let len_before = models.len();
            models.retain(|m| m.id != id);
            if models.len() == len_before {
                Err(RepositoryError::NotFound(format!("id={id}")))
            } else {
                Ok(())
            }
        }
    }

    #[tokio::test]
    async fn test_import_from_file_names_from_stem() {
        let repo = Arc::new(MockRepo::new());
        let service = ModelService::new(repo);

        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("Qwen3-8B-Q4_K_M.gguf");
        std::fs::File::create(&path).unwrap();

        let model = service
            .import_from_file(
                &path,
                &crate::ports::NoopGgufParser,
                None,
                ImportMode::Fresh,
            )
            .await
            .unwrap();

        assert_eq!(model.name, "Qwen3-8B-Q4_K_M");
        assert_eq!(model.hf_repo_id, None);
    }

    /// Adding a file already in the library is a conflict. `insert` upserts on
    /// the model key, so without the lookup the second add would overwrite the
    /// first row and tell the caller the model had been added.
    #[tokio::test]
    async fn importing_the_same_file_twice_is_a_conflict() {
        let repo = Arc::new(MockRepo::new());
        let service = ModelService::new(repo);

        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("Qwen3-8B-Q4_K_M.gguf");
        std::fs::File::create(&path).unwrap();

        service
            .import_from_file(
                &path,
                &crate::ports::NoopGgufParser,
                None,
                ImportMode::Fresh,
            )
            .await
            .expect("first add succeeds");

        let err = service
            .import_from_file(
                &path,
                &crate::ports::NoopGgufParser,
                None,
                ImportMode::Fresh,
            )
            .await
            .expect_err("second add is a conflict");

        assert!(
            matches!(
                err,
                CoreError::Repository(RepositoryError::AlreadyExists(_))
            ),
            "expected AlreadyExists, got {err:?}"
        );
    }

    /// `--reimport` is the documented way to refresh a model's derived columns
    /// from the file. The guard above blocks the workflow `docs/tags.md`
    /// describes, so `Refresh` has to restore it: same file, same row, no
    /// conflict.
    #[tokio::test]
    async fn refresh_re_imports_a_file_already_in_the_library() {
        let repo = Arc::new(MockRepo::new());
        let service = ModelService::new(repo);

        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("Qwen3-8B-Q4_K_M.gguf");
        std::fs::File::create(&path).unwrap();

        let first = service
            .import_from_file(
                &path,
                &crate::ports::NoopGgufParser,
                None,
                ImportMode::Fresh,
            )
            .await
            .expect("first add succeeds");

        let refreshed = service
            .import_from_file(
                &path,
                &crate::ports::NoopGgufParser,
                None,
                ImportMode::Refresh,
            )
            .await
            .expect("--reimport re-imports rather than refusing");

        assert_eq!(
            refreshed.id, first.id,
            "a refresh updates the row in place; it does not create a second"
        );
        assert_eq!(service.list().await.unwrap().len(), 1);
    }

    /// The lookup a caller can run before doing expensive or interactive work
    /// has to agree with the guard inside the import, or the CLI would refuse
    /// files the import would accept and vice versa.
    #[tokio::test]
    async fn find_by_path_agrees_with_the_import_guard() {
        let repo = Arc::new(MockRepo::new());
        let service = ModelService::new(repo);

        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("Qwen3-8B-Q4_K_M.gguf");
        std::fs::File::create(&path).unwrap();

        assert!(
            service.find_by_path(&path).await.unwrap().is_none(),
            "nothing is registered yet"
        );

        service
            .import_from_file(
                &path,
                &crate::ports::NoopGgufParser,
                None,
                ImportMode::Fresh,
            )
            .await
            .unwrap();

        assert!(
            service.find_by_path(&path).await.unwrap().is_some(),
            "the file the import just refused to duplicate must be findable"
        );
    }

    /// An unresolvable path is an error, never `Ok(None)`. `Ok(None)` reads
    /// as "no duplicate" and silently reinstates the overwrite the guard
    /// exists to prevent.
    #[tokio::test]
    async fn find_by_path_reports_an_unresolvable_path_instead_of_no_duplicate() {
        let repo = Arc::new(MockRepo::new());
        let service = ModelService::new(repo);

        let dir = tempfile::tempdir().unwrap();
        let err = service
            .find_by_path(&dir.path().join("Absent.gguf"))
            .await
            .expect_err("a path that does not resolve is not 'no duplicate'");

        assert!(matches!(err, CoreError::Validation(_)), "got {err:?}");
    }

    /// A repository that matches any path, standing in for the sibling-shard
    /// arm of the real `find_by_path` — the one case where the row handed back
    /// is *not* the row the queried path is the primary file of.
    struct SiblingMatchRepo(MockRepo);

    #[async_trait]
    impl ModelRepository for SiblingMatchRepo {
        async fn list(&self) -> Result<Vec<Model>, RepositoryError> {
            self.0.list().await
        }
        async fn get_by_id(&self, id: i64) -> Result<Model, RepositoryError> {
            self.0.get_by_id(id).await
        }
        async fn get_by_name(&self, name: &str) -> Result<Model, RepositoryError> {
            self.0.get_by_name(name).await
        }
        async fn find_by_path(&self, _path: &Path) -> Result<Option<Model>, RepositoryError> {
            Ok(self.0.list().await?.into_iter().next())
        }
        async fn insert(&self, model: &NewModel) -> Result<Model, RepositoryError> {
            self.0.insert(model).await
        }
        async fn update(&self, model: &Model) -> Result<(), RepositoryError> {
            self.0.update(model).await
        }
        async fn delete(&self, id: i64) -> Result<(), RepositoryError> {
            self.0.delete(id).await
        }
    }

    /// The refusal resolves both sides, so a row whose stored path is spelled
    /// differently from the caller's — while naming the same file — is still
    /// recognised as that caller's own model and refreshed.
    ///
    /// That state is reachable: `canonical_model_path_string` falls back to
    /// the literal path when the file cannot be resolved, so a row registered
    /// while its file was missing keeps whatever spelling it was given.
    /// Comparing the stored column directly would refuse a refresh of the
    /// model's own file and print the same path on both sides of the error.
    #[tokio::test]
    async fn a_refresh_is_accepted_when_the_stored_path_is_only_spelled_differently() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("Solo.gguf");
        std::fs::File::create(&file).unwrap();
        std::fs::create_dir(dir.path().join("sub")).unwrap();
        let respelled = dir.path().join("sub").join("..").join("Solo.gguf");

        // Seed the row under the indirect spelling, as a registration made
        // while the file was unresolvable would have.
        let inner = MockRepo::new();
        inner
            .insert(&NewModel::new(
                "Solo".to_string(),
                respelled,
                Some(7.0),
                Utc::now(),
            ))
            .await
            .unwrap();

        let service = ModelService::new(Arc::new(SiblingMatchRepo(inner)));
        let outcome = service
            .import_from_file(
                &file,
                &crate::ports::NoopGgufParser,
                None,
                ImportMode::Refresh,
            )
            .await;

        // Only that the guard *lets it through* is asserted here. Which row it
        // then lands on turns on key identity, and `MockRepo` upserts on
        // `file_path` while the real repository upserts on `model_key` — the
        // divergence noted above. Under this double the two spellings look
        // like two rows; under the real repository both resolve to one key.
        // The landing is covered against the real repository by
        // gglib-app-services' `forcing_from_a_sibling_shard_refuses_rather_than_repointing`.
        assert!(
            outcome.is_ok(),
            "the row names this very file, however it was spelled: {:?}",
            outcome.err()
        );
    }

    /// The other side of the same guard: when the located row names a
    /// genuinely different file — the sharded case, where the sibling arm
    /// returns the shard-1 row — the refresh is refused rather than
    /// repointing it.
    #[tokio::test]
    async fn a_refresh_is_refused_when_the_located_row_names_another_file() {
        let dir = tempfile::tempdir().unwrap();
        let first = dir.path().join("m-00001-of-00002.gguf");
        let second = dir.path().join("m-00002-of-00002.gguf");
        std::fs::File::create(&first).unwrap();
        std::fs::File::create(&second).unwrap();

        let inner = MockRepo::new();
        inner
            .insert(&NewModel::new(
                "Sharded".to_string(),
                first,
                Some(7.0),
                Utc::now(),
            ))
            .await
            .unwrap();

        let service = ModelService::new(Arc::new(SiblingMatchRepo(inner)));
        let err = service
            .import_from_file(
                &second,
                &crate::ports::NoopGgufParser,
                None,
                ImportMode::Refresh,
            )
            .await
            .expect_err("shard 2 must not repoint the shard-1 row");

        assert!(matches!(err, CoreError::Validation(_)), "got {err:?}");
    }

    /// A second *different* file must still go in — the check keys on the
    /// path, not on "the library is non-empty".
    #[tokio::test]
    async fn a_different_file_is_not_a_conflict() {
        let repo = Arc::new(MockRepo::new());
        let service = ModelService::new(repo);

        let dir = tempfile::tempdir().unwrap();
        for name in ["a.gguf", "b.gguf"] {
            let path = dir.path().join(name);
            std::fs::File::create(&path).unwrap();
            service
                .import_from_file(
                    &path,
                    &crate::ports::NoopGgufParser,
                    None,
                    ImportMode::Fresh,
                )
                .await
                .unwrap_or_else(|e| panic!("{name} should import: {e:?}"));
        }
    }

    #[tokio::test]
    async fn test_import_from_file_missing_path_is_validation_error() {
        let repo = Arc::new(MockRepo::new());
        let service = ModelService::new(repo);

        let err = service
            .import_from_file(
                Path::new("/nonexistent/model.gguf"),
                &crate::ports::NoopGgufParser,
                None,
                ImportMode::Fresh,
            )
            .await
            .unwrap_err();
        assert!(matches!(err, CoreError::Validation(_)));
    }

    #[tokio::test]
    async fn test_import_from_file_wrong_extension_is_validation_error() {
        let repo = Arc::new(MockRepo::new());
        let service = ModelService::new(repo);

        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("model.bin");
        std::fs::File::create(&path).unwrap();

        let err = service
            .import_from_file(
                &path,
                &crate::ports::NoopGgufParser,
                None,
                ImportMode::Fresh,
            )
            .await
            .unwrap_err();
        assert!(matches!(err, CoreError::Validation(_)));
    }

    #[tokio::test]
    async fn test_import_from_file_param_override_reaches_new_model() {
        let repo = Arc::new(MockRepo::new());
        let service = ModelService::new(repo);

        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("model.gguf");
        std::fs::File::create(&path).unwrap();

        let model = service
            .import_from_file(
                &path,
                &crate::ports::NoopGgufParser,
                Some(13.0),
                ImportMode::Fresh,
            )
            .await
            .unwrap();

        assert_eq!(model.param_count_b, Some(13.0));
    }

    #[tokio::test]
    async fn test_list_empty() {
        let repo = Arc::new(MockRepo::new());
        let service = ModelService::new(repo);
        let models = service.list().await.unwrap();
        assert!(models.is_empty());
    }

    #[tokio::test]
    async fn test_add_and_get() {
        let repo = Arc::new(MockRepo::new());
        let service = ModelService::new(repo);

        let new_model = NewModel::new(
            "test-model".to_string(),
            PathBuf::from("/path/to/model.gguf"),
            Some(7.0),
            Utc::now(),
        );

        let created = service.add(new_model).await.unwrap();
        assert_eq!(created.name, "test-model");

        let found = service.get("test-model").await.unwrap();
        assert!(found.is_some());
        assert_eq!(found.unwrap().id, created.id);
    }

    /// A miss is no model, not an error: what to say about one is the
    /// caller's.
    #[tokio::test]
    async fn an_identifier_that_matches_nothing_is_no_model() {
        let service = ModelService::new(Arc::new(MockRepo::new()));

        let found = service.get("nonexistent").await.unwrap();

        assert!(found.is_none());
    }

    #[tokio::test]
    async fn test_remove_tag_allows_user_tag() {
        let repo = Arc::new(MockRepo::new());
        let service = ModelService::new(repo);

        let mut new_model = NewModel::new(
            "u".to_string(),
            PathBuf::from("/p.gguf"),
            Some(7.0),
            Utc::now(),
        );
        new_model.tags = vec!["chat".to_string(), "favorite".to_string()];
        let created = service.add(new_model).await.unwrap();

        service.remove_tag(created.id, "chat").await.unwrap();
        let tags = service.get_tags(created.id).await.unwrap();
        assert_eq!(tags, vec!["favorite".to_string()]);
    }

    /// Stub parser that emits a fixed capability set for retag tests.
    struct StubCapsParser {
        tags: Vec<String>,
    }

    impl crate::ports::GgufParserPort for StubCapsParser {
        fn parse(
            &self,
            _file_path: &std::path::Path,
        ) -> std::result::Result<crate::ports::GgufMetadata, crate::ports::GgufParseError> {
            Ok(crate::ports::GgufMetadata::default())
        }

        fn detect_capabilities(
            &self,
            _metadata: &crate::ports::GgufMetadata,
        ) -> crate::ports::GgufCapabilities {
            let mut flags = crate::domain::gguf::CapabilityFlags::empty();
            if self.tags.iter().any(|t| t == "agent") {
                flags |= crate::domain::gguf::CapabilityFlags::TOOL_CALLING;
            }
            crate::ports::GgufCapabilities { flags }
        }

        fn tensor_table(
            &self,
            _path: &std::path::Path,
        ) -> std::result::Result<crate::ports::TensorTable, crate::ports::GgufParseError> {
            Err(crate::ports::GgufParseError::InvalidFormat(
                "no tensor table".to_owned(),
            ))
        }

        fn tensor_table_of_head(
            &self,
            _head: &[u8],
        ) -> std::result::Result<crate::ports::TensorTable, crate::ports::GgufParseError> {
            Err(crate::ports::GgufParseError::InvalidFormat(
                "no tensor table".to_owned(),
            ))
        }

        fn parse_head(
            &self,
            _head: &[u8],
            _file_name: &str,
        ) -> std::result::Result<crate::ports::HeadMetadata, crate::ports::GgufParseError> {
            Err(crate::ports::GgufParseError::InvalidFormat(
                "no head is read".to_owned(),
            ))
        }
    }

    #[tokio::test]
    async fn test_retag_additive_appends_missing_tags() {
        let repo = Arc::new(MockRepo::new());
        let service = ModelService::new(repo);

        let mut new_model = NewModel::new(
            "m".to_string(),
            PathBuf::from("/p.gguf"),
            Some(7.0),
            Utc::now(),
        );
        new_model.tags = vec!["chat".to_string()];
        let created = service.add(new_model).await.unwrap();

        let parser = StubCapsParser {
            tags: vec!["agent".to_string()],
        };
        let diff = service
            .retag_model(created.id, &parser, false)
            .await
            .unwrap();
        assert_eq!(diff.unwrap().added, vec!["agent".to_string()]);

        let tags = service.get_tags(created.id).await.unwrap();
        assert!(tags.contains(&"chat".to_string()));
        assert!(tags.contains(&"agent".to_string()));
    }

    #[tokio::test]
    async fn test_retag_additive_noop_when_already_present() {
        let repo = Arc::new(MockRepo::new());
        let service = ModelService::new(repo);

        let mut new_model = NewModel::new(
            "m".to_string(),
            PathBuf::from("/p.gguf"),
            Some(7.0),
            Utc::now(),
        );
        new_model.tags = vec!["agent".to_string()];
        let created = service.add(new_model).await.unwrap();

        let parser = StubCapsParser {
            tags: vec!["agent".to_string()],
        };
        let diff = service
            .retag_model(created.id, &parser, false)
            .await
            .unwrap();
        assert!(diff.is_none());
    }

    #[tokio::test]
    async fn test_retag_full_replaces_auto_tags_preserves_user() {
        let repo = Arc::new(MockRepo::new());
        let service = ModelService::new(repo);

        let mut new_model = NewModel::new(
            "m".to_string(),
            PathBuf::from("/p.gguf"),
            Some(7.0),
            Utc::now(),
        );
        new_model.tags = vec![
            "favorite".to_string(),  // user
            "reasoning".to_string(), // stale auto capability
        ];
        let created = service.add(new_model).await.unwrap();

        let parser = StubCapsParser {
            tags: vec!["agent".to_string()],
        };
        service
            .retag_model(created.id, &parser, true)
            .await
            .unwrap();

        let tags = service.get_tags(created.id).await.unwrap();
        assert!(tags.contains(&"favorite".to_string()));
        assert!(tags.contains(&"agent".to_string()));
        assert!(!tags.contains(&"reasoning".to_string()));
    }

    #[tokio::test]
    async fn test_retag_full_drops_stale_mtp_tag() {
        let repo = Arc::new(MockRepo::new());
        let service = ModelService::new(repo);

        let mut new_model = NewModel::new(
            "m".to_string(),
            PathBuf::from("/p.gguf"),
            Some(7.0),
            Utc::now(),
        );
        new_model.tags = vec!["mtp".to_string()]; // stale auto capability
        let created = service.add(new_model).await.unwrap();

        // Detection reports no MTP support.
        let parser = StubCapsParser { tags: Vec::new() };
        service
            .retag_model(created.id, &parser, true)
            .await
            .unwrap();

        let tags = service.get_tags(created.id).await.unwrap();
        assert!(!tags.contains(&"mtp".to_string()));
    }
}
