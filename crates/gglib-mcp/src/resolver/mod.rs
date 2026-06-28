#![doc = include_str!("README.md")]
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
