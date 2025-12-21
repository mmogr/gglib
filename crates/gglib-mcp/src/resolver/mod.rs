//! Executable path resolution for MCP server commands.
//!
//! This module provides a robust, testable way to resolve command names
//! (like "npx") to absolute executable paths across different platforms
//! and installation methods.
//!
//! ## Architecture
//!
//! The resolver is split into small, focused modules:
//! - `types`: Core types (`ResolveResult`, `Attempt`, `AttemptOutcome`)
//! - `env`: Environment variable access trait (injectable for testing)
//! - `fs`: Filesystem operations trait (injectable for testing)
//! - `search`: Platform-specific search strategies
//! - `resolve`: Main resolution logic and orchestration
//!
//! ## Usage
//!
//! ```rust,no_run
//! use gglib_mcp::resolver::resolve_executable;
//!
//! // Resolve "npx" to absolute path
//! let result = resolve_executable("npx", &[]).unwrap();
//! println!("Resolved to: {}", result.resolved_path.display());
//!
//! // Show diagnostic info
//! for attempt in &result.attempts {
//!     println!("  {} - {}", attempt.candidate.display(), attempt.outcome);
//! }
//! ```

mod env;
mod fs;
mod resolve;
mod search;
mod types;

pub use env::{EnvProvider, SystemEnv};
pub use fs::{FsProvider, SystemFs};
pub use resolve::{resolve_executable, resolve_executable_with_deps};
pub use types::{Attempt, AttemptOutcome, ResolveError, ResolveResult};

#[cfg(test)]
pub use env::MockEnv;
#[cfg(test)]
pub use fs::MockFs;
