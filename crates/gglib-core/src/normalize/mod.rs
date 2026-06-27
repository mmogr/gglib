#![doc = include_str!("README.md")]

#![doc = include_str!("README.md")]

// MIGRATION: content extracted to README.md — remove this //! block after review
// MIGRATION: content extracted to README.md — remove this //! block after review
//! Universal local-LLM consistency layer.
//!
//! This module rewrites model-specific output dialects into the strict
//! OpenAI-shaped [`crate::domain::agent::LlmStreamEvent`] sequence that the
//! rest of the codebase expects.  Adapters wrap the LLM stream once at the
//! port boundary; every downstream surface (Axum, CLI, Tauri, proxy)
//! consumes the canonical form.
//!
//! ## Module map
//!
//! - [`tags`] — `format:*` constants used to pick a parser.
//! - [`error`] — non-fatal [`error::NormalizationError`] surfaced from parsers.
//! - [`parser`] — the [`parser::ToolCallParser`] trait + [`parser::ParserOutput`].
//! - [`parsers`] — concrete parser implementations, one file per dialect.
//! - [`registry`] — the single dispatch site that maps tags to parsers.
//!
//! ## Adding a new dialect
//!
//! 1. Add a `pub const FORMAT_*` to [`tags`].
//! 2. Drop a new file under [`parsers`].
//! 3. Add **one** match arm to [`registry::get_parser`].
//!
//! The registry is the only place that knows the full set of parsers, by
//! design — see the module docs there.

pub mod error;
pub mod history;
pub mod parser;
pub mod parsers;
pub mod registry;
pub mod stream;
pub mod tags;

pub use error::{NormalizationError, NormalizationErrorKind};
pub use history::strip_thinking_debt;
pub use parser::{ParserOutput, ToolCallParser};
pub use registry::get_parser;
pub use stream::NormalizingStream;
