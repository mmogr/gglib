//! Linking a model to the projector it loads, and unlinking it.
//!
//! The rule lives here once. Every surface that sets or clears
//! `models.projector_path` calls [`ModelService::set_projector`].

use std::path::{Path, PathBuf};

use thiserror::Error;

use super::ModelService;
use crate::domain::Model;
use crate::ports::{CoreError, GgufParserPort, RepositoryError};

/// Why a file was not linked as a model's projector.
#[derive(Debug, Error)]
pub enum ProjectorError {
    /// The path does not resolve to a file.
    #[error("no projector file at {}: {reason}", .path.display())]
    Missing {
        /// The path as the caller gave it.
        path: PathBuf,
        /// What resolving it reported.
        reason: String,
    },

    /// The file's header could not be read as GGUF.
    #[error("{} is not a readable GGUF file: {reason}", .path.display())]
    Unreadable {
        /// The resolved path.
        path: PathBuf,
        /// What the parser reported.
        reason: String,
    },

    /// The file is a GGUF, and its header says it holds a model's weights.
    #[error(
        "{} holds a model's weights, not a projector (a projector's header says general.type = mmproj)",
        .0.display()
    )]
    Weights(PathBuf),

    /// The model could not be read or written.
    #[error(transparent)]
    Repository(#[from] RepositoryError),
}

impl From<ProjectorError> for CoreError {
    fn from(error: ProjectorError) -> Self {
        match error {
            ProjectorError::Repository(repository) => Self::Repository(repository),
            refused => Self::Validation(refused.to_string()),
        }
    }
}

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
    /// [`ProjectorError::Missing`] when the path resolves to no file,
    /// [`ProjectorError::Unreadable`] when it is not a GGUF,
    /// [`ProjectorError::Weights`] when its header says it is a model, and
    /// [`ProjectorError::Repository`] when model `id` cannot be read or
    /// written.
    pub async fn set_projector(
        &self,
        id: i64,
        projector: Option<&Path>,
        gguf_parser: &dyn GgufParserPort,
    ) -> Result<Model, ProjectorError> {
        let mut model = self.repo.get_by_id(id).await?;
        model.projector_path = projector
            .map(|path| checked_projector(path, gguf_parser))
            .transpose()?;
        self.repo.update(&model).await?;
        Ok(model)
    }
}

/// The canonical path of `path`, once its header has said it is a projector.
fn checked_projector(
    path: &Path,
    gguf_parser: &dyn GgufParserPort,
) -> Result<PathBuf, ProjectorError> {
    let resolved =
        crate::paths::canonical_model_path(path).map_err(|e| ProjectorError::Missing {
            path: path.to_path_buf(),
            reason: e.to_string(),
        })?;
    let header = gguf_parser
        .parse(&resolved)
        .map_err(|e| ProjectorError::Unreadable {
            path: resolved.clone(),
            reason: e.to_string(),
        })?;
    if header.role.is_projector() {
        Ok(resolved)
    } else {
        Err(ProjectorError::Weights(resolved))
    }
}

#[cfg(test)]
#[path = "model_projector_tests.rs"]
mod tests;
