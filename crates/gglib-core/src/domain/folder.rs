//! A chat's folder: where gglib's own file tools read in its runs, and the
//! two rules that make one.
//!
//! The folder is the chat session's to decide. `gglib chat`'s is the
//! directory it was launched in ([`launched`]), never this machine's home
//! folder or the filesystem's root; the page's is the folder the person
//! chose ([`chosen`]); a paired device's turn has none. Either rule answers
//! a canonical path, so what is shown as the folder is what is read. The
//! decision is [ADR 0023].
//!
//! [ADR 0023]: https://github.com/mmogr/gglib/blob/main/docs/adr/0023-a-chats-folder-is-where-gglibs-own-file-tools-read.md

use std::path::{Path, PathBuf};

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

/// The folder of a chat launched in `cwd`: its canonical path, or `None`
/// when that is this machine's home folder or the filesystem's root, which
/// are never read without being chosen.
///
/// A `cwd` that cannot be resolved is answered as given; the file tools
/// refuse each call there with their own words.
#[must_use]
pub fn launched(cwd: &Path) -> Option<PathBuf> {
    launched_in(cwd, dirs::home_dir().as_deref())
}

/// [`launched`] with `home` as this machine's home folder.
fn launched_in(cwd: &Path, home: Option<&Path>) -> Option<PathBuf> {
    let cwd = cwd.canonicalize().unwrap_or_else(|_| cwd.to_path_buf());
    let home = home.map(|home| home.canonicalize().unwrap_or_else(|_| home.to_path_buf()));
    let is_root = cwd.parent().is_none();
    (!is_root && home.as_deref() != Some(cwd.as_path())).then_some(cwd)
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
    let canonical = path
        .canonicalize()
        .map_err(|_| FolderError::Missing(path.to_path_buf()))?;
    if !canonical.is_dir() {
        return Err(FolderError::NotAFolder(path.to_path_buf()));
    }
    Ok(canonical)
}

#[cfg(test)]
#[path = "folder_tests.rs"]
mod tests;
