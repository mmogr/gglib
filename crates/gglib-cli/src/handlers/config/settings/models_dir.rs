//! `models-dir`, the one key of `gglib config` that is not a setting: the
//! download folder, stored as a line in the data root's `.env` rather than in
//! the database, so it is read before there is a library to open.

use anyhow::Result;
use gglib_app_services::types::ModelsDirectoryInfo;
use gglib_core::paths::{ModelsDirSource, resolve_models_dir, set_models_dir, unset_models_dir};

/// The row `config show` prints for `info`, the folder the settings page
/// shows: where it is, how it was chosen, and its state in the word every
/// surface uses.
pub(super) fn row(info: &ModelsDirectoryInfo) -> (String, String) {
    (
        "models-dir".to_owned(),
        format!("{} ({}, {})", info.path, info.source, info.state),
    )
}

/// Store `path` as the download folder, creating it when missing, and say
/// where downloads go: `GGLIB_MODELS_DIR` in this environment outranks it.
pub(super) fn set(path: &str) -> Result<()> {
    let stored = set_models_dir(path)?;
    let now = resolve_models_dir(None)?;
    if now.path == stored.path {
        println!("✓ models-dir is now {}", stored.path.display());
    } else {
        println!(
            "✓ models-dir is stored as {}, but GGLIB_MODELS_DIR in this environment \
             outranks it: downloads go to {}",
            stored.path.display(),
            now.path.display()
        );
    }
    Ok(())
}

/// Remove the stored download folder, so the default applies, and say
/// where downloads go now.
pub(super) fn unset() -> Result<()> {
    let now = unset_models_dir()?;
    if now.source == ModelsDirSource::Default {
        println!(
            "✓ models-dir is now unset: downloads go to the default, {}",
            now.path.display()
        );
    } else {
        println!(
            "✓ models-dir is now unset, but GGLIB_MODELS_DIR in this environment \
             outranks it: downloads go to {}",
            now.path.display()
        );
    }
    Ok(())
}
