#![doc = include_str!("README.md")]

mod cell;
mod executor;
mod reader;
mod registry;

pub use registry::RunRegistry;

use std::sync::Arc;

/// Milliseconds since the Unix epoch. An argument, so tests drive time.
pub(crate) type Clock = Arc<dyn Fn() -> u64 + Send + Sync>;

#[cfg(test)]
#[path = "limits_tests.rs"]
mod limits_tests;
#[cfg(test)]
#[path = "registry_tests.rs"]
mod registry_tests;
#[cfg(test)]
#[path = "retention_tests.rs"]
mod retention_tests;
#[cfg(test)]
#[path = "scope_tests.rs"]
mod scope_tests;
#[cfg(test)]
mod test_executor;
