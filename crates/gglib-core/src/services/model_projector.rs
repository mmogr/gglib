//! Linking a model to the projector it loads, and unlinking it.
//!
//! Every surface that sets or clears `models.projector_path` calls
//! [`ModelService::set_projector`], and a download that brings a projector is
//! registered through the same check: `model_links::checked_link`, the one
//! rule a projector and an image model's components are linked by.

use std::path::{Path, PathBuf};

use super::ModelService;
use super::model_links::{LinkError, LinkRole, checked_link, resolved_or_literal};
use crate::domain::{Model, NewModel};
use crate::ports::{GgufParserPort, ModelRepository};

impl ModelService {
    /// Link model `id` to the projector at `projector`, or unlink it with
    /// `None`. Answers the model as stored afterwards.
    ///
    /// The file is accepted when its GGUF header says it is a projector, and
    /// stored under its canonical path, the form `models.file_path` takes, so
    /// two spellings of one file are one link. Nothing is read for `None`.
    ///
    /// # Errors
    ///
    /// [`LinkError::Missing`] when the path resolves to no file,
    /// [`LinkError::Unreadable`] when it is not a GGUF,
    /// [`LinkError::Weights`] when its header says it is a model, and
    /// [`LinkError::Repository`] when model `id` cannot be read or
    /// written.
    pub async fn set_projector(
        &self,
        id: i64,
        projector: Option<&Path>,
        gguf_parser: &dyn GgufParserPort,
    ) -> Result<Model, LinkError> {
        let mut model = self.repo.get_by_id(id).await?;
        model.projector_path = projector
            .map(|path| checked_link(path, LinkRole::Projector, gguf_parser))
            .transpose()?;
        self.repo.update(&model).await?;
        Ok(model)
    }
}

/// Links `model`, a download about to be registered, to the `projector` that
/// came with its weights. Answers why it was not linked, when it was not.
///
/// The file passes the check a hand-made link passes. A model the library
/// already holds with a link keeps that link: a repair or an update of its
/// weights does not replace the projector its owner chose, and the answer
/// names the link kept when it is to another file.
pub(super) async fn link_downloaded_projector(
    repo: &dyn ModelRepository,
    model: &mut NewModel,
    projector: Option<&Path>,
    gguf_parser: &dyn GgufParserPort,
) -> Option<String> {
    let projector = projector?;
    // The library stores a model under its resolved path and is asked by it.
    let stored_at = resolved_or_literal(&model.file_path);
    let held = repo.find_by_path(&stored_at).await.ok().flatten();
    let held = held.and_then(|held| held.projector_path);
    link_projector(
        held.as_deref(),
        &mut model.projector_path,
        projector,
        gguf_parser,
    )
}

/// The rule [`link_downloaded_projector`] links by, for a model whose link
/// the library holds as `held`: sets `link` to the downloaded `projector`
/// when there is no held link and the file passes the check, and answers why
/// it did not otherwise. A held link is kept, and named when it is to
/// another file.
pub(super) fn link_projector(
    held: Option<&Path>,
    link: &mut Option<PathBuf>,
    projector: &Path,
    gguf_parser: &dyn GgufParserPort,
) -> Option<String> {
    if let Some(kept) = held {
        return (kept != resolved_or_literal(projector))
            .then(|| format!("the model keeps its link to {}", kept.display()));
    }
    match checked_link(projector, LinkRole::Projector, gguf_parser) {
        Ok(checked) => {
            *link = Some(checked);
            None
        }
        Err(refused) => Some(refused.to_string()),
    }
}

#[cfg(test)]
#[path = "model_projector_tests.rs"]
pub(super) mod tests;
