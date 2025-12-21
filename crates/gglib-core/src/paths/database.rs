//! Database path resolution.
//!
//! Provides the canonical path to the gglib `SQLite` database file.

use std::fs;
use std::path::PathBuf;

use super::error::PathError;
use super::platform::data_root;

/// Get the path to the gglib database file.
///
/// Returns the path to `gglib.db` in the user data directory.
/// This is shared between dev and release builds.
///
/// The `data/` subdirectory is created if it doesn't exist.
pub fn database_path() -> Result<PathBuf, PathError> {
    let data_dir = data_root()?.join("data");

    fs::create_dir_all(&data_dir).map_err(|e| PathError::CreateFailed {
        path: data_dir.clone(),
        reason: e.to_string(),
    })?;

    Ok(data_dir.join("gglib.db"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_database_path_ends_with_gglib_db() {
        let result = database_path();
        assert!(result.is_ok());
        let path = result.unwrap();
        assert!(path.to_string_lossy().ends_with("gglib.db"));
    }
}
