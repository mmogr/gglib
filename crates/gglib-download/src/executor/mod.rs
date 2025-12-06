//! Download execution.
//!
//! This module handles the actual download execution via Python subprocess.
//! All Python-related logic is internal to this module.

mod python_runner;

pub(crate) use python_runner::PythonRunner;

// Placeholder for the executor implementation
// TODO: Move PythonDownloadExecutor from src/download/executor/
