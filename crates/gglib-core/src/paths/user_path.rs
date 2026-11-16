//! A folder put on, and taken off, the user's own `PATH`, as text.
//!
//! The rules are functions of the variable's text, so they are tested on
//! every OS.

/// `path`, a `PATH` value in Windows' form (entries separated by `;`), with
/// `folder` added as its last entry, or `None` when an entry already names
/// `folder`.
#[must_use]
pub fn with_folder(path: &str, folder: &str) -> Option<String> {
    let _ = (path, folder);
    None
}

/// `path` with every entry that names `folder` taken out, or `None` when no
/// entry does.
#[must_use]
pub fn without_folder(path: &str, folder: &str) -> Option<String> {
    let _ = (path, folder);
    None
}

#[cfg(test)]
#[path = "user_path_tests.rs"]
mod tests;
