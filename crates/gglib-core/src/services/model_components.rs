//! An image model's family and the components it draws with, as the library
//! keeps them.
//!
//! A family is read from the main file's tensor names when the model is
//! imported. A model imported before gglib read them has none, and a retag
//! fills it from the file here, without ever changing one already set.

use crate::domain::{ImageFamily, Model};
use crate::ports::GgufParserPort;

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
