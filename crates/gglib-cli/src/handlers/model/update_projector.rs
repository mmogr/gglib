//! The projector half of `gglib model update`'s preview: `--projector` and
//! `--no-projector`.
//!
//! The flags travel as the update request's `projector_path`, the field the
//! inspector's picker sends. What may be linked is decided where that field
//! is applied, by `ModelService::set_projector`.

use std::path::Path;

use gglib_core::Model;

use crate::projector_args::ProjectorChange;

/// The preview's line for a link change, or `None` when the flags leave the
/// model as it is.
pub(super) fn preview(existing: &Model, change: Option<ProjectorChange<'_>>) -> Option<String> {
    let shown =
        |path: Option<&Path>| path.map_or_else(|| "--".to_owned(), |p| p.display().to_string());
    let old = shown(existing.projector_path.as_deref());
    let new = shown(change?.path());
    (old != new).then(|| format!("  {:<15} {old} → {new}", "Projector:"))
}

#[cfg(test)]
#[path = "update_projector_tests.rs"]
mod tests;
