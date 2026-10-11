//! Removing the image runtime: `.sd/`, whole.

use anyhow::Result;
use gglib_core::paths::sd_data_dir;
use std::path::Path;

use crate::llama::UninstallOutcome;

/// Remove everything gglib installed for image generation, `.sd/` whole.
///
/// That is the binary and its library, the record, a source checkout and any
/// leftover download. Unconditional; the confirmation, and the refusal while
/// an image model is running, belong to the caller.
pub fn uninstall_sd() -> Result<UninstallOutcome> {
    uninstall_sd_at(&sd_data_dir()?)
}

/// Whether there is anything for [`uninstall_sd`] to remove.
pub fn sd_files_present() -> Result<bool> {
    Ok(sd_data_dir()?.exists())
}

/// [`uninstall_sd`] for the directory `dir`.
pub(crate) fn uninstall_sd_at(dir: &Path) -> Result<UninstallOutcome> {
    if !dir.exists() {
        return Ok(UninstallOutcome {
            was_installed: false,
            removed_paths: Vec::new(),
        });
    }
    std::fs::remove_dir_all(dir)?;
    Ok(UninstallOutcome {
        was_installed: true,
        removed_paths: vec![dir.display().to_string()],
    })
}
