//! Platform-specific executable search logic.

use super::env::EnvProvider;
use super::fs::FsProvider;
use super::types::{Attempt, AttemptOutcome};
use std::path::PathBuf;

/// Search for an executable in various platform-specific locations.
pub struct ExecutableSearcher<'a> {
    env: &'a dyn EnvProvider,
    fs: &'a dyn FsProvider,
}

impl<'a> ExecutableSearcher<'a> {
    pub fn new(env: &'a dyn EnvProvider, fs: &'a dyn FsProvider) -> Self {
        Self { env, fs }
    }

    /// Search for a command in PATH environment variable.
    pub fn search_in_path(&self, command: &str) -> Vec<Attempt> {
        let mut attempts = Vec::new();

        if let Some(path_var) = self.env.get("PATH") {
            if let Some(path_str) = path_var.to_str() {
                for dir in path_str.split(Self::path_separator()) {
                    if dir.is_empty() {
                        continue;
                    }

                    let mut candidate = PathBuf::from(dir);
                    candidate.push(command);

                    // Try with PATHEXT variants on Windows
                    #[cfg(windows)]
                    {
                        for variant in self.pathext_variants(command) {
                            let mut win_candidate = PathBuf::from(dir);
                            win_candidate.push(&variant);
                            let outcome = self.fs.check_executable(&win_candidate);
                            attempts.push(Attempt {
                                candidate: win_candidate,
                                outcome: outcome.clone(),
                            });
                            if outcome == AttemptOutcome::Ok {
                                return attempts; // Early return on first success
                            }
                        }
                    }

                    #[cfg(not(windows))]
                    {
                        let outcome = self.fs.check_executable(&candidate);
                        attempts.push(Attempt {
                            candidate,
                            outcome: outcome.clone(),
                        });
                        if outcome == AttemptOutcome::Ok {
                            return attempts; // Early return on first success
                        }
                    }
                }
            }
        }

        attempts
    }

    /// Search in /etc/paths and /etc/paths.d/* (macOS-specific).
    #[cfg(target_os = "macos")]
    pub fn search_in_etc_paths(&self, command: &str) -> Vec<Attempt> {
        let mut attempts = Vec::new();
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

        // Check each directory
        for dir in dirs {
            let mut candidate = PathBuf::from(dir);
            candidate.push(command);
            let outcome = self.fs.check_executable(&candidate);
            attempts.push(Attempt {
                candidate,
                outcome: outcome.clone(),
            });
            if outcome == AttemptOutcome::Ok {
                return attempts; // Early return on success
            }
        }

        attempts
    }

    #[cfg(not(target_os = "macos"))]
    pub const fn search_in_etc_paths(&self, _command: &str) -> Vec<Attempt> {
        let _ = self; // Silence unused self warning - needed for API consistency with macOS impl
        Vec::new() // No-op on non-macOS
    }

    /// Search in platform-specific default locations.
    pub fn search_platform_defaults(&self, command: &str) -> Vec<Attempt> {
        let mut attempts = Vec::new();
        let candidates = Self::platform_default_dirs();

        for dir in candidates {
            let mut candidate = PathBuf::from(dir);
            candidate.push(command);

            #[cfg(windows)]
            {
                for variant in self.pathext_variants(command) {
                    let mut win_candidate = PathBuf::from(dir);
                    win_candidate.push(&variant);
                    let outcome = self.fs.check_executable(&win_candidate);
                    attempts.push(Attempt {
                        candidate: win_candidate,
                        outcome: outcome.clone(),
                    });
                    if outcome == AttemptOutcome::Ok {
                        return attempts;
                    }
                }
            }

            #[cfg(not(windows))]
            {
                let outcome = self.fs.check_executable(&candidate);
                attempts.push(Attempt {
                    candidate,
                    outcome: outcome.clone(),
                });
                if outcome == AttemptOutcome::Ok {
                    return attempts;
                }
            }
        }

        attempts
    }

    /// Search in Node.js version manager shims.
    pub fn search_node_managers(&self, command: &str) -> Vec<Attempt> {
        let mut attempts = Vec::new();

        // Only search for npm/npx/node commands
        if !matches!(command, "npm" | "npx" | "node") {
            return attempts;
        }

        // Try asdf first (shim takes precedence)
        if let Some(home) = self
            .env
            .get("HOME")
            .and_then(|h| h.to_str().map(String::from))
        {
            let asdf_shim = PathBuf::from(&home).join(".asdf/shims").join(command);
            let outcome = self.fs.check_executable(&asdf_shim);
            attempts.push(Attempt {
                candidate: asdf_shim,
                outcome: outcome.clone(),
            });
            if outcome == AttemptOutcome::Ok {
                return attempts;
            }

            // Try volta
            let volta_bin = PathBuf::from(&home).join(".volta/bin").join(command);
            let outcome = self.fs.check_executable(&volta_bin);
            attempts.push(Attempt {
                candidate: volta_bin,
                outcome: outcome.clone(),
            });
            if outcome == AttemptOutcome::Ok {
                return attempts;
            }

            // Try nvm (scan for default alias, then latest version)
            let nvm_dir = PathBuf::from(&home).join(".nvm");
            if nvm_dir.exists() {
                // Check for default alias
                if let Ok(default_version) = std::fs::read_to_string(nvm_dir.join("alias/default"))
                {
                    let version = default_version.trim();
                    let default_bin =
                        nvm_dir.join(format!("versions/node/{version}/bin/{command}"));
                    let outcome = self.fs.check_executable(&default_bin);
                    attempts.push(Attempt {
                        candidate: default_bin,
                        outcome: outcome.clone(),
                    });
                    if outcome == AttemptOutcome::Ok {
                        return attempts;
                    }
                }

                // Fall back to scanning for latest version
                if let Ok(versions_dir) = std::fs::read_dir(nvm_dir.join("versions/node")) {
                    let mut versions: Vec<String> = versions_dir
                        .filter_map(std::result::Result::ok)
                        .filter_map(|e| e.file_name().into_string().ok())
                        .collect();
                    versions.sort();

                    // Try versions from newest to oldest
                    for version in versions.iter().rev() {
                        let version_bin =
                            nvm_dir.join(format!("versions/node/{version}/bin/{command}"));
                        let outcome = self.fs.check_executable(&version_bin);
                        attempts.push(Attempt {
                            candidate: version_bin,
                            outcome: outcome.clone(),
                        });
                        if outcome == AttemptOutcome::Ok {
                            return attempts;
                        }
                    }
                }
            }
        }

        attempts
    }

    /// Search in user-provided additional paths.
    pub fn search_user_paths(&self, command: &str, user_paths: &[String]) -> Vec<Attempt> {
        let mut attempts = Vec::new();

        for dir in user_paths {
            if dir.is_empty() {
                continue;
            }

            let mut candidate = PathBuf::from(dir);
            candidate.push(command);

            #[cfg(windows)]
            {
                for variant in self.pathext_variants(command) {
                    let mut win_candidate = PathBuf::from(dir);
                    win_candidate.push(&variant);
                    let outcome = self.fs.check_executable(&win_candidate);
                    attempts.push(Attempt {
                        candidate: win_candidate,
                        outcome: outcome.clone(),
                    });
                    if outcome == AttemptOutcome::Ok {
                        return attempts;
                    }
                }
            }

            #[cfg(not(windows))]
            {
                let outcome = self.fs.check_executable(&candidate);
                attempts.push(Attempt {
                    candidate,
                    outcome: outcome.clone(),
                });
                if outcome == AttemptOutcome::Ok {
                    return attempts;
                }
            }
        }

        attempts
    }

    /// Get PATHEXT variants for Windows (e.g., npx -> [npx, npx.cmd, npx.exe, npx.bat]).
    #[cfg(windows)]
    fn pathext_variants(&self, command: &str) -> Vec<String> {
        let mut variants = vec![command.to_string()];

        if let Some(pathext) = self.env.get("PATHEXT") {
            if let Some(pathext_str) = pathext.to_str() {
                for ext in pathext_str.split(';') {
                    if !ext.is_empty() {
                        variants.push(format!("{command}{ext}"));
                    }
                }
            }
        } else {
            // Default Windows executable extensions if PATHEXT not set
            for ext in [".cmd", ".exe", ".bat", ".com"] {
                variants.push(format!("{command}{ext}"));
            }
        }

        variants
    }

    /// Get platform-specific default directories to search.
    fn platform_default_dirs() -> Vec<&'static str> {
        #[cfg(target_os = "macos")]
        {
            vec![
                "/opt/homebrew/bin", // Apple Silicon Homebrew
                "/usr/local/bin",    // Intel Homebrew / manual installs
                "/usr/bin",
                "/bin",
            ]
        }

        #[cfg(target_os = "linux")]
        {
            vec!["/usr/local/bin", "/usr/bin", "/bin"]
        }

        #[cfg(target_os = "windows")]
        {
            // Windows uses PATHEXT and system PATH, less reliance on hardcoded paths
            vec![]
        }

        #[cfg(not(any(target_os = "macos", target_os = "linux", target_os = "windows")))]
        {
            vec!["/usr/local/bin", "/usr/bin", "/bin"]
        }
    }

    /// Get platform-specific PATH separator.
    #[cfg(unix)]
    const fn path_separator() -> char {
        ':'
    }

    #[cfg(windows)]
    const fn path_separator() -> char {
        ';'
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::resolver::env::MockEnv;
    use crate::resolver::fs::MockFs;

    #[test]
    fn test_search_in_path_finds_executable() {
        let env = MockEnv::new().with_var("PATH", "/usr/bin:/usr/local/bin");
        let fs = MockFs::new().with_executable("/usr/local/bin/npx");

        let searcher = ExecutableSearcher::new(&env, &fs);
        let attempts = searcher.search_in_path("npx");

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

        let searcher = ExecutableSearcher::new(&env, &fs);
        let attempts = searcher.search_in_path("npx");

        assert!(attempts.is_empty());
    }
}
