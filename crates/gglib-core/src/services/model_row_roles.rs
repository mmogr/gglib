//! What each of a model's own files holds, read from its header.
//!
//! A model's `model_files` rows name its weights and, when it was downloaded
//! with one, its projector. What a file is comes from its header, never its
//! name: an import keeps whatever name a file was given, and a renamed file
//! is still what it was.

use std::path::Path;

use super::model_links::resolved_or_literal;
use crate::domain::{Model, ModelFile, ModelFolder};
use crate::download::GgufFileRole;
use crate::ports::GgufParserPort;

/// The role `path`'s header states, or `None` when no header can be read
/// there: the file is missing, or it is no GGUF.
pub(super) fn header_role(path: &Path, gguf_parser: &dyn GgufParserPort) -> Option<GgufFileRole> {
    gguf_parser.parse(path).ok().map(|header| header.role)
}

/// The role of each of `files`, `model`'s rows read in `folder`, in row
/// order.
///
/// A row is what its header says. A row whose header cannot be read, which
/// is what verification and a repair look for, is the projector when `model`
/// links that very file as its projector: a link is made only to a file that
/// was one. Any other such row counts as weights, and its name is not read.
pub(super) fn row_roles(
    model: &Model,
    folder: &ModelFolder,
    files: &[ModelFile],
    gguf_parser: &dyn GgufParserPort,
) -> Vec<GgufFileRole> {
    files
        .iter()
        .map(|row| {
            let path = folder.file(&row.file_path);
            header_role(&path, gguf_parser).unwrap_or_else(|| {
                let linked = model.projector_path.as_deref().is_some_and(|projector| {
                    resolved_or_literal(projector) == resolved_or_literal(&path)
                });
                if linked {
                    GgufFileRole::Projector
                } else {
                    GgufFileRole::Weights
                }
            })
        })
        .collect()
}

#[cfg(test)]
#[path = "model_row_roles_tests.rs"]
mod tests;
