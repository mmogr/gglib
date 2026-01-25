//! Filesystem provider trait for testable path resolution.

use super::types::AttemptOutcome;
use std::path::Path;

/// Trait for filesystem operations (injectable for testing).
pub trait FsProvider {
    /// Check if a path exists and is a valid executable.
    /// Returns Ok if the file is executable, or a specific outcome otherwise.
    fn check_executable(&self, path: &Path) -> AttemptOutcome;
}

/// Production filesystem provider that uses real filesystem operations.
pub struct SystemFs;

impl FsProvider for SystemFs {
    fn check_executable(&self, path: &Path) -> AttemptOutcome {
        // Check if path exists
        if !path.exists() {
            return AttemptOutcome::NotFound;
        }

        // Check if it's a file
        if !path.is_file() {
            return AttemptOutcome::NotAFile;
        }

        // Check if executable (Unix only - Windows assumes .exe/.cmd are executable)
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            match std::fs::metadata(path) {
                Ok(metadata) => {
                    let permissions = metadata.permissions();
                    if permissions.mode() & 0o111 == 0 {
                        return AttemptOutcome::NotExecutable;
                    }
                }
                Err(e) if e.kind() == std::io::ErrorKind::PermissionDenied => {
                    return AttemptOutcome::PermissionDenied;
                }
                Err(e) => {
                    return AttemptOutcome::IoError(e.to_string());
                }
            }
        }

        AttemptOutcome::Ok
    }
}

/// Test/mock filesystem provider with predefined responses.
#[cfg(test)]
#[derive(Default)]
pub struct MockFs {
    executables: std::collections::HashSet<std::path::PathBuf>,
    non_executables: std::collections::HashSet<std::path::PathBuf>,
}

#[cfg(test)]
impl MockFs {
    pub fn new() -> Self {
        Self::default()
    }

    #[must_use]
    pub fn with_executable(mut self, path: impl Into<std::path::PathBuf>) -> Self {
        self.executables.insert(path.into());
        self
    }

    #[must_use]
    pub fn with_non_executable(mut self, path: impl Into<std::path::PathBuf>) -> Self {
        self.non_executables.insert(path.into());
        self
    }
}

#[cfg(test)]
impl FsProvider for MockFs {
    fn check_executable(&self, path: &Path) -> AttemptOutcome {
        if self.executables.contains(path) {
            AttemptOutcome::Ok
        } else if self.non_executables.contains(path) {
            AttemptOutcome::NotExecutable
        } else {
            AttemptOutcome::NotFound
        }
    }
}
