//! What each of a model's own files holds, read from its header.
//!
//! A model's `model_files` rows name its weights and, when it was downloaded
//! with one, its projector. What a file is comes from its header, never its
//! name: an import keeps whatever name a file was given, and a renamed file
//! is still what it was.

use std::path::{Path, PathBuf};
use std::sync::Arc;

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
/// order; `None` for a row whose role cannot be told.
///
/// The row that is the model's own file is its weights, and its header is
/// not read: a model is registered from its weights. Every other row is
/// what its header says. A row whose header cannot be read, which is what
/// verification and a repair look for, is the projector when `model` links
/// that very file as its projector: a link is made only to a file that was
/// one. Any other such row is unknown, and its name is not read.
///
/// Every path is resolved and every header read on the blocking pool, so a
/// slow disk or a dead mount holds a blocking thread and never the runtime.
/// A read that panics is logged and leaves every row unknown.
pub(super) async fn row_roles(
    model: &Model,
    folder: &ModelFolder,
    files: &[ModelFile],
    gguf_parser: &Arc<dyn GgufParserPort>,
) -> Vec<Option<GgufFileRole>> {
    let weights = model.file_path.clone();
    let projector = model.projector_path.clone();
    let paths: Vec<PathBuf> = files
        .iter()
        .map(|row| folder.file(&row.file_path))
        .collect();
    let count = paths.len();
    let parser = Arc::clone(gguf_parser);
    let read = tokio::task::spawn_blocking(move || {
        let weights = resolved_or_literal(&weights);
        let projector = projector.as_deref().map(resolved_or_literal);
        paths
            .iter()
            .map(|path| {
                let resolved = resolved_or_literal(path);
                if resolved == weights {
                    return Some(GgufFileRole::Weights);
                }
                let linked = projector.as_ref() == Some(&resolved);
                header_role(path, parser.as_ref())
                    .or_else(|| linked.then_some(GgufFileRole::Projector))
            })
            .collect::<Vec<_>>()
    })
    .await;
    read.unwrap_or_else(|panicked| {
        tracing::warn!(
            model_id = model.id,
            error = %panicked,
            "a model's files could not be read for their roles"
        );
        vec![None; count]
    })
}

#[cfg(test)]
#[path = "model_row_roles_tests.rs"]
mod tests;
