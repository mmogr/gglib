//! Shared helper functions for model resolution.

use gglib_core::domain::Model;
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
