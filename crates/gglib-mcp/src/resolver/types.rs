//! Types for executable path resolution.

use std::path::PathBuf;

/// Result of attempting to resolve a command to an executable path.
#[derive(Debug, Clone)]
pub struct ResolveResult {
    /// The successfully resolved absolute path to the executable.
    pub resolved_path: PathBuf,
    /// All locations that were checked during resolution (for diagnostics).
    pub attempts: Vec<Attempt>,
    /// Non-fatal warnings about the resolution (e.g., "multiple candidates found").
    pub warnings: Vec<String>,
}

/// A single attempt to locate an executable at a candidate path.
#[derive(Debug, Clone)]
pub struct Attempt {
    /// The path that was checked.
    pub candidate: PathBuf,
    /// The outcome of checking this candidate.
    pub outcome: AttemptOutcome,
}

/// Possible outcomes when checking if a candidate path is a valid executable.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AttemptOutcome {
    /// File was found and is executable (success case).
    Ok,
    /// Path does not exist.
    NotFound,
    /// Path exists but is not a file (e.g., directory).
    NotAFile,
    /// File exists but is not executable (permission issue or not marked executable).
    NotExecutable,
    /// Permission denied when checking the path.
    PermissionDenied,
    /// Other I/O error occurred.
    IoError(String),
}

impl std::fmt::Display for AttemptOutcome {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Ok => write!(f, "OK"),
            Self::NotFound => write!(f, "not found"),
            Self::NotAFile => write!(f, "not a file"),
            Self::NotExecutable => write!(f, "not executable"),
            Self::PermissionDenied => write!(f, "permission denied"),
            Self::IoError(msg) => write!(f, "I/O error: {msg}"),
        }
    }
}

/// Error returned when executable resolution fails completely.
#[derive(Debug, thiserror::Error)]
pub enum ResolveError {
    #[error("Command is empty")]
    EmptyCommand,

    #[error("Could not resolve '{command}' to an executable path. Tried:\n{attempts}")]
    NotResolved { command: String, attempts: String },
}

impl ResolveError {
    /// Create a `NotResolved` error with formatted attempt details.
    pub fn not_resolved(command: impl Into<String>, attempts: &[Attempt]) -> Self {
        let command = command.into();
        let attempts_str = attempts
            .iter()
            .map(|a| format!("  ✗ {}: {}", a.candidate.display(), a.outcome))
            .collect::<Vec<_>>()
            .join("\n");

        Self::NotResolved {
            command,
            attempts: if attempts_str.is_empty() {
                "  (no candidates checked)".to_string()
            } else {
                attempts_str
            },
        }
    }
}
