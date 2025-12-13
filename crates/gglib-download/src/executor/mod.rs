//! Download execution.
//!
//! This module handles the actual download execution via Python subprocess.
//! All Python-related logic is internal to this module.

// TODO(#221): Remove after Phase 2.5 completes
#![allow(dead_code, unused_imports)]

mod python_runner;

pub use python_runner::PythonRunner;

// Placeholder for the executor implementation
// TODO: Move PythonDownloadExecutor from src/download/executor/
