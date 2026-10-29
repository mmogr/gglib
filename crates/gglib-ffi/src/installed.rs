//! The models already on the device: their list, and removing one, files
//! and row.

use std::collections::HashSet;
use std::path::{Path, PathBuf};

use gglib_core::domain::capability_tags::capabilities_of;
use gglib_core::domain::{Model, ModelFolder};
use gglib_core::ports::{ModelFilesRepositoryPort, ModelRepository};
use gglib_core::services::{resident_bytes, total_model_bytes, weights_paths};

use crate::errors::GglibError;
use crate::verdict::{FitVerdict, ModelShape};

/// A model on the device.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct InstalledModel {
    /// Its id in the library, which [`crate::Library::remove`] takes.
    pub id: i64,
    /// Its name, which a served engine's model list calls it by.
    pub name: String,
    /// The bytes its files take on the device: the weights, and a projector
    /// or companions it has.
    pub bytes: u64,
    /// The context it was trained for, when its header says.
    pub trained_ctx: Option<u64>,
    /// What it can do beyond chat, in the words a model list uses
    /// (`reasoning`, `vision` and so on).
    pub capabilities: Vec<String>,
    /// Whether it fits the device, at the budget the library holds.
    pub verdict: FitVerdict,
}

/// Every model in the library, in the library's order, judged at `budget`.
pub(crate) async fn installed(
    models: &dyn ModelRepository,
    budget: Option<u64>,
) -> Result<Vec<InstalledModel>, GglibError> {
    Ok(models
        .list()
        .await?
        .into_iter()
        .map(|model| {
            let weights = Some(total_model_bytes(&model.file_path));
            let (verdict, _) = ModelShape::of_model(&model).judge(weights, budget);
            InstalledModel {
                id: model.id,
                bytes: resident_bytes(&model),
                trained_ctx: model.context_length,
                capabilities: capabilities_of(
                    &model.tags,
                    model.image_input(),
                    model.generates_images(),
                )
                .into_iter()
                .map(str::to_owned)
                .collect(),
                verdict,
                name: model.name,
            }
        })
        .collect())
}

/// Delete model `id`'s files, then its row.
///
/// Its files are its weights (every shard), the files its download recorded,
/// and its projector and companions, except a file another model also names.
/// Then each folder a file was in, up to `models_dir`, goes once it is empty.
/// A file already gone is no error, and a file that cannot be deleted stops
/// the removal before the row goes, so the model can be removed again.
pub(crate) async fn remove(
    models: &dyn ModelRepository,
    model_files: &dyn ModelFilesRepositoryPort,
    models_dir: &Path,
    id: i64,
) -> Result<(), GglibError> {
    let model = models.get_by_id(id).await?;
    let mut others = Vec::new();
    for other in models.list().await? {
        if other.id != id {
            others.push(other);
        }
    }
    let rows = model_files.get_by_model_id(id).await?;
    let folder = ModelFolder::of(&model.file_path, &rows);
    let named_by_others: HashSet<PathBuf> = others.iter().flat_map(files_named_by).collect();

    let mut files: Vec<PathBuf> = weights_paths(&model.file_path);
    files.extend(rows.iter().map(|row| folder.file(&row.file_path)));
    files.extend(files_named_by(&model));
    files.retain(|file| !named_by_others.contains(file));
    files.sort();
    files.dedup();

    for file in &files {
        match std::fs::remove_file(file) {
            Ok(()) => {}
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => return Err(GglibError::from_io(&error)),
        }
    }
    models.delete(id).await?;
    for file in &files {
        remove_empty_folders(file, models_dir);
    }
    Ok(())
}

/// The files a model's row names: its weights' first file, its projector
/// and its companions.
fn files_named_by(model: &Model) -> Vec<PathBuf> {
    std::iter::once(model.file_path.clone())
        .chain(model.projector_path.clone())
        .chain(
            model
                .components
                .iter()
                .map(|component| component.path.clone()),
        )
        .collect()
}

/// Remove each folder `file` was in that is now empty, from its own up to,
/// and not including, `models_dir`. A folder outside `models_dir` is left
/// alone. Both are compared as the file system resolves them, since the
/// library may record a path through a link (`/var` on macOS is one).
fn remove_empty_folders(file: &Path, models_dir: &Path) {
    let (Ok(models_dir), Some(Ok(folder))) = (
        models_dir.canonicalize(),
        file.parent().map(Path::canonicalize),
    ) else {
        return;
    };
    for folder in folder.ancestors() {
        if folder == models_dir || !folder.starts_with(&models_dir) {
            return;
        }
        // `remove_dir` refuses a folder with anything in it, which is the
        // only test of emptiness that cannot race a file being written.
        if std::fs::remove_dir(folder).is_err() {
            return;
        }
    }
}

#[cfg(test)]
#[path = "installed_tests.rs"]
mod tests;
