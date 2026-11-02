//! The projector files a surface offers when a model is being linked to one.
//!
//! [`projector_choices`] holds the rule once: the files other models already
//! load, and the model's own files whose headers say they are projectors.

use std::collections::BTreeSet;
use std::path::PathBuf;

use super::ModelVerificationService;
use super::model_row_roles::header_role;
use crate::domain::{Model, ModelFile, ModelFolder};
use crate::download::GgufFileRole;
use crate::paths::canonical_model_path;
use crate::ports::{GgufParserPort, RepositoryError};

impl ModelVerificationService {
    /// The files the library records for model `model_id`, in file order.
    ///
    /// # Errors
    ///
    /// [`RepositoryError::Storage`] when the rows cannot be read.
    pub async fn files_of(&self, model_id: i64) -> Result<Vec<ModelFile>, RepositoryError> {
        self.model_files_repo.get_by_model_id(model_id).await
    }

    /// The files a verification reads, or the message it stops with: what a
    /// store that failed said, alone, and an error of any other kind under
    /// its label.
    pub(super) async fn files_to_verify(&self, model_id: i64) -> Result<Vec<ModelFile>, String> {
        self.files_of(model_id).await.map_err(|e| {
            let said = match e {
                RepositoryError::Storage(said) => said,
                labelled => labelled.to_string(),
            };
            format!("Failed to get model files: {said}")
        })
    }
}

/// The projectors to offer for `model`, each path once, in path order.
///
/// Two sources. Every projector a model in `library` is linked to is offered,
/// because one projector serves every build of its base model. And each of
/// `files`, the model's own rows, whose header `gguf_parser` reads as a
/// projector's is offered under its canonical path, the form a link is
/// stored in; a row's name is read in the model's [`ModelFolder`]. A file
/// whose header cannot be read, one not on disk among them, is not offered,
/// since linking it would be refused. The weights themselves are never read.
#[must_use]
pub fn projector_choices(
    model: &Model,
    files: &[ModelFile],
    library: &[Model],
    gguf_parser: &dyn GgufParserPort,
) -> Vec<PathBuf> {
    let folder = ModelFolder::of(&model.file_path, files);
    let own = files
        .iter()
        .map(|file| folder.file(&file.file_path))
        .filter(|path| *path != model.file_path)
        .filter(|path| header_role(path, gguf_parser).is_some_and(GgufFileRole::is_projector))
        .filter_map(|path| canonical_model_path(&path).ok());
    let in_use = library
        .iter()
        .filter_map(|other| other.projector_path.clone());
    in_use
        .chain(own)
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect()
}

#[cfg(test)]
#[path = "projector_choices_tests.rs"]
mod tests;
