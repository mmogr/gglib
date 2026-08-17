//! Model CRUD operations for GUI backend.

use std::path::PathBuf;
use std::sync::Arc;

use gglib_core::events::AppEvent;
use gglib_core::ports::{AppEventEmitter, GgufParserPort, ModelRuntimePort};
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
    /// Broadcasts library membership changes to every connected client.
    ///
    /// Without it a mutation is only visible to the caller that made it: the
    /// GUI refetches its own list after its own edit, and a model added from
    /// the CLI or a second window never appears until someone hits refresh.
    pub emitter: Arc<dyn AppEventEmitter>,
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

        self.deps
            .emitter
            .emit(AppEvent::model_added((&model).into()));

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

        self.deps
            .emitter
            .emit(AppEvent::model_updated((&stored).into()));

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

        self.deps.emitter.emit(AppEvent::model_removed(id));

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
#[path = "models_tests.rs"]
mod tests;
