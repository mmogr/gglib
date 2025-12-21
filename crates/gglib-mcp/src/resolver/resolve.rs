//! Main executable path resolution logic.

use super::env::{EnvProvider, SystemEnv};
use super::fs::{FsProvider, SystemFs};
use super::search::ExecutableSearcher;
use super::types::{Attempt, AttemptOutcome, ResolveError, ResolveResult};
use std::path::Path;

/// Resolve a command to an absolute executable path.
///
/// Search order:
/// 1. If command is absolute and valid → return it
/// 2. Search in PATH environment variable
/// 3. Search in /etc/paths and /etc/paths.d/* (macOS only)
/// 4. Search in platform-specific default directories
/// 5. Search in Node.js version manager shims (for npm/npx/node)
/// 6. Search in user-provided additional paths
pub fn resolve_executable(
    command: &str,
    user_search_paths: &[String],
) -> Result<ResolveResult, ResolveError> {
    resolve_executable_with_deps(command, user_search_paths, &SystemEnv, &SystemFs)
}

/// Resolve with injected dependencies (for testing).
pub fn resolve_executable_with_deps(
    command: &str,
    user_search_paths: &[String],
    env: &dyn EnvProvider,
    fs: &dyn FsProvider,
) -> Result<ResolveResult, ResolveError> {
    if command.is_empty() {
        return Err(ResolveError::EmptyCommand);
    }

    let mut all_attempts = Vec::new();
    let mut warnings = Vec::new();

    // Step 1: If command is absolute, validate and return
    let command_path = Path::new(command);
    if command_path.is_absolute() {
        let outcome = fs.check_executable(command_path);
        all_attempts.push(Attempt {
            candidate: command_path.to_path_buf(),
            outcome: outcome.clone(),
        });

        if outcome == AttemptOutcome::Ok {
            return Ok(ResolveResult {
                resolved_path: command_path.to_path_buf(),
                attempts: all_attempts,
                warnings,
            });
        }

        // Absolute path failed - try falling back to basename
        if let Some(basename) = command_path.file_name().and_then(|n| n.to_str()) {
            warnings.push(format!(
                "Absolute path '{}' failed ({}), falling back to basename '{}'",
                command, outcome, basename
            ));
            return resolve_relative_command(
                basename,
                user_search_paths,
                env,
                fs,
                all_attempts,
                warnings,
            );
        }

        // Can't extract basename, fail with the absolute path error
        return Err(ResolveError::not_resolved(command, &all_attempts));
    }

    // Step 2-6: Search for relative command
    resolve_relative_command(command, user_search_paths, env, fs, all_attempts, warnings)
}

/// Resolve a relative command (not an absolute path).
fn resolve_relative_command(
    command: &str,
    user_search_paths: &[String],
    env: &dyn EnvProvider,
    fs: &dyn FsProvider,
    mut all_attempts: Vec<Attempt>,
    warnings: Vec<String>,
) -> Result<ResolveResult, ResolveError> {
    let searcher = ExecutableSearcher::new(env, fs);

    // Step 2: Search in PATH
    let path_attempts = searcher.search_in_path(command);
    if let Some(success) = find_success(&path_attempts) {
        let resolved_path = success.candidate.clone();
        all_attempts.extend(path_attempts);
        return Ok(ResolveResult {
            resolved_path,
            attempts: all_attempts,
            warnings,
        });
    }
    all_attempts.extend(path_attempts);

    // Step 3: Search /etc/paths (macOS)
    let etc_attempts = searcher.search_in_etc_paths(command);
    if let Some(success) = find_success(&etc_attempts) {
        let resolved_path = success.candidate.clone();
        all_attempts.extend(etc_attempts);
        return Ok(ResolveResult {
            resolved_path,
            attempts: all_attempts,
            warnings,
        });
    }
    all_attempts.extend(etc_attempts);

    // Step 4: Search platform defaults
    let platform_attempts = searcher.search_platform_defaults(command);
    if let Some(success) = find_success(&platform_attempts) {
        let resolved_path = success.candidate.clone();
        all_attempts.extend(platform_attempts);
        return Ok(ResolveResult {
            resolved_path,
            attempts: all_attempts,
            warnings,
        });
    }
    all_attempts.extend(platform_attempts);

    // Step 5: Search Node.js version managers
    let node_attempts = searcher.search_node_managers(command);
    if let Some(success) = find_success(&node_attempts) {
        let resolved_path = success.candidate.clone();
        all_attempts.extend(node_attempts);
        return Ok(ResolveResult {
            resolved_path,
            attempts: all_attempts,
            warnings,
        });
    }
    all_attempts.extend(node_attempts);

    // Step 6: Search user-provided paths
    let user_attempts = searcher.search_user_paths(command, user_search_paths);
    if let Some(success) = find_success(&user_attempts) {
        let resolved_path = success.candidate.clone();
        all_attempts.extend(user_attempts);
        return Ok(ResolveResult {
            resolved_path,
            attempts: all_attempts,
            warnings,
        });
    }
    all_attempts.extend(user_attempts);

    // Nothing found
    Err(ResolveError::not_resolved(command, &all_attempts))
}

/// Find the first successful attempt in a list.
fn find_success(attempts: &[Attempt]) -> Option<&Attempt> {
    attempts.iter().find(|a| a.outcome == AttemptOutcome::Ok)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::resolver::env::MockEnv;
    use crate::resolver::fs::MockFs;
    use std::path::PathBuf;

    #[test]
    fn test_resolve_absolute_path_success() {
        let env = MockEnv::new();
        let fs = MockFs::new().with_executable("/usr/local/bin/npx");

        let result = resolve_executable_with_deps("/usr/local/bin/npx", &[], &env, &fs);

        assert!(result.is_ok());
        let resolved = result.unwrap();
        assert_eq!(resolved.resolved_path, PathBuf::from("/usr/local/bin/npx"));
    }

    #[test]
    fn test_resolve_absolute_path_failure_falls_back() {
        let env = MockEnv::new().with_var("PATH", "/opt/homebrew/bin");
        let fs = MockFs::new().with_executable("/opt/homebrew/bin/npx");

        // Try absolute path that doesn't exist, should fall back to basename search
        let result = resolve_executable_with_deps("/usr/local/bin/npx", &[], &env, &fs);

        assert!(result.is_ok());
        let resolved = result.unwrap();
        assert_eq!(
            resolved.resolved_path,
            PathBuf::from("/opt/homebrew/bin/npx")
        );
        assert!(!resolved.warnings.is_empty()); // Should warn about fallback
    }

    #[test]
    fn test_resolve_in_path() {
        let env = MockEnv::new().with_var("PATH", "/usr/bin:/usr/local/bin");
        let fs = MockFs::new().with_executable("/usr/local/bin/npx");

        let result = resolve_executable_with_deps("npx", &[], &env, &fs);

        assert!(result.is_ok());
        let resolved = result.unwrap();
        assert_eq!(resolved.resolved_path, PathBuf::from("/usr/local/bin/npx"));
    }

    #[test]
    fn test_resolve_empty_command() {
        let env = MockEnv::new();
        let fs = MockFs::new();

        let result = resolve_executable_with_deps("", &[], &env, &fs);

        assert!(result.is_err());
        assert!(matches!(result.unwrap_err(), ResolveError::EmptyCommand));
    }

    #[test]
    fn test_resolve_not_found() {
        let env = MockEnv::new().with_var("PATH", "/usr/bin");
        let fs = MockFs::new();

        let result = resolve_executable_with_deps("nonexistent", &[], &env, &fs);

        assert!(result.is_err());
        if let Err(ResolveError::NotResolved { command, attempts }) = result {
            assert_eq!(command, "nonexistent");
            assert!(!attempts.is_empty());
        } else {
            panic!("Expected NotResolved error");
        }
    }

    #[test]
    fn test_resolve_with_user_paths() {
        let env = MockEnv::new();
        let fs = MockFs::new().with_executable("/custom/bin/npx");
        let user_paths = vec!["/custom/bin".to_string()];

        let result = resolve_executable_with_deps("npx", &user_paths, &env, &fs);

        assert!(result.is_ok());
        let resolved = result.unwrap();
        assert_eq!(resolved.resolved_path, PathBuf::from("/custom/bin/npx"));
    }
}
