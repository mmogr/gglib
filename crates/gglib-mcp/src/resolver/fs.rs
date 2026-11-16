//! Filesystem provider trait for testable path resolution.

use super::platform::Platform;
use super::types::AttemptOutcome;
use std::path::Path;

/// Trait for filesystem operations (injectable for testing).
pub(crate) trait FsProvider {
    /// Check if a path exists and is a program by `platform`'s rules.
    /// Returns Ok if it is, or a specific outcome otherwise.
    fn check_executable(&self, path: &Path, platform: &Platform) -> AttemptOutcome;
}

/// Production filesystem provider that uses real filesystem operations.
pub(crate) struct SystemFs;

impl FsProvider for SystemFs {
    fn check_executable(&self, path: &Path, platform: &Platform) -> AttemptOutcome {
        if !path.exists() {
            return AttemptOutcome::NotFound;
        }
        if !path.is_file() {
            return AttemptOutcome::NotAFile;
        }
        platform.check_runnable(path)
    }
}

/// Test/mock filesystem provider with predefined responses. It ignores the
/// platform's rules: what it was given is a program, and nothing else is there.
#[cfg(test)]
#[derive(Default)]
pub(crate) struct MockFs {
    executables: std::collections::HashSet<std::path::PathBuf>,
}

#[cfg(test)]
impl MockFs {
    pub(crate) fn new() -> Self {
        Self::default()
    }

    #[must_use]
    pub(crate) fn with_executable(mut self, path: impl Into<std::path::PathBuf>) -> Self {
        self.executables.insert(path.into());
        self
    }
}

#[cfg(test)]
impl FsProvider for MockFs {
    fn check_executable(&self, path: &Path, _platform: &Platform) -> AttemptOutcome {
        if self.executables.contains(path) {
            AttemptOutcome::Ok
        } else {
            AttemptOutcome::NotFound
        }
    }
}
