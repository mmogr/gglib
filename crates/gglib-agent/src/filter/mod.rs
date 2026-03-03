//! [`FilteredToolExecutor`] and [`EmptyToolExecutor`] — decorators that
//! restrict a [`ToolExecutorPort`] to a named allowlist of tools.
//!
//! # Architectural placement
//!
//! These are *concrete implementations* (decorators), not pure ports or domain
//! types, so they live here in `gglib-agent` (the orchestration layer) rather
//! than in `gglib-core` (which contains only traits and domain models).
//! Both downstream consumers — the Axum HTTP handler (`gglib-axum`) and the
//! CLI agent handler (`gglib-cli`) — already depend on `gglib-agent`, so
//! keeping the decorators here is DRY with zero extra dependency edges.
//!
//! # Security model
//!
//! The allowlist is enforced on **both** `list_tools` (so the LLM only sees
//! permitted tools) and `execute` (so an adversarially-prompted model that
//! synthesises a call for a tool it was never told about cannot bypass the
//! filter).

mod empty;
mod filtered;

#[cfg(test)]
mod tests;

pub(crate) use empty::EmptyToolExecutor;
pub(crate) use filtered::FilteredToolExecutor;

/// Sentinel phrase embedded in every tool-rejection error produced by this
/// module.  Both [`EmptyToolExecutor`] and [`FilteredToolExecutor`] use this
/// constant so tests can assert on `error_string.contains(TOOL_NOT_AVAILABLE_MSG)`
/// without depending on the surrounding format string.
pub(crate) const TOOL_NOT_AVAILABLE_MSG: &str = "is not available in this session";
