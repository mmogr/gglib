//! Path-related error types.
//!
//! Provides semantic errors for path operations without exposing
//! implementation details or adapter-specific concerns.

use std::path::PathBuf;
use thiserror::Error;

/// Errors that can occur during path resolution and directory operations.
#[derive(Debug, Error)]
pub enum PathError {
    /// Could not determine the user's home directory.
    #[error("Cannot determine home directory")]
    NoHomeDir,

    /// Could not determine the system data directory.
    #[error("Cannot determine system data directory")]
    NoDataDir,

    /// A path was expected to be a directory but was not.
    #[error("{0} exists but is not a directory")]
    NotADirectory(PathBuf),

    /// A directory does not exist and creation was not allowed.
    #[error("Directory {0} does not exist")]
    DirectoryNotFound(PathBuf),

    /// Failed to create a directory.
    #[error("Failed to create directory {path}: {reason}")]
    CreateFailed { path: PathBuf, reason: String },

    /// A directory is not writable.
    #[error("Directory {path} is not writable: {reason}")]
    NotWritable { path: PathBuf, reason: String },

    /// An empty path was provided.
    #[error("Path cannot be empty")]
    EmptyPath,

    /// Failed to read or write the environment file.
    #[error("Failed to access env file {path}: {reason}")]
    EnvFileError { path: PathBuf, reason: String },

    /// Failed to get the current working directory.
    #[error("Cannot determine current directory: {0}")]
    CurrentDirError(String),
}
