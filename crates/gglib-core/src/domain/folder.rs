//! A chat's folder: where gglib's own file tools read in its runs, and the
//! two rules that make one.
//!
//! The folder is the chat session's to decide. `gglib chat`'s and `gglib
//! q`'s is the directory it was launched in ([`launched`]), never this
//! machine's home folder or a system folder ([`Places`]); the page's is the
//! folder the person chose ([`chosen`]), whatever it is; a paired device's
//! turn has none. Either rule answers a canonical path, so what is shown as
//! the folder is what is read. The decision is [ADR 0023].
//!
//! [ADR 0023]: https://github.com/mmogr/gglib/blob/main/docs/adr/0023-a-chats-folder-is-where-gglibs-own-file-tools-read.md

use std::path::{Path, PathBuf};

use crate::paths::canonical;

#[path = "folder_places.rs"]
mod places;

pub use places::Places;

/// Why a path cannot be a chosen folder.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum FolderError {
    /// The path is relative: a folder is named from the filesystem's root.
    #[error("'{}' is not an absolute path", .0.display())]
    NotAbsolute(PathBuf),
    /// Nothing is at the path, or it cannot be resolved.
    #[error("no folder at '{}'", .0.display())]
    Missing(PathBuf),
    /// The path resolves to something that is not a folder.
    #[error("'{}' is not a folder", .0.display())]
    NotAFolder(PathBuf),
}

/// Why a chat launched in a folder does not take it as its folder.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum NotAdopted {
    /// It is this machine's home folder.
    Home,
    /// It is a system folder ([`Places::is_system`]), named canonical.
    System(PathBuf),
}

/// The folder of a chat launched in `cwd`: its canonical path, unless that
/// is this machine's home folder or a system folder, which are never read
/// without being chosen.
///
/// A `cwd` that cannot be resolved is answered as given; the file tools
/// refuse each call there with their own words.
///
/// # Errors
///
/// [`NotAdopted`], saying which of the two it is.
pub fn launched(cwd: &Path) -> Result<PathBuf, NotAdopted> {
    launched_on(cwd, &Places::current())
}

/// [`launched`] on a machine whose places are `places`.
///
/// # Errors
///
/// [`NotAdopted`], as [`adopted`] answers it.
pub fn launched_on(cwd: &Path, places: &Places) -> Result<PathBuf, NotAdopted> {
    adopted(canonical(cwd).unwrap_or_else(|_| cwd.to_path_buf()), places)
}

/// The one rule, on values: whether a chat launched in the canonical `dir`
/// takes it as its folder.
///
/// `places` are the machine's. The rule reads no disk and no environment,
/// so it answers alike on every OS.
///
/// # Errors
///
/// [`NotAdopted::Home`] for this user's home folder,
/// [`NotAdopted::System`] for a system folder.
pub fn adopted(dir: PathBuf, places: &Places) -> Result<PathBuf, NotAdopted> {
    if places.is_home(&dir) {
        Err(NotAdopted::Home)
    } else if places.is_system(&dir) {
        Err(NotAdopted::System(dir))
    } else {
        Ok(dir)
    }
}

/// The folder a person chose at `path`: its canonical path, once it is
/// absolute, exists and is a folder.
///
/// # Errors
///
/// [`FolderError`], naming `path` as it was given.
pub fn chosen(path: &Path) -> Result<PathBuf, FolderError> {
    if !path.is_absolute() {
        return Err(FolderError::NotAbsolute(path.to_path_buf()));
    }
    let resolved = canonical(path).map_err(|_| FolderError::Missing(path.to_path_buf()))?;
    if !resolved.is_dir() {
        return Err(FolderError::NotAFolder(path.to_path_buf()));
    }
    Ok(resolved)
}

#[cfg(test)]
#[path = "folder_tests.rs"]
mod tests;
