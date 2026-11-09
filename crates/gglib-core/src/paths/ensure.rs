//! Directory creation and verification utilities.
//!
//! Provides strategies for creating directories and verifying they are writable.
//! The `DirectoryCreationStrategy` enum does NOT include interactive/prompt variants;
//! adapter code should handle user interaction separately.

use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::Path;

use super::error::PathError;

/// Strategy for how to handle missing directories when ensuring they exist.
///
/// Note: This enum is intentionally non-interactive. CLI or GUI code that needs
/// to prompt users should handle that logic separately and then pass `AutoCreate`
/// or `Disallow` to `ensure_directory`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum DirectoryCreationStrategy {
    /// Create directories automatically if they are missing.
    #[default]
    AutoCreate,
    /// Do not create directories; return an error if missing.
    Disallow,
}

/// Ensure the provided directory exists and is writable according to the chosen strategy.
///
/// If the directory exists, verifies it's actually a directory and is writable.
/// If the directory doesn't exist, behavior depends on `strategy`:
/// - `AutoCreate`: Creates the directory (and parents)
/// - `Disallow`: Returns an error
pub fn ensure_directory(path: &Path, strategy: DirectoryCreationStrategy) -> Result<(), PathError> {
    if path.exists() {
        if !path.is_dir() {
            return Err(PathError::NotADirectory(path.to_path_buf()));
        }
    } else {
        match strategy {
            DirectoryCreationStrategy::AutoCreate => {
                fs::create_dir_all(path).map_err(|e| PathError::CreateFailed {
                    path: path.to_path_buf(),
                    reason: e.to_string(),
                })?;
            }
            DirectoryCreationStrategy::Disallow => {
                return Err(PathError::DirectoryNotFound(path.to_path_buf()));
            }
        }
    }

    verify_writable(path)?;
    Ok(())
}

/// Whether a folder at `path` can be written to, or made and then written.
///
/// The folder itself is tried when it is there, else the nearest folder
/// above it that is, since that is where making it would write. Decided by
/// writing a probe file ([`verify_writable`]), not by reading mode bits, so
/// a folder another user owns, or one under a read-only parent, is not taken
/// as writable.
#[must_use]
pub fn can_write_folder(path: &Path) -> bool {
    path.ancestors()
        .find(|folder| folder.exists())
        .is_some_and(|folder| folder.is_dir() && verify_writable(folder).is_ok())
}

/// Verify a directory is writable by attempting to create a test file.
fn verify_writable(path: &Path) -> Result<(), PathError> {
    let test_file = path.join(".gglib_write_test");
    let result = OpenOptions::new()
        .create(true)
        .write(true)
        .truncate(true)
        .open(&test_file);

    match result {
        Ok(mut file) => {
            let written = file.write_all(b"test");
            drop(file);
            // Removed whether the write landed or not: a status read probes
            // this folder each time, and must not leave the file behind.
            let _ = fs::remove_file(&test_file);
            written.map_err(|e| PathError::NotWritable {
                path: path.to_path_buf(),
                reason: e.to_string(),
            })
        }
        Err(err) => Err(PathError::NotWritable {
            path: path.to_path_buf(),
            reason: err.to_string(),
        }),
    }
}

#[cfg(test)]
#[path = "ensure_tests.rs"]
mod tests;
