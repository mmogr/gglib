//! The files a surface offers when an image model's component is being
//! linked.
//!
//! [`component_choices`] holds the rule once: for each role the model's
//! family needs, the files models of that family link in that role, this
//! model's own link included.

use std::collections::BTreeSet;
use std::path::PathBuf;

use crate::domain::{ComponentRole, Model};

/// The files to offer for each role `model`'s family needs, in the recipe's
/// order, each path once and in path order within its role; empty for a model
/// that chats.
///
/// A file is offered for a role when a model in `library` of the same family
/// links it in that role, `model` itself included, so the current link is
/// among the choices. A model of another family is not read: its VAE is not
/// this family's, and linking it would be refused.
#[must_use]
pub fn component_choices(model: &Model, library: &[Model]) -> Vec<(ComponentRole, Vec<PathBuf>)> {
    let Some(family) = model.image_family else {
        return Vec::new();
    };
    let same_family = || {
        library
            .iter()
            .filter(move |other| other.image_family == Some(family))
    };
    family
        .recipe()
        .components
        .iter()
        .map(|spec| {
            let linked = same_family()
                .flat_map(|other| other.components.iter())
                .filter(|component| component.role == spec.role)
                .map(|component| component.path.clone())
                .collect::<BTreeSet<_>>();
            (spec.role, linked.into_iter().collect())
        })
        .collect()
}

#[cfg(test)]
#[path = "component_choices_tests.rs"]
mod tests;
