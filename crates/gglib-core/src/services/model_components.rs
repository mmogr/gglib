//! An image model's family and the components it draws with, as the library
//! keeps them.
//!
//! A family is read from the main file's tensor names when the model is
//! imported. A model imported before gglib read them has none, and a retag
//! fills it from the file here, without ever changing one already set.
//!
//! A component is linked by hand through [`ModelService::set_component`], and
//! a download that brings a family's components is linked through the same
//! check, `model_links::checked_link`.

use std::path::{Path, PathBuf};

use super::ModelService;
use super::model_links::{LinkError, LinkRole, checked_link, in_recipe, resolved_or_literal};
use crate::domain::{ComponentRole, ImageFamily, Model, ModelComponent, NewModel};
use crate::ports::{GgufParserPort, ModelRepository};

impl ModelService {
    /// Link model `id`'s `role` to the file at `path`, or unlink the role
    /// with `None`. Answers the model as stored afterwards.
    ///
    /// The model must draw images and its family's recipe must name the
    /// role, whether linking or unlinking. A file is accepted when its tensor
    /// names are the role's for the family, and stored under its canonical
    /// path, so two spellings of one file are one link. Linking replaces the
    /// role's link, if it had one.
    ///
    /// # Errors
    ///
    /// [`LinkError::NotAnImageModel`] and [`LinkError::NotInRecipe`] before
    /// anything is read; the refusals of `checked_link` for the file; and
    /// [`LinkError::Repository`] when model `id` cannot be read or written.
    pub async fn set_component(
        &self,
        id: i64,
        role: ComponentRole,
        path: Option<&Path>,
        gguf_parser: &dyn GgufParserPort,
    ) -> Result<Model, LinkError> {
        let mut model = self.repo.get_by_id(id).await?;
        let Some(family) = model.image_family else {
            return Err(LinkError::NotAnImageModel(model.name));
        };
        if !in_recipe(family, role) {
            return Err(LinkError::NotInRecipe { family, role });
        }
        let linked = path
            .map(|path| checked_link(path, LinkRole::Component { family, role }, gguf_parser))
            .transpose()?;
        model.components.retain(|component| component.role != role);
        if let Some(path) = linked {
            model.components.push(ModelComponent { role, path });
            model.components.sort_by_key(|component| component.role);
        }
        self.repo.update(&model).await?;
        Ok(model)
    }
}

/// Links `model`, a download about to be registered, to the `components`
/// that came with it. Answers a sentence for each one not linked.
///
/// The twin of `link_downloaded_projector`: each file passes the check a
/// hand-made link passes, and a model the library already holds keeps every
/// link it has, the answer naming the kept link when it is to another file. A
/// refused file is named with the reason; the model is registered either way.
#[cfg_attr(
    not(test),
    allow(
        dead_code,
        reason = "the download registrar calls it once downloads bring components"
    )
)]
pub(super) async fn link_downloaded_components(
    repo: &dyn ModelRepository,
    model: &mut NewModel,
    components: &[(ComponentRole, PathBuf)],
    gguf_parser: &dyn GgufParserPort,
) -> Vec<String> {
    if components.is_empty() {
        return Vec::new();
    }
    let Some(family) = model.image_family else {
        return vec![LinkError::NotAnImageModel(model.name.clone()).to_string()];
    };
    // The library stores a model under its resolved path and is asked by it.
    let stored_at = resolved_or_literal(&model.file_path);
    let held = repo
        .find_by_path(&stored_at)
        .await
        .ok()
        .flatten()
        .map(|held| held.components)
        .unwrap_or_default();
    let mut refusals = Vec::new();
    for (role, path) in components {
        if let Some(kept) = held.iter().find(|link| link.role == *role) {
            if kept.path != resolved_or_literal(path) {
                refusals.push(format!(
                    "the model keeps its {role} link to {}",
                    kept.path.display()
                ));
            }
            continue;
        }
        match checked_link(
            path,
            LinkRole::Component {
                family,
                role: *role,
            },
            gguf_parser,
        ) {
            Ok(checked) => model.components.push(ModelComponent {
                role: *role,
                path: checked,
            }),
            Err(refused) => refusals.push(refused.to_string()),
        }
    }
    model.components.sort_by_key(|component| component.role);
    refusals
}

/// Fill `model`'s family from its file's tensor table when it has none, and
/// answer the family found.
///
/// Best effort, as the rest of a retag is: a family already set is never
/// changed, and a file that is missing or unreadable, or whose table names no
/// family, leaves the model as it was without an error.
pub(super) fn sniff_missing_family(
    model: &mut Model,
    parser: &dyn GgufParserPort,
) -> Option<ImageFamily> {
    if model.image_family.is_some() {
        return None;
    }
    let table = parser.tensor_table(&model.file_path).ok()?;
    let family = ImageFamily::sniff(&table)?;
    model.image_family = Some(family);
    Some(family)
}

#[cfg(test)]
#[path = "model_components_tests.rs"]
mod tests;
