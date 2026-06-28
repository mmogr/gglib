#![doc = include_str!("README.md")]
mod empty;
mod filtered;

#[cfg(test)]
mod tests;

pub use empty::EmptyToolExecutor;
pub use filtered::FilteredToolExecutor;

/// Sentinel phrase embedded in every tool-rejection error produced by this module.
///
/// Both [`EmptyToolExecutor`] and [`FilteredToolExecutor`] use this constant so
/// tests can assert on `error_string.contains(TOOL_NOT_AVAILABLE_MSG)` without
/// depending on the surrounding format string.
pub const TOOL_NOT_AVAILABLE_MSG: &str = "is not available in this session";
