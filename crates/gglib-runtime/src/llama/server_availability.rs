//! llama-server binary availability checking and path resolution.
//!
//! This module provides centralized logic for resolving the llama-server binary path
//! with support for multiple resolution strategies and legacy path migration.

use std::path::{Path, PathBuf};
use thiserror::Error;

/// Errors that can occur when resolving or validating the llama-server binary.
#[derive(Debug, Error)]
pub enum LlamaServerError {
    /// The llama-server binary was not found at the expected location.
    #[error(
        "llama-server binary not found at: {path}\n\nPlease install llama.cpp by running:\n  gglib llama install"
    )]
    NotFound {
        /// The path where the binary was expected
        path: PathBuf,
        /// Optional legacy path where an old installation was detected
        legacy_path: Option<PathBuf>,
    },

    /// The binary exists but is not executable (permission denied).
    #[error(
        "llama-server binary exists but is not executable: {path}\n\nPlease check file permissions or reinstall with:\n  gglib llama install"
    )]
    NotExecutable {
        /// The path to the non-executable binary
        path: PathBuf,
    },

    /// The binary exists but permission was denied when trying to access it.
    #[error(
        "Permission denied accessing llama-server binary: {path}\n\nPlease check file permissions."
    )]
    PermissionDenied {
        /// The path to the inaccessible binary
        path: PathBuf,
    },

    /// Failed to resolve the path (e.g., data directory not available).
    #[error("Failed to resolve llama-server path: {0}")]
    PathResolution(String),
}

/// Result type for llama-server operations.
pub type LlamaServerResult<T> = Result<T, LlamaServerError>;

/// Resolve the llama-server binary path with automatic fallback and validation.
///
/// This function applies the following precedence order:
/// 1. `GGLIB_LLAMA_SERVER_PATH` environment variable (explicit override)
/// 2. Default path from `gglib_core::paths::llama_server_path()`
/// 3. Legacy paths (pre-refactor locations) with migration hints
///
/// After resolving a candidate path, this function validates that:
/// - The file exists
/// - The file is executable (has execute permissions)
///
/// # Returns
///
/// - `Ok(PathBuf)` - The validated path to the llama-server binary
/// - `Err(LlamaServerError)` - Detailed error with resolution suggestions
///
/// # Examples
///
/// ```no_run
/// use gglib_runtime::llama::resolve_llama_server;
///
/// match resolve_llama_server() {
///     Ok(path) => println!("Found llama-server at: {}", path.display()),
///     Err(e) => eprintln!("Error: {}", e),
/// }
/// ```
pub fn resolve_llama_server() -> LlamaServerResult<PathBuf> {
    // Strategy 1: Check for explicit environment variable override
    if let Ok(env_path) = std::env::var("GGLIB_LLAMA_SERVER_PATH") {
        let path = PathBuf::from(env_path);
        return validate_binary(&path);
    }

    // Strategy 2: Use default path resolution
    let default_path = gglib_core::paths::llama_server_path()
        .map_err(|e| LlamaServerError::PathResolution(e.to_string()))?;

    // Check if default path works
    match validate_binary(&default_path) {
        Ok(path) => Ok(path),
        Err(default_err) => {
            // Strategy 3: Probe legacy paths for migration hints
            if let Some(legacy_path) = probe_legacy_paths() {
                // Found a legacy installation - return error with migration hint
                Err(LlamaServerError::NotFound {
                    path: default_path,
                    legacy_path: Some(legacy_path),
                })
            } else {
                // No legacy path found - return original validation error
                Err(default_err)
            }
        }
    }
}

/// Validate that a binary exists and is executable.
fn validate_binary(path: &Path) -> LlamaServerResult<PathBuf> {
    // Check if the file exists
    if !path.exists() {
        return Err(LlamaServerError::NotFound {
            path: path.to_path_buf(),
            legacy_path: None,
        });
    }

    // Check if the file is executable
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        match std::fs::metadata(path) {
            Ok(metadata) => {
                let permissions = metadata.permissions();
                let mode = permissions.mode();
                // Check if any execute bit is set (owner, group, or other)
                if mode & 0o111 == 0 {
                    return Err(LlamaServerError::NotExecutable {
                        path: path.to_path_buf(),
                    });
                }
            }
            Err(e) if e.kind() == std::io::ErrorKind::PermissionDenied => {
                return Err(LlamaServerError::PermissionDenied {
                    path: path.to_path_buf(),
                });
            }
            Err(e) => {
                return Err(LlamaServerError::PathResolution(format!(
                    "Failed to read metadata for {}: {}",
                    path.display(),
                    e
                )));
            }
        }
    }

    // On Windows, just check if the file exists (already done above)
    #[cfg(not(unix))]
    {
        // Additional Windows-specific checks could go here if needed
        let _ = path; // Use path to avoid unused variable warning
    }

    Ok(path.to_path_buf())
}

/// Probe for legacy llama-server installation paths.
///
/// This function checks common locations where llama-server may have been
/// installed in previous versions of gglib, before recent path refactors.
///
/// Returns `Some(PathBuf)` if a valid legacy binary is found, `None` otherwise.
fn probe_legacy_paths() -> Option<PathBuf> {
    // Legacy path candidates (add more as needed based on historical locations)
    let legacy_candidates: Vec<PathBuf> = vec![
        // Example legacy paths - adjust based on actual refactor history
        // These would be paths used before PRs #226-227
    ];

    legacy_candidates
        .into_iter()
        .find(|candidate| candidate.exists() && validate_binary(candidate).is_ok())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_validate_binary_not_found() {
        let nonexistent = PathBuf::from("/nonexistent/path/to/llama-server");
        let result = validate_binary(&nonexistent);
        assert!(matches!(result, Err(LlamaServerError::NotFound { .. })));
    }

    #[test]
    fn test_legacy_path_probe_returns_none_for_empty_candidates() {
        // With no legacy candidates defined, should return None
        let result = probe_legacy_paths();
        assert_eq!(result, None);
    }
}
