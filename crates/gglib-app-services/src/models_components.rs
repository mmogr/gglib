//! An image model's component links, as the inspector sets them and picks
//! them.
//!
//! The rule for what may be linked is `ModelService::set_component`'s, the
//! one `gglib model update --component` calls too.

use std::path::Path;

use gglib_core::services::component_choices;

use crate::error::GuiError;
use crate::helpers::resolve_model;
use crate::models::ModelOps;
use crate::models_projector::{file_choice, refusal};
use crate::types::{ComponentChoices, UpdateModelRequest};

impl ModelOps {
    /// Apply `request.components`: link each role it names with a path to
    /// that file, unlink each it names with `null`, and leave every other
    /// role alone. Nothing happens for `None`.
    ///
    /// The roles are linked one at a time, in role order, each as its own
    /// write. A refused file stops the request at its role: the roles before
    /// it stay linked as asked, and nothing after it is written.
    pub(crate) async fn link_components(
        &self,
        id: i64,
        request: &UpdateModelRequest,
    ) -> Result<(), GuiError> {
        let Some(changes) = &request.components else {
            return Ok(());
        };
        for (role, change) in changes {
            self.deps
                .core
                .models()
                .set_component(
                    id,
                    *role,
                    change.as_deref().map(Path::new),
                    self.deps.gguf_parser.as_ref(),
                )
                .await
                .map_err(|error| refusal(id, error))?;
        }
        Ok(())
    }

    /// The files the picker offers for each role model `id`'s family needs:
    /// the files models of that family link in that role, this model's own
    /// link included. Empty for a model that chats.
    pub async fn component_choices(&self, id: i64) -> Result<Vec<ComponentChoices>, GuiError> {
        let core = &self.deps.core;
        let model = resolve_model(core.models(), id).await?;
        let library = core.models().list().await?;
        Ok(component_choices(&model, &library)
            .into_iter()
            .map(|(role, paths)| ComponentChoices {
                role,
                files: paths.iter().map(|path| file_choice(path)).collect(),
            })
            .collect())
    }
}

#[cfg(test)]
#[path = "models_components_tests.rs"]
mod tests;
