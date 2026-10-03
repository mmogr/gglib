//! The projector half of `gglib model update`: `--projector` and
//! `--no-projector`.
//!
//! What may be linked is decided by `ModelService::set_projector`, the
//! function the inspector's picker reaches too.

use std::path::Path;

use anyhow::Result;
use gglib_core::Model;

use crate::bootstrap::CliContext;
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

/// Link or unlink `updated`'s model as `change` asks, and carry the stored
/// link onto `updated` so the write that follows keeps it.
///
/// A file that is not a projector is refused here, by name, before the rest
/// of the update is written.
pub(super) async fn apply(
    ctx: &CliContext,
    updated: &mut Model,
    change: Option<ProjectorChange<'_>>,
) -> Result<()> {
    if let Some(change) = change {
        let linked = ctx
            .app
            .models()
            .set_projector(updated.id, change.path(), ctx.gguf_parser.as_ref())
            .await?;
        updated.projector_path = linked.projector_path;
    }
    Ok(())
}

#[cfg(test)]
#[path = "update_projector_tests.rs"]
mod tests;
