//! Model CRUD operations for GUI backend.

use std::path::PathBuf;
use std::sync::Arc;

use gglib_core::ports::{GgufParserPort, ModelRuntimePort};
use gglib_core::services::AppCore;
use gglib_core::{
    ModelCapabilities, ModelFilterOptions,
    domain::{ModelListQuery, apply_query},
};

use crate::error::GuiError;
use crate::sampling_explain::{self, SamplingExplanationDto};
use crate::types::{
    AddModelRequest, GuiModel, ModelDetailDto, RemoveModelRequest, RetagResponse,
    SetCapabilitiesRequest, UpdateModelRequest, UpgradeCheck, UpgradeOutcome,
};

/// Dependencies for model operations.
pub struct ModelDeps {
    pub core: Arc<AppCore>,
    /// The runtime backing server lifecycle — the same one `ServerOps` starts
    /// models through, so serving status here agrees with what `ServerOps`
    /// actually has running rather than a second, independent registry.
    pub runtime: Arc<dyn ModelRuntimePort>,
    pub gguf_parser: Arc<dyn GgufParserPort>,
}

/// Model operations handler.
pub struct ModelOps {
    deps: ModelDeps,
}

impl ModelOps {
    pub fn new(deps: ModelDeps) -> Self {
        Self { deps }
    }

    /// Check if a model is currently being served.
    async fn get_server_status(&self, model_id: i64) -> (bool, Option<u16>) {
        self.deps
            .runtime
            .list_running()
            .await
            .into_iter()
            .find(|h| h.model_id == model_id)
            .map_or((false, None), |h| (true, Some(h.port)))
    }

    /// List all models with their serving status.
    pub async fn list(&self) -> Result<Vec<GuiModel>, GuiError> {
        let models = self
            .deps
            .core
            .models()
            .list()
            .await
            .map_err(|e| GuiError::Internal(format!("Failed to list models: {e}")))?;

        let mut gui_models = Vec::new();
        for model in models {
            let (is_serving, port) = self.get_server_status(model.id).await;
            gui_models.push(GuiModel::from_model(model, is_serving, port));
        }

        Ok(gui_models)
    }

    /// List models filtered and sorted by the given query.
    ///
    /// Fetches all models from the repository, applies [`apply_query`] (the
    /// single source of truth for filter/sort semantics), then enriches each
    /// surviving model with its current serving status.
    pub async fn list_with_query(&self, query: ModelListQuery) -> Result<Vec<GuiModel>, GuiError> {
        let models = self
            .deps
            .core
            .models()
            .list()
            .await
            .map_err(|e| GuiError::Internal(format!("Failed to list models: {e}")))?;

        let filtered = apply_query(models, &query);

        let mut gui_models = Vec::new();
        for model in filtered {
            let (is_serving, port) = self.get_server_status(model.id).await;
            gui_models.push(GuiModel::from_model(model, is_serving, port));
        }

        Ok(gui_models)
    }

    /// Get a specific model by ID.
    pub async fn get(&self, id: i64) -> Result<GuiModel, GuiError> {
        let model = crate::helpers::resolve_model(self.deps.core.models(), id).await?;
        let (is_serving, port) = self.get_server_status(id).await;
        Ok(GuiModel::from_model(model, is_serving, port))
    }

    /// Get full details for a model by ID, for the inspect view.
    ///
    /// Returns a [`ModelDetailDto`] — a superset of [`GuiModel`] that
    /// includes raw GGUF metadata, MoE topology, and full HuggingFace
    /// provenance.  This is the shared data source for the CLI
    /// `model inspect` command and the `GET /api/models/:id/detail` route.
    pub async fn get_detail(&self, id: i64) -> Result<ModelDetailDto, GuiError> {
        let model = crate::helpers::resolve_model(self.deps.core.models(), id).await?;
        let (is_serving, port) = self.get_server_status(id).await;
        Ok(ModelDetailDto::from_model(model, is_serving, port))
    }

    /// Resolve a model's sampling parameters and report which layer supplied
    /// each one.
    ///
    /// The shared data source for the CLI `model explain` command and the
    /// `GET /api/models/:id/explain` route. `profile` names a configured
    /// [`InferenceProfile`] to apply on top of the model's own defaults;
    /// an unknown name is a [`GuiError::ValidationFailed`] rather than a
    /// silent fall back to the unprofiled resolution.
    ///
    /// [`InferenceProfile`]: gglib_core::domain::InferenceProfile
    pub async fn explain_sampling(
        &self,
        id: i64,
        profile: Option<&str>,
    ) -> Result<SamplingExplanationDto, GuiError> {
        let model = crate::helpers::resolve_model(self.deps.core.models(), id).await?;
        let settings = self
            .deps
            .core
            .settings()
            .get()
            .await
            .map_err(|e| GuiError::Internal(format!("Failed to load settings: {e}")))?;

        let selected = profile
            .map(|name| {
                sampling_explain::find_profile(name, settings.inference_profiles.as_deref())
            })
            .transpose()?;

        Ok(sampling_explain::explain(&model, &settings, selected))
    }

    pub async fn add(&self, request: AddModelRequest) -> Result<GuiModel, GuiError> {
        let path = PathBuf::from(&request.file_path);

        // Delegate to shared core logic for model import with full metadata
        // extraction. Always `Fresh`: the HTTP surface has no way to ask for
        // the destructive re-import, so a duplicate is always a 409 here. The
        // refresh workflow is `gglib model add --force`, which is explicit
        // about overwriting a row the caller already has.
        let model = self
            .deps
            .core
            .models()
            .import_from_file(
                &path,
                self.deps.gguf_parser.as_ref(),
                None,
                gglib_core::services::ImportMode::Fresh,
            )
            .await
            .map_err(|e| match e {
                gglib_core::ports::CoreError::Validation(msg) => GuiError::ValidationFailed(msg),
                gglib_core::ports::CoreError::Repository(
                    gglib_core::ports::RepositoryError::AlreadyExists(_),
                ) => GuiError::Conflict(format!(
                    "Model at path '{}' already exists in database",
                    request.file_path
                )),
                _ => GuiError::Internal(format!("Failed to add model: {e}")),
            })?;

        // Return with serving status
        let (is_serving, port) = self.get_server_status(model.id).await;
        Ok(GuiModel::from_model(model, is_serving, port))
    }

    /// Update a model in the database.
    pub async fn update(&self, id: i64, request: UpdateModelRequest) -> Result<GuiModel, GuiError> {
        let mut model = crate::helpers::resolve_model(self.deps.core.models(), id).await?;

        if let Some(name) = request.name {
            model.name = name;
        }
        if let Some(quantization) = request.quantization {
            model.quantization = Some(quantization);
        }
        if let Some(file_path) = request.file_path {
            model.file_path = PathBuf::from(file_path);
        }
        if let Some(inference_defaults) = request.inference_defaults {
            model.inference_defaults = Some(inference_defaults);
            // A deliberate WebUI edit, so this is a user-set value from
            // here on — even if it happens to land on the same numbers
            // gglib would have guessed. See `DefaultsOrigin`.
            model.defaults_origin = Some(gglib_core::domain::DefaultsOrigin::User);
        }
        match request.server_defaults {
            Some(Some(config)) => model.server_defaults = Some(config),
            Some(None) => model.server_defaults = None,
            None => {} // don't touch
        }

        self.deps
            .core
            .models()
            .update(&model)
            .await
            .map_err(|e| GuiError::Internal(format!("Failed to update model: {e}")))?;

        // Answer with the row as stored, not as sent. `update` canonicalises
        // `file_path` on write, so echoing the in-memory copy would hand back
        // the caller's spelling and disagree with the very next GET.
        let stored = crate::helpers::resolve_model(self.deps.core.models(), id).await?;
        Ok(GuiModel::from_domain(stored))
    }

    /// Remove a model from the database.
    pub async fn remove(&self, id: i64, request: RemoveModelRequest) -> Result<String, GuiError> {
        let model = crate::helpers::resolve_model(self.deps.core.models(), id).await?;

        let running = self
            .deps
            .runtime
            .list_running()
            .await
            .into_iter()
            .find(|h| h.model_id == id);

        if let Some(handle) = running {
            if !request.force {
                return Err(GuiError::Conflict(format!(
                    "Model is currently serving on port {}. Stop the server first or use force=true",
                    handle.port
                )));
            }
            self.deps
                .runtime
                .stop_current()
                .await
                .map_err(|e| GuiError::Internal(format!("Failed to stop server: {e}")))?;
        }

        self.deps
            .core
            .models()
            .delete(id)
            .await
            .map_err(|e| GuiError::Internal(format!("Failed to delete model: {e}")))?;

        Ok(format!("Model '{}' removed successfully", model.name))
    }

    /// List all unique tags.
    pub async fn list_tags(&self) -> Result<Vec<String>, GuiError> {
        self.deps
            .core
            .models()
            .list_tags()
            .await
            .map_err(|e| GuiError::Internal(format!("Failed to list tags: {e}")))
    }

    /// Add a tag to a model.
    pub async fn add_tag(&self, model_id: i64, tag: String) -> Result<(), GuiError> {
        self.deps
            .core
            .models()
            .add_tag(model_id, tag)
            .await
            .map_err(|e| GuiError::Internal(format!("Failed to add tag: {e}")))
    }

    /// Remove a tag from a model.
    pub async fn remove_tag(&self, model_id: i64, tag: String) -> Result<(), GuiError> {
        self.deps
            .core
            .models()
            .remove_tag(model_id, &tag)
            .await
            .map_err(|e| GuiError::Internal(format!("Failed to remove tag: {e}")))
    }

    /// Get all tags for a specific model.
    pub async fn get_tags(&self, model_id: i64) -> Result<Vec<String>, GuiError> {
        self.deps
            .core
            .models()
            .get_tags(model_id)
            .await
            .map_err(|e| GuiError::Internal(format!("Failed to get tags: {e}")))
    }

    /// Get filter options for the model library UI.
    pub async fn get_filter_options(&self) -> Result<ModelFilterOptions, GuiError> {
        self.deps
            .core
            .models()
            .get_filter_options()
            .await
            .map_err(|e| GuiError::Internal(format!("Failed to get filter options: {e}")))
    }

    /// Override one or more capability flags on a model.
    ///
    /// Each field in [`SetCapabilitiesRequest`] independently sets or clears
    /// one [`ModelCapabilities`] bit.  `None` fields are left unchanged.
    /// The result is persisted to the database and returned as an updated
    /// [`GuiModel`].
    ///
    /// This is the **single shared implementation** called by the CLI, the
    /// Axum WebUI, and the Tauri app.  No business logic lives in the surface
    /// crates.
    pub async fn set_capabilities(
        &self,
        id: i64,
        request: SetCapabilitiesRequest,
    ) -> Result<GuiModel, GuiError> {
        let mut model = crate::helpers::resolve_model(self.deps.core.models(), id).await?;

        let mut caps = model.capabilities;

        if let Some(v) = request.supports_system_role {
            caps.set(ModelCapabilities::SUPPORTS_SYSTEM_ROLE, v);
        }
        if let Some(v) = request.requires_strict_turns {
            caps.set(ModelCapabilities::REQUIRES_STRICT_TURNS, v);
        }
        if let Some(v) = request.supports_tool_calls {
            caps.set(ModelCapabilities::SUPPORTS_TOOL_CALLS, v);
        }
        if let Some(v) = request.supports_reasoning {
            caps.set(ModelCapabilities::SUPPORTS_REASONING, v);
        }

        model.capabilities = caps;

        self.deps
            .core
            .models()
            .update(&model)
            .await
            .map_err(|e| GuiError::Internal(format!("Failed to update model capabilities: {e}")))?;

        Ok(GuiModel::from_domain(model))
    }

    /// Re-run capability detection over the model's stored GGUF metadata.
    ///
    /// `full = false` only adds missing tags; `full = true` rebuilds the
    /// system-tag namespace and re-derives the dialect spec. User-curated
    /// tags outside that namespace survive either way. Returns `changed:
    /// false` when the pass was a no-op.
    pub async fn retag(&self, id: i64, full: bool) -> Result<RetagResponse, GuiError> {
        // Resolve first so a stale id surfaces as NotFound, not Internal.
        crate::helpers::resolve_model(self.deps.core.models(), id).await?;
        let diff = self
            .deps
            .core
            .models()
            .retag_model(id, self.deps.gguf_parser.as_ref(), full)
            .await
            .map_err(|e| GuiError::Internal(format!("Retag failed: {e}")))?;

        Ok(match diff {
            Some(diff) => RetagResponse {
                changed: diff.is_changed(),
                added: diff.added,
                removed: diff.removed,
                spec_changed: diff.spec_changed,
            },
            None => RetagResponse {
                changed: false,
                added: Vec::new(),
                removed: Vec::new(),
                spec_changed: false,
            },
        })
    }

    /// Preconditions shared by the upgrade check and the upgrade itself.
    fn upgrade_source(model: &gglib_core::Model) -> Result<(String, String), GuiError> {
        let repo = model.hf_repo_id.clone().ok_or_else(|| {
            GuiError::ValidationFailed("Model is not from HuggingFace, cannot update".into())
        })?;
        let quant = model.quantization.clone().ok_or_else(|| {
            GuiError::ValidationFailed("Model has no quantization info stored".into())
        })?;
        Ok((repo, quant))
    }

    /// Whether a newer HuggingFace revision exists — the commit-SHA check
    /// `gglib model upgrade` runs before downloading, distinct from the
    /// shard-level diff on `/{id}/updates`.
    ///
    /// Not the same question as `gglib model check-updates`: with no recorded
    /// revision this reports `has_update: true` (nothing to compare against)
    /// where that command declines to answer. Callers should present a
    /// `current_sha` of `None` as "no baseline recorded", not as a new release.
    pub async fn check_upgrade(&self, id: i64) -> Result<UpgradeCheck, GuiError> {
        let model = crate::helpers::resolve_model(self.deps.core.models(), id).await?;
        let (repo, _quant) = Self::upgrade_source(&model)?;
        let models_dir = gglib_core::paths::resolve_models_dir(None)
            .map_err(|e| GuiError::Internal(format!("Could not resolve models dir: {e}")))?
            .path;

        let check = gglib_download::cli_exec::check_update(
            &repo,
            model.hf_commit_sha.as_deref(),
            &models_dir,
        )
        .await
        .map_err(|e| GuiError::Internal(format!("Update check failed: {e}")))?;

        Ok(UpgradeCheck {
            has_update: check.has_update,
            current_sha: check.current_sha,
            latest_sha: check.latest_sha,
        })
    }

    /// Re-download the model at the latest HuggingFace revision and rewrite
    /// the row — `gglib model upgrade`, shared by the CLI and the GUI route.
    ///
    /// Checks first and returns `updated: false` without downloading when the
    /// model is already current. The HF token comes from the process
    /// environment, matching the CLI. The call does not return until the
    /// download finishes; queue integration is future work, as is any
    /// serialisation between two upgrades of the same model (concurrent
    /// callers both download and the last one wins the row).
    pub async fn apply_upgrade(&self, id: i64) -> Result<UpgradeOutcome, GuiError> {
        let mut model = crate::helpers::resolve_model(self.deps.core.models(), id).await?;
        let (repo, quant) = Self::upgrade_source(&model)?;
        let models_dir = gglib_core::paths::resolve_models_dir(None)
            .map_err(|e| GuiError::Internal(format!("Could not resolve models dir: {e}")))?
            .path;

        let check = gglib_download::cli_exec::check_update(
            &repo,
            model.hf_commit_sha.as_deref(),
            &models_dir,
        )
        .await
        .map_err(|e| GuiError::Internal(format!("Update check failed: {e}")))?;
        if !check.has_update {
            return Ok(UpgradeOutcome {
                updated: false,
                latest_sha: check.latest_sha,
                file_path: None,
            });
        }

        // Detached deliberately. The forced re-download deletes the existing
        // file before writing its replacement, so if this ran inline in an
        // Axum request future a client disconnect would drop it mid-transfer
        // and leave the user with no model at all — while the row still
        // pointed at the deleted path. Spawning means the download and the row
        // rewrite always finish as a pair; only the reply is lost.
        let core = self.deps.core.clone();
        let request = gglib_download::cli_exec::CliUpdateRequest {
            model_path: model.file_path.clone(),
            repo_id: repo,
            quantization: quant,
            models_dir,
            token: std::env::var("HF_TOKEN").ok(),
        };

        tokio::spawn(async move {
            let result = gglib_download::cli_exec::update_model(request)
                .await
                .map_err(|e| GuiError::Internal(format!("Upgrade download failed: {e}")))?;

            model.file_path = result.primary_path.clone();
            model.hf_commit_sha = Some(result.commit_sha.clone());
            model.last_update_check = Some(chrono::Utc::now());
            core.models()
                .update(&model)
                .await
                .map_err(|e| GuiError::Internal(format!("Failed to update model row: {e}")))?;

            Ok(UpgradeOutcome {
                updated: true,
                latest_sha: result.commit_sha,
                file_path: Some(result.primary_path.display().to_string()),
            })
        })
        .await
        .map_err(|e| GuiError::Internal(format!("Upgrade task panicked: {e}")))?
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use tempfile::tempdir;
    use tokio::fs;

    use super::*;
    use crate::error::GuiError;
    use crate::sampling_explain::ProvenanceKindDto;
    use crate::test_support::test_core;
    use gglib_core::ports::{NoopGgufParser, NoopModelRuntime};

    fn make_ops(core: Arc<AppCore>) -> ModelOps {
        ModelOps::new(ModelDeps {
            core,
            runtime: Arc::new(NoopModelRuntime),
            gguf_parser: Arc::new(NoopGgufParser),
        })
    }

    #[tokio::test]
    async fn list_returns_empty_on_fresh_db() {
        let core = test_core().await;
        let ops = make_ops(core);
        let models = ops.list().await.expect("list should succeed");
        assert!(models.is_empty());
    }

    #[tokio::test]
    async fn get_unknown_id_returns_not_found() {
        let core = test_core().await;
        let ops = make_ops(core);
        let result = ops.get(999).await;
        assert!(
            matches!(
                result,
                Err(GuiError::NotFound {
                    entity: "model",
                    ..
                })
            ),
            "expected NotFound, got {result:?}"
        );
    }

    #[tokio::test]
    async fn add_and_list_model() {
        let core = test_core().await;
        let ops = make_ops(core);

        let dir = tempdir().unwrap();
        let gguf_path = dir.path().join("model.gguf");
        fs::write(&gguf_path, b"placeholder").await.unwrap();
        // Canonicalize to resolve macOS /var → /private/var symlinks so the
        // comparison matches the path the service stores after canonicalization.
        let gguf_path = gguf_path.canonicalize().unwrap();

        let req = AddModelRequest {
            file_path: gguf_path.to_str().unwrap().to_string(),
        };

        let added = ops.add(req).await.expect("add should succeed");
        let canonical = std::fs::canonicalize(&gguf_path).unwrap();
        assert_eq!(added.file_path, canonical.to_str().unwrap());

        let models = ops.list().await.unwrap();
        assert_eq!(models.len(), 1);
        assert_eq!(models[0].id, added.id);
    }

    /// **The link production actually crosses.** The duplicate leaves the core
    /// as `CoreError::Repository(AlreadyExists)`, and `add` intercepts it here
    /// before the blanket `CoreError -> HttpError` conversion ever sees it. A
    /// test that exercises only that blanket conversion leaves this arm
    /// unpinned: reverting it to `GuiError::Internal` turns the duplicate back
    /// into a 500 with every error-mapping test still green.
    #[tokio::test]
    async fn adding_a_file_already_in_the_library_is_a_conflict() {
        let core = test_core().await;
        let ops = make_ops(core);

        let dir = tempdir().unwrap();
        let gguf_path = dir.path().join("model.gguf");
        fs::write(&gguf_path, b"placeholder").await.unwrap();
        let file_path = gguf_path.to_str().unwrap().to_string();

        ops.add(AddModelRequest {
            file_path: file_path.clone(),
        })
        .await
        .expect("first add should succeed");

        let result = ops.add(AddModelRequest { file_path }).await;
        assert!(
            matches!(result, Err(GuiError::Conflict(_))),
            "expected Conflict, got {result:?}"
        );
    }

    /// **The duplicate `--force` used to create.** A downloaded model is keyed
    /// `hf:<repo>@<sha>#<file>`, but re-importing its file computes a
    /// `local:<hash>` key. Nothing conflicted, `file_path` carries no unique
    /// index, and the refresh appended a *second* row for one file — the exact
    /// failure this change exists to prevent, reintroduced by the flag added
    /// to serve it.
    ///
    /// This has to run against the real repository. `MockRepo` upserts on
    /// `file_path` while `SqliteModelRepository` upserts on `model_key`, so the
    /// core-level double reports "one row, id kept" for input the product
    /// duplicates — it cannot observe this bug at all.
    #[tokio::test]
    async fn forcing_a_downloaded_model_refreshes_it_rather_than_duplicating_it() {
        use gglib_core::domain::NewModel;
        use gglib_core::ports::NoopGgufParser;
        use gglib_core::services::ImportMode;

        let core = test_core().await;

        let dir = tempdir().unwrap();
        let gguf_path = dir.path().join("Qwen3-8B-Q4_K_M.gguf");
        fs::write(&gguf_path, b"placeholder").await.unwrap();

        // Registered the way a completed download registers it: with HF
        // provenance, and therefore an `hf:` model key.
        let mut downloaded = NewModel::new(
            "Qwen3-8B".to_string(),
            gguf_path.clone(),
            8.0,
            chrono::Utc::now(),
        );
        downloaded.hf_repo_id = Some("Qwen/Qwen3-8B-GGUF".to_string());
        downloaded.hf_commit_sha = Some("abc123".to_string());
        downloaded.hf_filename = Some("Qwen3-8B-Q4_K_M.gguf".to_string());
        let original = core
            .models()
            .add(downloaded)
            .await
            .expect("registering a download should succeed");

        let refreshed = core
            .models()
            .import_from_file(&gguf_path, &NoopGgufParser, None, ImportMode::Refresh)
            .await
            .expect("--force must refresh rather than fail");

        assert_eq!(
            refreshed.id, original.id,
            "the refresh must land on the downloaded row, not create a new one"
        );
        assert_eq!(
            core.models().list().await.unwrap().len(),
            1,
            "one file is one model"
        );
    }

    /// **A refresh must not repoint a sharded model at the wrong file.**
    ///
    /// `find_by_path` matches a sharded model through its sibling paths, so
    /// `--force` on shard 2 finds the row keyed to shard 1. Landing on it and
    /// assigning `file_path = excluded.file_path` would repoint the model at
    /// the shard-2 file — which llama.cpp cannot open a split GGUF from, so
    /// the model would stop launching. Appending a stray row (the behaviour
    /// before the refresh landed on the right row) was survivable; destroying
    /// the good row is not.
    #[tokio::test]
    async fn forcing_from_a_sibling_shard_refuses_rather_than_repointing() {
        use gglib_core::domain::NewModel;
        use gglib_core::ports::NoopGgufParser;
        use gglib_core::services::ImportMode;

        let core = test_core().await;

        let dir = tempdir().unwrap();
        let first = dir.path().join("Qwen3-30B-00001-of-00002.gguf");
        let second = dir.path().join("Qwen3-30B-00002-of-00002.gguf");
        fs::write(&first, b"placeholder").await.unwrap();
        fs::write(&second, b"placeholder").await.unwrap();

        // Seeded with HuggingFace provenance, because that is the only way the
        // product ever produces a sharded row — `file_paths` is set by the
        // download path alone. A local sharded model would give the mutation a
        // `local:` key to collide with and demonstrate a duplicate rather than
        // the repoint this guard exists to prevent.
        let mut sharded = NewModel::new(
            "Qwen3-30B".to_string(),
            first.clone(),
            30.0,
            chrono::Utc::now(),
        );
        sharded.file_paths = Some(vec![first.clone(), second.clone()]);
        sharded.hf_repo_id = Some("Qwen/Qwen3-30B-GGUF".to_string());
        sharded.hf_commit_sha = Some("abc123".to_string());
        sharded.hf_filename = Some("Qwen3-30B-00001-of-00002.gguf".to_string());
        let original = core.models().add(sharded).await.expect("register");

        let err = core
            .models()
            .import_from_file(&second, &NoopGgufParser, None, ImportMode::Refresh)
            .await
            .expect_err("refreshing from shard 2 must be refused");
        assert!(
            matches!(err, gglib_core::ports::CoreError::Validation(_)),
            "got {err:?}"
        );

        let still = core
            .models()
            .get_by_id(original.id)
            .await
            .unwrap()
            .expect("the row must still be there");
        assert_eq!(
            still.file_path,
            std::fs::canonicalize(&first).unwrap(),
            "the model must still point at its first shard"
        );
        assert_eq!(core.models().list().await.unwrap().len(), 1);

        // The other half of the guard: refreshing the model by its *own*
        // first shard is the documented workflow and must be accepted —
        // including when the caller spells that path indirectly. A guard that
        // compared the stored column without resolving it would refuse this
        // and print the same path on both sides of the message.
        std::fs::create_dir(dir.path().join("sub")).unwrap();
        let respelled = dir
            .path()
            .join("sub")
            .join("..")
            .join(first.file_name().expect("the shard has a file name"));
        let refreshed = core
            .models()
            .import_from_file(&respelled, &NoopGgufParser, None, ImportMode::Refresh)
            .await
            .expect("refreshing by the model's own first shard must be accepted");
        assert_eq!(refreshed.id, original.id);
        assert_eq!(core.models().list().await.unwrap().len(), 1);
    }

    #[tokio::test]
    async fn add_nonexistent_file_returns_validation_error() {
        let core = test_core().await;
        let ops = make_ops(core);

        let req = AddModelRequest {
            file_path: "/no/such/file.gguf".to_string(),
        };
        let result = ops.add(req).await;
        assert!(
            matches!(result, Err(GuiError::ValidationFailed(_))),
            "expected ValidationFailed, got {result:?}"
        );
    }

    #[tokio::test]
    async fn remove_unknown_id_returns_not_found() {
        let core = test_core().await;
        let ops = make_ops(core);
        let result = ops.remove(999, RemoveModelRequest::default()).await;
        assert!(
            matches!(
                result,
                Err(GuiError::NotFound {
                    entity: "model",
                    ..
                })
            ),
            "expected NotFound, got {result:?}"
        );
    }

    /// A runtime that reports one fixed model as running, and records
    /// whether `stop_current` was called.
    ///
    /// Stands in for the shared `ProcessManager`-backed runtime `ServerOps`
    /// starts models through. Before this fix, `ModelOps` consulted its own
    /// `ProcessRunner` instead — a registry `ServerOps` never wrote to — so a
    /// model actually running under the proxy looked idle here and the force
    /// guard below never fired.
    #[derive(Debug)]
    struct RunningRuntime {
        model_id: i64,
        port: u16,
        stopped: std::sync::atomic::AtomicBool,
    }

    impl RunningRuntime {
        fn new(model_id: i64, port: u16) -> Self {
            Self {
                model_id,
                port,
                stopped: std::sync::atomic::AtomicBool::new(false),
            }
        }

        fn stopped(&self) -> bool {
            self.stopped.load(std::sync::atomic::Ordering::SeqCst)
        }
    }

    #[async_trait::async_trait]
    impl gglib_core::ports::ModelRuntimePort for RunningRuntime {
        async fn admit(
            &self,
            _model_name: &str,
            _num_ctx: Option<u64>,
            _default_ctx: u64,
            _overrides: gglib_core::ports::LaunchOverrides,
        ) -> Result<gglib_core::ports::Admission, gglib_core::ports::ModelRuntimeError> {
            unimplemented!("not exercised by the remove() tests")
        }

        async fn current_model(&self) -> Option<gglib_core::ports::RunningTarget> {
            None
        }

        async fn list_running(&self) -> Vec<gglib_core::ports::ProcessHandle> {
            vec![gglib_core::ports::ProcessHandle::new(
                self.model_id,
                "running-model".to_string(),
                None,
                self.port,
                0,
            )]
        }

        async fn stop_current(&self) -> Result<(), gglib_core::ports::ModelRuntimeError> {
            self.stopped
                .store(true, std::sync::atomic::Ordering::SeqCst);
            Ok(())
        }
    }

    /// Add a placeholder model on disk and register it, returning the DTO.
    async fn add_placeholder_model(core: Arc<AppCore>, dir: &tempfile::TempDir) -> GuiModel {
        let gguf_path = dir.path().join("model.gguf");
        fs::write(&gguf_path, b"placeholder").await.unwrap();
        let gguf_path = gguf_path.canonicalize().unwrap();

        make_ops(core)
            .add(AddModelRequest {
                file_path: gguf_path.to_str().unwrap().to_string(),
            })
            .await
            .expect("add should succeed")
    }

    /// Regression test for the `ModelOps`/`ServerOps` registry split: `remove`
    /// must consult the same runtime models are actually started through, so
    /// a model the proxy reports running blocks deletion here too.
    #[tokio::test]
    async fn remove_blocks_when_the_shared_runtime_reports_the_model_running() {
        let core = test_core().await;
        let dir = tempdir().unwrap();
        let added = add_placeholder_model(Arc::clone(&core), &dir).await;

        let runtime = Arc::new(RunningRuntime::new(added.id, 5500));
        let ops = ModelOps::new(ModelDeps {
            core,
            runtime: Arc::clone(&runtime) as Arc<dyn ModelRuntimePort>,
            gguf_parser: Arc::new(NoopGgufParser),
        });

        let result = ops.remove(added.id, RemoveModelRequest::default()).await;
        assert!(
            matches!(result, Err(GuiError::Conflict(_))),
            "expected Conflict while the shared runtime reports the model running, got {result:?}"
        );
        assert!(
            !runtime.stopped(),
            "a blocked (non-forced) remove must not stop the server"
        );
    }

    /// `force=true` must stop the server through the same shared runtime
    /// `ServerOps` uses, not a disconnected registry that never saw it start.
    #[tokio::test]
    async fn remove_with_force_stops_the_server_through_the_shared_runtime() {
        let core = test_core().await;
        let dir = tempdir().unwrap();
        let added = add_placeholder_model(Arc::clone(&core), &dir).await;

        let runtime = Arc::new(RunningRuntime::new(added.id, 5500));
        let ops = ModelOps::new(ModelDeps {
            core,
            runtime: Arc::clone(&runtime) as Arc<dyn ModelRuntimePort>,
            gguf_parser: Arc::new(NoopGgufParser),
        });

        let result = ops
            .remove(added.id, RemoveModelRequest { force: true })
            .await;
        assert!(result.is_ok(), "force=true should proceed: {result:?}");
        assert!(
            runtime.stopped(),
            "force=true must stop the server via the shared runtime"
        );
    }

    #[tokio::test]
    async fn list_tags_empty_on_fresh_db() {
        let core = test_core().await;
        let ops = make_ops(core);
        let tags = ops.list_tags().await.expect("list_tags should succeed");
        assert!(tags.is_empty());
    }

    /// End-to-end: drive `ModelOps::update` with `UpdateModelRequest` values
    /// built from real `serde_json::from_str` payloads (not constructed
    /// directly in Rust) to prove the double-`Option` null-clearing fix
    /// works across the actual JSON boundary, not just in isolated
    /// deserialization tests.
    #[tokio::test]
    async fn update_server_defaults_json_round_trip() {
        let core = test_core().await;
        let ops = make_ops(core);

        let dir = tempdir().unwrap();
        let gguf_path = dir.path().join("model.gguf");
        fs::write(&gguf_path, b"placeholder").await.unwrap();
        let gguf_path = gguf_path.canonicalize().unwrap();

        let added = ops
            .add(AddModelRequest {
                file_path: gguf_path.to_str().unwrap().to_string(),
            })
            .await
            .expect("add should succeed");
        assert!(
            added.server_defaults.is_none(),
            "fresh model has no override"
        );

        // 1. Set server_defaults via a populated-object JSON payload.
        let set_req: UpdateModelRequest =
            serde_json::from_str(r#"{"serverDefaults": {"contextLength": 8192}}"#).unwrap();
        let updated = ops
            .update(added.id, set_req)
            .await
            .expect("update should succeed");
        assert_eq!(
            updated
                .server_defaults
                .as_ref()
                .and_then(|c| c.context_length),
            Some(8192),
            "server_defaults.contextLength should be set from JSON"
        );

        // 2. Omitted key is a no-op — other fields change, override survives.
        let noop_req: UpdateModelRequest = serde_json::from_str(r#"{"name": "Renamed"}"#).unwrap();
        let after_noop = ops
            .update(added.id, noop_req)
            .await
            .expect("update should succeed");
        assert_eq!(after_noop.name, "Renamed");
        assert_eq!(
            after_noop
                .server_defaults
                .as_ref()
                .and_then(|c| c.context_length),
            Some(8192),
            "omitted serverDefaults key must not clear the existing override"
        );

        // 3. Explicit JSON null clears the override.
        let clear_req: UpdateModelRequest =
            serde_json::from_str(r#"{"serverDefaults": null}"#).unwrap();
        let cleared = ops
            .update(added.id, clear_req)
            .await
            .expect("update should succeed");
        assert!(
            cleared.server_defaults.is_none(),
            "explicit JSON null must clear server_defaults"
        );
    }

    // ── explain_sampling ──────────────────────────────────────────────────
    //
    // The resolution itself is covered in `sampling_explain`; these cover the
    // wiring — model lookup, settings load, and profile selection.

    /// Import a placeholder model and return its id.
    async fn seed_model(ops: &ModelOps, dir: &tempfile::TempDir) -> i64 {
        let gguf_path = dir.path().join("model.gguf");
        fs::write(&gguf_path, b"placeholder").await.unwrap();
        ops.add(AddModelRequest {
            file_path: gguf_path.to_str().unwrap().to_string(),
        })
        .await
        .expect("add should succeed")
        .id
    }

    fn profile(name: &str, temperature: f32) -> gglib_core::domain::InferenceProfile {
        gglib_core::domain::InferenceProfile {
            name: name.to_owned(),
            description: None,
            config: gglib_core::domain::InferenceConfig {
                temperature: Some(temperature),
                ..Default::default()
            },
            list_in_models: false,
        }
    }

    #[tokio::test]
    async fn explain_sampling_falls_back_to_the_floor_when_nothing_is_stored() {
        let core = test_core().await;
        let ops = make_ops(Arc::clone(&core));
        let dir = tempdir().unwrap();
        let id = seed_model(&ops, &dir).await;

        let dto = ops.explain_sampling(id, None).await.expect("explain");

        assert_eq!(dto.resolved.temperature, Some(0.7));
        assert!(dto.profile.is_none());
        assert!(!dto.is_reasoning);
        assert!(
            dto.sources
                .iter()
                .all(|entry| entry.layer.is_none() && entry.kind != ProvenanceKindDto::Layer),
            "no layer should have supplied anything: {:?}",
            dto.sources
        );
    }

    #[tokio::test]
    async fn explain_sampling_unknown_id_returns_not_found() {
        let core = test_core().await;
        let ops = make_ops(core);
        let result = ops.explain_sampling(999, None).await;
        assert!(
            matches!(
                result,
                Err(GuiError::NotFound {
                    entity: "model",
                    ..
                })
            ),
            "expected NotFound, got {result:?}"
        );
    }

    /// A named profile that does not exist is a caller error, not a reason to
    /// answer a different question.
    #[tokio::test]
    async fn explain_sampling_unknown_profile_names_the_configured_ones() {
        let core = test_core().await;
        let ops = make_ops(Arc::clone(&core));
        let dir = tempdir().unwrap();
        let id = seed_model(&ops, &dir).await;

        core.settings()
            .update(gglib_core::SettingsUpdate {
                inference_profiles: Some(Some(vec![profile("coding", 0.2)])),
                ..Default::default()
            })
            .await
            .unwrap();

        let result = ops.explain_sampling(id, Some("codign")).await;
        let Err(GuiError::ValidationFailed(message)) = result else {
            panic!("expected ValidationFailed, got {result:?}");
        };
        assert!(message.contains("codign"), "{message}");
        assert!(message.contains("coding"), "{message}");
    }

    #[tokio::test]
    async fn explain_sampling_applies_a_named_profile_over_global_settings() {
        let core = test_core().await;
        let ops = make_ops(Arc::clone(&core));
        let dir = tempdir().unwrap();
        let id = seed_model(&ops, &dir).await;

        core.settings()
            .update(gglib_core::SettingsUpdate {
                inference_defaults: Some(Some(gglib_core::domain::InferenceConfig {
                    temperature: Some(0.4),
                    ..Default::default()
                })),
                inference_profiles: Some(Some(vec![profile("coding", 0.2)])),
                ..Default::default()
            })
            .await
            .unwrap();

        let unprofiled = ops.explain_sampling(id, None).await.expect("explain");
        assert_eq!(unprofiled.resolved.temperature, Some(0.4));

        let profiled = ops
            .explain_sampling(id, Some("coding"))
            .await
            .expect("explain");
        assert_eq!(profiled.profile.as_deref(), Some("coding"));
        assert_eq!(profiled.resolved.temperature, Some(0.2));
    }

    #[tokio::test]
    async fn explain_sampling_reads_the_reasoning_tag_from_the_stored_model() {
        let core = test_core().await;
        let ops = make_ops(Arc::clone(&core));
        let dir = tempdir().unwrap();
        let id = seed_model(&ops, &dir).await;

        ops.add_tag(id, "reasoning".to_owned()).await.unwrap();

        let dto = ops.explain_sampling(id, None).await.expect("explain");
        assert!(dto.is_reasoning);
        assert_eq!(dto.resolved.presence_penalty, Some(1.0));
    }
}
