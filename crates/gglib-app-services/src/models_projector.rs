//! A model's projector link, as the inspector sets it and picks it.
//!
//! The rule for what may be linked is `ModelService::set_projector`'s, the
//! one `gglib model update --projector` calls too.

use std::path::Path;

use gglib_core::ports::RepositoryError;
use gglib_core::services::{ProjectorError, projector_choices};

use crate::error::GuiError;
use crate::helpers::resolve_model;
use crate::models::ModelOps;
use crate::types::{ProjectorChoice, UpdateModelRequest};

impl ModelOps {
    /// Apply `request.projector_path`: link model `id` to the file, unlink it
    /// for `Some(None)`, and leave the link alone for `None`.
    ///
    /// A file that is not a projector is refused before anything is written.
    pub(crate) async fn link_projector(
        &self,
        id: i64,
        request: &UpdateModelRequest,
    ) -> Result<(), GuiError> {
        let Some(change) = &request.projector_path else {
            return Ok(());
        };
        self.deps
            .core
            .models()
            .set_projector(
                id,
                change.as_deref().map(Path::new),
                self.deps.gguf_parser.as_ref(),
            )
            .await
            .map(drop)
            .map_err(|error| refusal(id, error))
    }

    /// The projectors the picker offers for model `id`: those already in use
    /// by any model, and the model's own files that are named as projectors.
    pub async fn projector_choices(&self, id: i64) -> Result<Vec<ProjectorChoice>, GuiError> {
        let core = &self.deps.core;
        let model = resolve_model(core.models(), id).await?;
        let library = core.models().list().await?;
        let files = match core.verification() {
            Some(verification) => verification.files_of(id).await?,
            None => Vec::new(),
        };
        Ok(projector_choices(&model, &files, &library)
            .into_iter()
            .map(|path| ProjectorChoice {
                name: path
                    .file_name()
                    .unwrap_or(path.as_os_str())
                    .to_string_lossy()
                    .into_owned(),
                path: path.to_string_lossy().into_owned(),
            })
            .collect())
    }
}

/// A file that may not be linked is the caller's mistake, named as the rule
/// names it; an unknown model is not found; any other failure of the store
/// is what `From<RepositoryError>` makes of it.
fn refusal(id: i64, error: ProjectorError) -> GuiError {
    match error {
        ProjectorError::Repository(RepositoryError::NotFound(_)) => GuiError::NotFound {
            entity: "model",
            id: id.to_string(),
        },
        ProjectorError::Repository(other) => other.into(),
        refused => GuiError::ValidationFailed(refused.to_string()),
    }
}

#[cfg(test)]
#[path = "models_projector_tests.rs"]
mod tests;
