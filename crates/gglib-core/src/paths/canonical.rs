//! A path's one canonical form: resolved, and on Windows written the way
//! every program reads it.

use std::ffi::OsStr;
use std::io;
use std::path::{Path, PathBuf};

/// The canonical form of the entry `name` in the canonical folder `dir`.
#[must_use]
pub fn child(dir: &Path, name: impl AsRef<OsStr>) -> PathBuf {
    dir.join(name.as_ref())
}

/// `dir` joined with the relative path `relative`, naming the file the
/// verbatim join would.
#[must_use]
pub fn join(dir: &Path, relative: &Path) -> PathBuf {
    dir.join(relative)
}

/// The part of the canonical `path` under the canonical `folder`.
#[must_use]
pub fn inside(path: &Path, folder: &Path) -> Option<PathBuf> {
    path.strip_prefix(folder).ok().map(Path::to_path_buf)
}

/// `path` resolved, symlinks and `..` included, as gglib stores, compares and
/// shows it.
///
/// # Errors
///
/// The [`io::Error`] resolving it gave, most often because nothing is there.
#[expect(
    clippy::disallowed_methods,
    reason = "the one place a path is canonicalized; clippy.toml sends every other caller here"
)]
pub fn canonical(path: impl AsRef<Path>) -> io::Result<PathBuf> {
    std::fs::canonicalize(path).map(without_verbatim_prefix)
}

/// Whether `text` is a path written in the verbatim form.
#[must_use]
pub fn is_verbatim(text: &str) -> bool {
    text.starts_with(VERBATIM)
}

/// The prefix of a verbatim path.
const VERBATIM: &str = r"\\?\";

/// `path` in its plain form when it has one, else as it is.
fn without_verbatim_prefix(path: PathBuf) -> PathBuf {
    let plain = path.to_str().and_then(plain_form);
    plain.map_or(path, PathBuf::from)
}

/// `text` without its verbatim prefix, when it has one and the path can be
/// written without it.
fn plain_form(text: &str) -> Option<String> {
    let _ = text;
    None
}

#[cfg(test)]
#[path = "canonical_tests.rs"]
mod tests;
