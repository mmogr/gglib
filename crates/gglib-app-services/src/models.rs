//! Model CRUD operations for GUI backend.

use std::path::PathBuf;
use std::sync::Arc;

use gglib_core::events::AppEvent;
use gglib_core::ports::{AppEventEmitter, GgufParserPort, ModelRuntimePort, ProcessHandle};
use gglib_core::services::{AppCore, ImportMode};
use gglib_core::{
    Model, ModelCapabilities, ModelFilterOptions,
    domain::{ModelDetailDto, ModelListQuery, apply_query},
};

use crate::error::GuiError;
use crate::sampling_explain::{self, SamplingExplanationDto};
use crate::types::{
    AddModelRequest, GuiModel, RemoveModelRequest, RetagResponse, SetCapabilitiesRequest,
    UpdateModelRequest,
};

/// Dependencies for model operations.
pub struct ModelDeps {
    pub core: Arc<AppCore>,
    /// The runtime backing server lifecycle — the same one `ServerOps` starts
    /// models through, so serving status here agrees with what `ServerOps`
    /// actually has running rather than a second, independent registry.
    ///
    /// A `gglib model …` command in a terminal is a separate process with no
    /// such runtime. It holds a read-only view of the pid files this one's
    /// servers leave under the data root — see `one_shot_model_ops` in
    /// `gglib-cli`.
    pub runtime: Arc<dyn ModelRuntimePort>,
    pub gguf_parser: Arc<dyn GgufParserPort>,
    /// Broadcasts library changes to every client attached to this daemon.
    ///
    /// Without it a mutation is only visible to the caller that made it: a
    /// GUI refetches its own list after its own edit, so a second window or
    /// browser tab keeps rendering the old row until someone hits refresh.
    ///
    /// The reach is one daemon process. A `gglib model …` command in a
    /// terminal is a *separate* process: it keeps what is emitted here and
    /// posts it to that daemon — see `one_shot_model_ops` in `gglib-cli`.
    pub emitter: Arc<dyn AppEventEmitter>,
}

/// Model operations handler.
pub struct ModelOps {
    pub(crate) deps: ModelDeps,
}

impl ModelOps {
    pub fn new(deps: ModelDeps) -> Self {
        Self { deps }
    }

    /// Broadcast that a stored model changed.
    ///
    /// The paths that mutate through the repository rather than through a
    /// `Model` they hold — tags, retag — have nothing to announce from
    /// afterwards, so this reads the row back. Announcing the stored row
    /// rather than the caller's copy is deliberate: that is what every other
    /// client will fetch.
    ///
    /// A failure here cannot fail the mutation that already succeeded. It
    /// costs one client a stale row until its next refresh, which is strictly
    /// better than reporting a write that did happen as an error.
    async fn announce_updated(&self, id: i64) {
        match crate::helpers::resolve_model(self.deps.core.models(), id).await {
            Ok(model) => self
                .deps
                .emitter
                .emit(AppEvent::model_updated((&model).into())),
            Err(e) => tracing::warn!(
                model_id = id,
                "could not read a changed model back to announce it: {e}"
            ),
        }
    }

    /// Whether `model_id` is among `running`, and the port it is served on.
    fn serving_status(running: &[ProcessHandle], model_id: i64) -> (bool, Option<u16>) {
        running
            .iter()
            .find(|h| h.model_id == model_id)
            .map_or((false, None), |h| (true, Some(h.port)))
    }

    /// Check if a model is currently being served.
    async fn get_server_status(&self, model_id: i64) -> (bool, Option<u16>) {
        Self::serving_status(&self.deps.runtime.list_running().await, model_id)
    }

    /// Refuses to take `model` from under a llama-server that is serving
    /// it: a conflict that names the model and the port, and says to stop
    /// the server.
    ///
    /// The one rule for what drops a model's row or replaces its file:
    /// [`remove`](Self::remove) and [`apply_upgrade`](Self::apply_upgrade)
    /// both ask here. Public for a surface that asks before it prompts, as
    /// `gglib model upgrade` does.
    pub async fn refuse_if_served(&self, model: &Model) -> Result<(), GuiError> {
        match self.get_server_status(model.id).await {
            (_, Some(port)) => Err(GuiError::Conflict(format!(
                "Model '{}' is being served on port {port}. Stop its server first.",
                model.name
            ))),
            _ => Ok(()),
        }
    }

    /// List models filtered and sorted by the given query.
    ///
    /// Fetches all models from the repository, applies [`apply_query`] (the
    /// single source of truth for filter/sort semantics), then enriches each
    /// surviving model with its current serving status. The one listing, for
    /// `gglib model list` and `GET /api/models` alike.
    ///
    /// The runtime is asked what is running once, and every row is read off
    /// that answer: a runtime that has to look, as the CLI's does in the pid
    /// files, looks once for the listing and not once a row.
    pub async fn list_with_query(&self, query: ModelListQuery) -> Result<Vec<GuiModel>, GuiError> {
        let models = self.deps.core.models().list().await?;

        let filtered = apply_query(models, &query);

        let running = self.deps.runtime.list_running().await;
        Ok(filtered
            .into_iter()
            .map(|model| {
                let (is_serving, port) = Self::serving_status(&running, model.id);
                GuiModel::from_model(model, is_serving, port)
            })
            .collect())
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
    /// includes raw GGUF metadata, `MoE` topology, and full `HuggingFace`
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
        let settings = self.deps.core.settings().get().await?;

        let selected = profile
            .map(|name| {
                sampling_explain::find_profile(name, settings.inference_profiles.as_deref())
            })
            .transpose()?;

        Ok(sampling_explain::explain(&model, &settings, selected))
    }

    /// Import the GGUF file `request` names: the one add, for `POST
    /// /api/models` and `gglib model add` alike.
    ///
    /// `param_count_override` and `mode` are what only a terminal asks for.
    /// `gglib model add` prompts for a parameter count to store in place of
    /// the one read from the file, and its `--reimport` is
    /// [`ImportMode::Refresh`], which is explicit about overwriting a row
    /// the caller already has. The HTTP surface has no way to ask for
    /// either: its route passes `None` and [`ImportMode::Fresh`], so a
    /// duplicate is always a 409 there.
    ///
    /// A re-import of a file that already has a row rewrites that row, and
    /// is announced as `model_updated`. Every other import adds a row, and
    /// is announced as `model_added`.
    pub async fn add(
        &self,
        request: AddModelRequest,
        param_count_override: Option<f64>,
        mode: ImportMode,
    ) -> Result<GuiModel, GuiError> {
        let path = PathBuf::from(&request.file_path);
        let models = self.deps.core.models();

        // Asked before the import, which answers with the same row whether
        // it wrote a new one or rewrote this one. A lookup that fails is
        // the import's to report.
        let rewrites = mode == ImportMode::Refresh
            && models
                .find_by_path(&path)
                .await
                .is_ok_and(|row| row.is_some());

        // Delegate to shared core logic for model import with full metadata
        // extraction.
        let model = models
            .import_from_file(
                &path,
                self.deps.gguf_parser.as_ref(),
                param_count_override,
                mode,
            )
            .await?;

        let summary = (&model).into();
        self.deps.emitter.emit(if rewrites {
            AppEvent::model_updated(summary)
        } else {
            AppEvent::model_added(summary)
        });

        // Return with serving status
        let (is_serving, port) = self.get_server_status(model.id).await;
        Ok(GuiModel::from_model(model, is_serving, port))
    }

    /// Update a model in the database: `request` is written onto its row by
    /// [`UpdateModelRequest::apply_to`], which is also what `gglib model
    /// update` previews an edit with.
    pub async fn update(&self, id: i64, request: UpdateModelRequest) -> Result<GuiModel, GuiError> {
        self.link_projector(id, &request).await?;
        self.link_components(id, &request).await?;
        let mut model = crate::helpers::resolve_model(self.deps.core.models(), id).await?;
        request.apply_to(&mut model);
        self.deps.core.models().update(&model).await?;

        // Answer with the row as stored, not as sent. `update` canonicalises
        // `file_path` on write, so echoing the in-memory copy would hand back
        // the caller's spelling and disagree with the very next GET.
        let stored = crate::helpers::resolve_model(self.deps.core.models(), id).await;

        // The write already succeeded, so every other client is stale whether
        // or not the row reads back. Announce the stored row when there is
        // one, and the local copy when the re-read failed — silence here would
        // leave them stale permanently, since nothing else will fire.
        self.deps.emitter.emit(AppEvent::model_updated(
            stored.as_ref().unwrap_or(&model).into(),
        ));

        Ok(GuiModel::from_domain(stored?))
    }

    /// Remove a model from the database.
    ///
    /// A model that is being served is refused
    /// ([`refuse_if_served`](Self::refuse_if_served)), unless `request.force`
    /// is set: then the runtime is told to stop its current model, and the
    /// row is removed.
    pub async fn remove(&self, id: i64, request: RemoveModelRequest) -> Result<String, GuiError> {
        let model = crate::helpers::resolve_model(self.deps.core.models(), id).await?;

        if let Err(served) = self.refuse_if_served(&model).await {
            if !request.force {
                return Err(served);
            }
            self.deps
                .runtime
                .stop_current()
                .await
                .map_err(|e| GuiError::Internal(format!("Failed to stop server: {e}")))?;
        }

        self.deps.core.models().delete(id).await?;

        self.deps.emitter.emit(AppEvent::model_removed(id));

        Ok(format!("Model '{}' removed successfully", model.name))
    }

    /// List all unique tags.
    pub async fn list_tags(&self) -> Result<Vec<String>, GuiError> {
        Ok(self.deps.core.models().list_tags().await?)
    }

    /// Add a tag to a model.
    pub async fn add_tag(&self, model_id: i64, tag: String) -> Result<(), GuiError> {
        self.deps.core.models().add_tag(model_id, tag).await?;

        // Tags are on `GuiModel` and drive the library filters, so a client
        // that missed this shows both the wrong chips and the wrong filter set.
        self.announce_updated(model_id).await;
        Ok(())
    }

    /// Remove a tag from a model.
    pub async fn remove_tag(&self, model_id: i64, tag: String) -> Result<(), GuiError> {
        self.deps.core.models().remove_tag(model_id, &tag).await?;

        self.announce_updated(model_id).await;
        Ok(())
    }

    /// Get all tags for a specific model.
    pub async fn get_tags(&self, model_id: i64) -> Result<Vec<String>, GuiError> {
        Ok(self.deps.core.models().get_tags(model_id).await?)
    }

    /// Get filter options for the model library UI.
    pub async fn get_filter_options(&self) -> Result<ModelFilterOptions, GuiError> {
        Ok(self.deps.core.models().get_filter_options().await?)
    }

    /// Override one or more capability flags on a model.
    ///
    /// Each field in [`SetCapabilitiesRequest`] independently sets or clears
    /// one [`ModelCapabilities`] bit.  `None` fields are left unchanged.
    /// The result is persisted to the database and returned as an updated
    /// [`GuiModel`].
    ///
    /// This is the **single shared implementation** called by the CLI, the
    /// Axum `WebUI`, and the Tauri app.  No business logic lives in the surface
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

        self.deps.core.models().update(&model).await?;

        // The same `models().update()` `Self::update` calls, so the same
        // announcement — capabilities are a field of `GuiModel`, and the
        // inspector renders them.
        self.deps
            .emitter
            .emit(AppEvent::model_updated((&model).into()));

        Ok(GuiModel::from_domain(model))
    }

    /// Re-run capability detection over the model's stored GGUF metadata.
    ///
    /// `full = false` only adds missing tags; `full = true` rebuilds the
    /// system-tag namespace and re-derives the dialect spec. User-curated
    /// tags outside that namespace survive either way. A model with no image
    /// family has one read from its file when it names one. Returns `changed:
    /// false` when the pass was a no-op.
    pub async fn retag(&self, id: i64, full: bool) -> Result<RetagResponse, GuiError> {
        // Resolve first so a stale id surfaces as NotFound, not Internal.
        crate::helpers::resolve_model(self.deps.core.models(), id).await?;
        let diff = self
            .deps
            .core
            .models()
            .retag_model(id, self.deps.gguf_parser.as_ref(), full)
            .await?;

        Ok(match diff {
            Some(diff) => {
                // Only when the pass actually moved something — a no-op retag
                // would otherwise tell every client to refetch for nothing.
                if diff.is_changed() {
                    self.announce_updated(id).await;
                }
                RetagResponse {
                    changed: diff.is_changed(),
                    added: diff.added,
                    removed: diff.removed,
                    spec_changed: diff.spec_changed,
                    family_found: diff.family_found,
                }
            }
            None => RetagResponse {
                changed: false,
                added: Vec::new(),
                removed: Vec::new(),
                spec_changed: false,
                family_found: None,
            },
        })
    }
}

#[cfg(test)]
#[path = "models_tests.rs"]
mod tests;
