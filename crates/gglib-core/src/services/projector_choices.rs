//! The projector files a surface offers when a model is being linked to one.
//!
//! [`projector_choices`] holds the rule once: the files other models already
//! load, and the model's own files that are named as projectors.

use std::collections::BTreeSet;
use std::path::PathBuf;

use super::ModelVerificationService;
use crate::domain::{Model, ModelFile};
use crate::download::GgufFileRole;
use crate::paths::canonical_model_path;
use crate::ports::RepositoryError;

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
/// `files`, the model's own rows, that is named as a projector and is on disk
/// is offered under its canonical path, the form a link is stored in; a row's
/// name is relative to the directory the model's weights are in.
#[must_use]
pub fn projector_choices(model: &Model, files: &[ModelFile], library: &[Model]) -> Vec<PathBuf> {
    let names = files.iter().map(|file| &file.file_path);
    let own = GgufFileRole::projectors_among(&model.file_path, names)
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
