//! Where a command is searched for, and in what order.

use super::env::EnvProvider;
use super::fs::FsProvider;
use super::platform::Platform;
use super::types::{Attempt, AttemptOutcome};
use std::path::{Path, PathBuf};

/// Search for an executable in various platform-specific locations.
///
/// Each search records every candidate it checks in `attempts`, in the order
/// it checks them, and stops at the first that is an executable: that
/// candidate is what it returns.
pub(crate) struct ExecutableSearcher<'a> {
    env: &'a dyn EnvProvider,
    fs: &'a dyn FsProvider,
    platform: &'a Platform,
}

impl<'a> ExecutableSearcher<'a> {
    pub(crate) const fn new(
        env: &'a dyn EnvProvider,
        fs: &'a dyn FsProvider,
        platform: &'a Platform,
    ) -> Self {
        Self { env, fs, platform }
    }

    /// Check one candidate and record the attempt.
    fn probe(&self, candidate: PathBuf, attempts: &mut Vec<Attempt>) -> Option<PathBuf> {
        let outcome = self.fs.check_executable(&candidate, self.platform);
        let found = (outcome == AttemptOutcome::Ok).then(|| candidate.clone());
        attempts.push(Attempt { candidate, outcome });
        found
    }

    /// Check for `command` in each directory in turn, skipping an empty
    /// entry. Each directory is checked under every name the platform gives
    /// the command before the next directory is.
    fn probe_dirs<D: AsRef<Path>>(
        &self,
        dirs: impl IntoIterator<Item = D>,
        command: &str,
        attempts: &mut Vec<Attempt>,
    ) -> Option<PathBuf> {
        let names = self.platform.names(command);

        dirs.into_iter()
            .filter(|dir| !dir.as_ref().as_os_str().is_empty())
            .find_map(|dir| {
                names
                    .iter()
                    .find_map(|name| self.probe(dir.as_ref().join(name), attempts))
            })
    }

    /// Search for a command in PATH environment variable.
    pub(crate) fn search_in_path(
        &self,
        command: &str,
        attempts: &mut Vec<Attempt>,
    ) -> Option<PathBuf> {
        let path_var = self.env.get("PATH")?;
        let dirs = self.platform.split(path_var.to_str()?);
        self.probe_dirs(dirs, command, attempts)
    }

    /// Search in /etc/paths and /etc/paths.d/*, where the platform has them
    /// (macOS).
    pub(crate) fn search_in_etc_paths(
        &self,
        command: &str,
        attempts: &mut Vec<Attempt>,
    ) -> Option<PathBuf> {
        if !self.platform.reads_etc_paths() {
            return None;
        }
        let mut dirs = Vec::new();

        // Read /etc/paths
        if let Ok(contents) = std::fs::read_to_string("/etc/paths") {
            for line in contents.lines() {
                let line = line.trim();
                if !line.is_empty() && !line.starts_with('#') {
                    dirs.push(line.to_string());
                }
            }
        }

        // Read /etc/paths.d/*
        if let Ok(entries) = std::fs::read_dir("/etc/paths.d") {
            for entry in entries.flatten() {
                if let Ok(contents) = std::fs::read_to_string(entry.path()) {
                    for line in contents.lines() {
                        let line = line.trim();
                        if !line.is_empty() && !line.starts_with('#') {
                            dirs.push(line.to_string());
                        }
                    }
                }
            }
        }

        self.probe_dirs(dirs, command, attempts)
    }

    /// Search in platform-specific default locations.
    pub(crate) fn search_platform_defaults(
        &self,
        command: &str,
        attempts: &mut Vec<Attempt>,
    ) -> Option<PathBuf> {
        self.probe_dirs(self.platform.default_dirs(), command, attempts)
    }

    /// Search in Node.js version manager shims.
    pub(crate) fn search_node_managers(
        &self,
        command: &str,
        attempts: &mut Vec<Attempt>,
    ) -> Option<PathBuf> {
        // Only search for npm/npx/node commands
        if !matches!(command, "npm" | "npx" | "node") {
            return None;
        }

        let home = self.env.get("HOME")?;
        let home = Path::new(home.to_str()?);

        // Try asdf first (shim takes precedence), then volta, then nvm
        let shims = [home.join(".asdf/shims"), home.join(".volta/bin")];
        self.probe_dirs(shims, command, attempts)
            .or_else(|| self.search_nvm(&home.join(".nvm"), command, attempts))
    }

    /// Search nvm's versions: the default alias's, then each from the newest.
    fn search_nvm(
        &self,
        nvm_dir: &Path,
        command: &str,
        attempts: &mut Vec<Attempt>,
    ) -> Option<PathBuf> {
        if !nvm_dir.exists() {
            return None;
        }
        let bin = |version: &str| [nvm_dir.join(format!("versions/node/{version}/bin"))];

        // Check for default alias
        if let Ok(default_version) = std::fs::read_to_string(nvm_dir.join("alias/default")) {
            if let Some(found) = self.probe_dirs(bin(default_version.trim()), command, attempts) {
                return Some(found);
            }
        }

        // Fall back to scanning for latest version
        let mut versions: Vec<String> = std::fs::read_dir(nvm_dir.join("versions/node"))
            .ok()?
            .filter_map(std::result::Result::ok)
            .filter_map(|e| e.file_name().into_string().ok())
            .collect();
        versions.sort();

        // Try versions from newest to oldest
        versions
            .iter()
            .rev()
            .find_map(|version| self.probe_dirs(bin(version), command, attempts))
    }

    /// Search in user-provided additional paths.
    pub(crate) fn search_user_paths(
        &self,
        command: &str,
        user_paths: &[String],
        attempts: &mut Vec<Attempt>,
    ) -> Option<PathBuf> {
        self.probe_dirs(user_paths, command, attempts)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::resolver::env::MockEnv;
    use crate::resolver::fs::MockFs;

    /// Unix's rules, so the `:` list splits as it does there on every OS.
    #[test]
    fn test_search_in_path_finds_executable() {
        let env = MockEnv::new().with_var("PATH", "/usr/bin:/usr/local/bin");
        let fs = MockFs::new().with_executable("/usr/local/bin/npx");

        let unix = Platform::unix();
        let searcher = ExecutableSearcher::new(&env, &fs, &unix);
        let mut attempts = Vec::new();
        searcher.search_in_path("npx", &mut attempts);

        assert!(attempts.iter().any(|a| a.outcome == AttemptOutcome::Ok));
        assert_eq!(
            attempts
                .iter()
                .find(|a| a.outcome == AttemptOutcome::Ok)
                .unwrap()
                .candidate,
            PathBuf::from("/usr/local/bin/npx")
        );
    }

    #[test]
    fn test_search_in_empty_path() {
        let env = MockEnv::new().with_var("PATH", "");
        let fs = MockFs::new();

        let host = Platform::host();
        let searcher = ExecutableSearcher::new(&env, &fs, &host);
        let mut attempts = Vec::new();
        searcher.search_in_path("npx", &mut attempts);

        assert!(attempts.is_empty());
    }
}
