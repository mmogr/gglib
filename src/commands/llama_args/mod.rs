//! Shared helpers for building llama.cpp invocations.
//!
//! This module hosts reusable utilities for resolving CLI flags and
//! configuration options so that multiple commands can stay DRY.

pub mod context;
pub mod jinja;
pub mod reasoning;

#[cfg(test)]
mod tests;

// Re-export public API to maintain backward compatibility
pub use context::{ContextResolution, ContextResolutionSource, resolve_context_size};
pub use jinja::{JinjaResolution, JinjaResolutionSource, resolve_jinja_flag};
pub use reasoning::{
    ReasoningFormatResolution, ReasoningFormatSource, resolve_reasoning_format,
    resolve_reasoning_format_with_metadata,
};
