#![doc = include_str!("README.md")]

#![doc = include_str!("README.md")]

// MIGRATION: content extracted to README.md — remove this //! block after review
// MIGRATION: content extracted to README.md — remove this //! block after review
//! Server-Sent Events (SSE) codec for `OpenAI`-compatible chat completion
//! streams.
//!
//! This module is the **single source of truth** for translating between
//! the `OpenAI` `chat.completion.chunk` SSE wire format and the typed
//! [`crate::LlmStreamEvent`] domain values.  It contains three pieces:
//!
//! | Submodule | Role |
//! |-----------|------|
//! | [`parser`] | Parse one `data:` JSON payload → typed events |
//! | [`decoder`] | Stateful byte-stream → events (line buffering, `[DONE]`) |
//! | [`encoder`] | Typed event → `data:` JSON payload (for re-emission) |
//!
//! Promoting the codec to `gglib-core` lets every adapter (runtime, proxy,
//! future GUIs) share a single, well-tested implementation rather than
//! re-rolling SSE parsing per surface.

pub mod decoder;
pub mod encoder;
pub mod parser;

pub use decoder::SseStreamDecoder;
pub use encoder::SseEncoder;
pub use parser::{SseParseResult, parse_sse_frame};
