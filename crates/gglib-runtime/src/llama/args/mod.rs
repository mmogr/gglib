#![doc = include_str!("README.md")]
// MIGRATION: content extracted to README.md — remove this //! block after review
//! Shared helpers for building llama.cpp invocations.
//!
//! This module hosts reusable utilities for resolving CLI flags and
//! configuration options so that multiple commands can stay DRY.

pub mod context;
pub mod jinja;
pub mod mtp;
pub mod reasoning;

// Re-export public API
pub use context::{ContextInput, ContextResolution, ContextResolutionSource, resolve_context_size};
pub use jinja::{JinjaResolution, JinjaResolutionSource, resolve_jinja_flag};
pub use mtp::{
    DEFAULT_DRAFT_N_MAX, DEFAULT_DRAFT_P_MIN, MtpResolution, MtpResolutionSource, resolve_mtp_args,
};
pub use reasoning::{
    ReasoningDetection, ReasoningFormatResolution, ReasoningFormatSource, resolve_reasoning_format,
    resolve_reasoning_format_with_detection,
};
