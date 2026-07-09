//! Shared helper functions for model resolution and process handle lookup.

use gglib_core::domain::Model;
use gglib_core::ports::{ProcessHandle, ProcessRunner};
use gglib_core::services::ModelService;

use crate::error::GuiError;

/// Resolve model by ID, returning GUI error if not found.
pub(crate) async fn resolve_model(models: &ModelService, id: i64) -> Result<Model, GuiError> {
    models
        .get_by_id(id)
        .await
        .map_err(|e| GuiError::Internal(format!("Failed to query model: {e}")))?
        .ok_or_else(|| GuiError::NotFound {
            entity: "model",
            id: id.to_string(),
        })
}

/// Find a running process handle for a model.
pub(crate) async fn find_handle(
    runner: &dyn ProcessRunner,
    model_id: i64,
) -> Option<ProcessHandle> {
    match runner.list_running().await {
        Ok(handles) => handles.into_iter().find(|h| h.model_id == model_id),
        Err(_) => None,
    }
}
