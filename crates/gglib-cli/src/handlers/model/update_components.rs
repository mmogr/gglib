//! The components half of `gglib model update`'s preview: `--component` and
//! `--no-component`.
//!
//! The flags travel as the update request's `components`, the field the
//! inspector's pickers send. What may be linked is decided where that field
//! is applied, by `ModelService::set_component`.

use std::path::Path;

use gglib_core::Model;

use crate::component_args::ComponentChanges;

/// The preview's lines for the component changes, a heading and one line
/// per role whose link changes; none when the flags leave every link as it
/// is.
pub(super) fn preview(existing: &Model, changes: &ComponentChanges<'_>) -> Vec<String> {
    let shown =
        |path: Option<&Path>| path.map_or_else(|| "--".to_owned(), |p| p.display().to_string());
    let lines: Vec<String> = changes
        .iter()
        .filter_map(|(role, change)| {
            let linked = existing.components.iter().find(|c| c.role == *role);
            let old = shown(linked.map(|c| c.path.as_path()));
            let new = shown(*change);
            (old != new).then(|| format!("    {role}: {old} → {new}"))
        })
        .collect();
    if lines.is_empty() {
        return lines;
    }
    std::iter::once("  Components:".to_owned())
        .chain(lines)
        .collect()
}

#[cfg(test)]
#[path = "update_components_tests.rs"]
mod tests;
