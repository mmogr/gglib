//! Model CRUD operations for GUI backend.

use std::path::PathBuf;

use gglib_core::ModelFilterOptions;
use gglib_core::domain::Model;

use crate::deps::GuiDeps;
use crate::error::GuiError;
use crate::types::{AddModelRequest, GuiModel, RemoveModelRequest, UpdateModelRequest};

/// Model operations handler.
pub struct ModelOps<'a> {
    deps: &'a GuiDeps,
}

impl<'a> ModelOps<'a> {
    pub fn new(deps: &'a GuiDeps) -> Self {
        Self { deps }
    }

    /// Resolve model by ID, returning GUI error if not found.
    async fn resolve_model(&self, id: i64) -> Result<Model, GuiError> {
        self.deps
            .models()
            .get_by_id(id)
            .await
            .map_err(|e| GuiError::Internal(format!("Failed to query model: {e}")))?
            .ok_or_else(|| GuiError::NotFound {
                entity: "model",
                id: id.to_string(),
            })
    }

    /// Check if a model is currently being served.
    async fn get_server_status(&self, model_id: i64) -> (bool, Option<u16>) {
        match self.deps.runner.list_running().await {
            Ok(handles) => {
                for handle in handles {
                    if handle.model_id == model_id {
                        return (true, Some(handle.port));
                    }
                }
                (false, None)
            }
            Err(_) => (false, None),
        }
    }

    /// Find a running process handle for a model.
    async fn find_handle(&self, model_id: i64) -> Option<gglib_core::ports::ProcessHandle> {
        match self.deps.runner.list_running().await {
            Ok(handles) => handles.into_iter().find(|h| h.model_id == model_id),
            Err(_) => None,
        }
    }

    /// List all models with their serving status.
    pub async fn list(&self) -> Result<Vec<GuiModel>, GuiError> {
        let models = self
            .deps
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

    /// Get a specific model by ID.
    pub async fn get(&self, id: i64) -> Result<GuiModel, GuiError> {
        let model = self.resolve_model(id).await?;
        let (is_serving, port) = self.get_server_status(id).await;
        Ok(GuiModel::from_model(model, is_serving, port))
    }

    /// Add a model to the database from a file path.
    pub async fn add(&self, request: AddModelRequest) -> Result<GuiModel, GuiError> {
        let path = PathBuf::from(&request.file_path);

        // Delegate to shared core logic for model import with full metadata extraction
        let model = self
            .deps
            .models()
            .import_from_file(&path, self.deps.gguf_parser().as_ref(), None)
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
        let mut model = self.resolve_model(id).await?;

        if let Some(name) = request.name {
            model.name = name;
        }
        if let Some(quantization) = request.quantization {
            model.quantization = Some(quantization);
        }
        if let Some(file_path) = request.file_path {
            model.file_path = PathBuf::from(file_path);
        }

        self.deps
            .models()
            .update(&model)
            .await
            .map_err(|e| GuiError::Internal(format!("Failed to update model: {e}")))?;

        Ok(GuiModel::from_domain(model))
    }

    /// Remove a model from the database.
    pub async fn remove(&self, id: i64, request: RemoveModelRequest) -> Result<String, GuiError> {
        let model = self.resolve_model(id).await?;

        if let Some(handle) = self.find_handle(id).await {
            if !request.force {
                return Err(GuiError::Conflict(format!(
                    "Model is currently serving on port {}. Stop the server first or use force=true",
                    handle.port
                )));
            }
            self.deps
                .runner
                .stop(&handle)
                .await
                .map_err(|e| GuiError::Internal(format!("Failed to stop server: {e}")))?;
        }

        self.deps
            .models()
            .delete(id)
            .await
            .map_err(|e| GuiError::Internal(format!("Failed to delete model: {e}")))?;

        Ok(format!("Model '{}' removed successfully", model.name))
    }

    /// List all unique tags.
    pub async fn list_tags(&self) -> Result<Vec<String>, GuiError> {
        self.deps
            .models()
            .list_tags()
            .await
            .map_err(|e| GuiError::Internal(format!("Failed to list tags: {e}")))
    }

    /// Add a tag to a model.
    pub async fn add_tag(&self, model_id: i64, tag: String) -> Result<(), GuiError> {
        self.deps
            .models()
            .add_tag(model_id, tag)
            .await
            .map_err(|e| GuiError::Internal(format!("Failed to add tag: {e}")))
    }

    /// Remove a tag from a model.
    pub async fn remove_tag(&self, model_id: i64, tag: String) -> Result<(), GuiError> {
        self.deps
            .models()
            .remove_tag(model_id, &tag)
            .await
            .map_err(|e| GuiError::Internal(format!("Failed to remove tag: {e}")))
    }

    /// Get all tags for a specific model.
    pub async fn get_tags(&self, model_id: i64) -> Result<Vec<String>, GuiError> {
        self.deps
            .models()
            .get_tags(model_id)
            .await
            .map_err(|e| GuiError::Internal(format!("Failed to get tags: {e}")))
    }

    /// Get all model IDs that have a specific tag.
    pub async fn get_by_tag(&self, tag: String) -> Result<Vec<i64>, GuiError> {
        let models = self
            .deps
            .models()
            .get_by_tag(&tag)
            .await
            .map_err(|e| GuiError::Internal(format!("Failed to get models by tag: {e}")))?;
        Ok(models.into_iter().map(|m| m.id).collect())
    }

    /// Get filter options for the model library UI.
    pub async fn get_filter_options(&self) -> Result<ModelFilterOptions, GuiError> {
        self.deps
            .models()
            .get_filter_options()
            .await
            .map_err(|e| GuiError::Internal(format!("Failed to get filter options: {e}")))
    }
}
