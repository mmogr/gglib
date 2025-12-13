//! Platform-specific path detection and resolution.
//!
//! This module contains private helpers for detecting the runtime environment
//! (local repo vs installed binary) and resolving platform-appropriate paths.
//! Public API is exposed through sibling modules.

use std::env;
use std::fs;
use std::path::PathBuf;

use super::error::PathError;

/// Detect if we are running from the local repository.
///
/// Returns `Some(path)` if we are in a dev environment or running a release build
/// from within the source repo (e.g. `make setup`).
/// Returns `None` if we are running a standalone binary (e.g. installed via cargo install,
/// downloaded, or bundled as a macOS/Linux app).
#[allow(clippy::unnecessary_wraps)] // Option is needed for release builds
pub(super) fn detect_local_repo() -> Option<PathBuf> {
    let repo_root = PathBuf::from(env!("GGLIB_REPO_ROOT"));

    #[cfg(debug_assertions)]
    {
        // In debug mode, always assume we want to use the repo we are building from
        Some(repo_root)
    }

    #[cfg(not(debug_assertions))]
    {
        // In release mode, check if this binary was built from a local repo.

        // First, verify the repo path exists and looks like a valid gglib repo
        if !repo_root.exists()
            || (!repo_root.join(".git").exists() && !repo_root.join("Cargo.toml").exists())
        {
            return None;
        }

        // Strategy 1: Check for the .gglib_repo_path marker file created by build.rs
        let marker_file = repo_root.join("data").join(".gglib_repo_path");
        if marker_file.exists() {
            if let Ok(contents) = fs::read_to_string(&marker_file) {
                let marker_path = contents.trim();
                if marker_path == repo_root.to_string_lossy() {
                    return Some(repo_root);
                }
            }
        }

        // Strategy 2 (fallback): Check if executable is inside the repo
        if let Ok(exe_path) = env::current_exe() {
            if let Ok(canonical_exe) = exe_path.canonicalize() {
                if let Ok(canonical_repo) = repo_root.canonicalize() {
                    if canonical_exe.starts_with(&canonical_repo) {
                        return Some(repo_root);
                    }
                }
            }
        }

        None
    }
}

/// Check if we are running from a pre-built binary (not from the source repo).
///
/// Returns `true` if this is a standalone/installed binary.
/// Returns `false` if running from the source repository.
pub fn is_prebuilt_binary() -> bool {
    detect_local_repo().is_none()
}

/// Get the root directory for application data (database, config).
///
/// Resolution order:
/// 1. `GGLIB_DATA_DIR` environment variable (highest priority)
/// 2. Local repository (if running from source)
/// 3. System data directory (e.g., `~/.local/share/gglib`)
pub fn data_root() -> Result<PathBuf, PathError> {
    // 1. Runtime override (highest priority)
    if let Ok(path) = env::var("GGLIB_DATA_DIR") {
        return Ok(PathBuf::from(path));
    }

    // 2. Try local repo (e.g. make setup)
    if let Some(repo) = detect_local_repo() {
        return Ok(repo);
    }

    // 3. Default to system data directory
    let data_dir = dirs::data_local_dir().ok_or(PathError::NoDataDir)?;

    let root = data_dir.join("gglib");

    // Ensure it exists
    if !root.exists() {
        fs::create_dir_all(&root).map_err(|e| PathError::CreateFailed {
            path: root.clone(),
            reason: e.to_string(),
        })?;
    }

    Ok(root)
}

/// Get the root directory for application resources (binaries, static assets).
///
/// Resolution order:
/// 1. `GGLIB_RESOURCE_DIR` environment variable
/// 2. Local repository (if running from source)
/// 3. Falls back to data root
pub fn resource_root() -> Result<PathBuf, PathError> {
    // 1. Runtime override
    if let Ok(path) = env::var("GGLIB_RESOURCE_DIR") {
        return Ok(PathBuf::from(path));
    }

    // 2. Try local repo
    if let Some(repo) = detect_local_repo() {
        return Ok(repo);
    }

    // 3. Fallback to system data directory
    data_root()
}

/// Normalize a user-provided path, expanding `~` and making it absolute.
pub(super) fn normalize_user_path(raw: &str) -> Result<PathBuf, PathError> {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return Err(PathError::EmptyPath);
    }

    let expanded = if trimmed.starts_with("~/") || trimmed == "~" {
        let home = dirs::home_dir().ok_or(PathError::NoHomeDir)?;
        if trimmed == "~" {
            home
        } else {
            home.join(trimmed.trim_start_matches("~/"))
        }
    } else {
        PathBuf::from(trimmed)
    };

    if expanded.is_absolute() {
        Ok(expanded)
    } else {
        env::current_dir()
            .map(|cwd| cwd.join(expanded))
            .map_err(|e| PathError::CurrentDirError(e.to_string()))
    }
}
