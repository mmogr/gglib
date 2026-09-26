//! Database path resolution.
//!
//! Provides the canonical path to the gglib `SQLite` database file.

use std::path::PathBuf;

use super::error::PathError;
use super::platform::data_root;
use super::private::create_private_dir;

/// Get the path to the gglib database file.
///
/// Returns the path to `gglib.db` in the user data directory.
/// This is shared between dev and release builds.
///
/// The `data/` subdirectory is created if it doesn't exist, and is this
/// user's alone either way: see [`create_private_dir`].
pub fn database_path() -> Result<PathBuf, PathError> {
    let data_dir = data_root()?.join("data");

    create_private_dir(&data_dir).map_err(|e| PathError::CreateFailed {
        path: data_dir.clone(),
        reason: e.to_string(),
    })?;

    Ok(data_dir.join("gglib.db"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::paths::test_utils::{ENV_LOCK, EnvVarGuard};
    use tempfile::tempdir;

    #[test]
    fn test_database_path_ends_with_gglib_db() {
        // `database_path()` creates `<data root>/data`, so it runs under the
        // lock every test that points GGLIB_DATA_DIR at a temporary root holds,
        // and in a root of its own (#1082). Without the lock it can resolve
        // into a neighbour's root as that root is being removed.
        let _guard = ENV_LOCK.lock().unwrap();
        let temp = tempdir().unwrap();
        let _env_guard = EnvVarGuard::set("GGLIB_DATA_DIR", temp.path().to_string_lossy().as_ref());

        let result = database_path();
        assert!(result.is_ok());
        let path = result.unwrap();
        assert!(path.to_string_lossy().ends_with("gglib.db"));
        assert!(
            path.starts_with(temp.path()),
            "the database path is outside the test's own root: {}",
            path.display()
        );
    }
}
