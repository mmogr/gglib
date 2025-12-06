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
pub use context::{resolve_context_size, ContextResolution, ContextResolutionSource};
pub use jinja::{resolve_jinja_flag, JinjaResolution, JinjaResolutionSource};
pub use reasoning::{
    resolve_reasoning_format, resolve_reasoning_format_with_metadata, ReasoningFormatResolution,
    ReasoningFormatSource,
};
