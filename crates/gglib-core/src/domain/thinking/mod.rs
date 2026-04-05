//! Thinking/reasoning content parsing and streaming accumulation.
//!
//! Reasoning models (`DeepSeek R1`, `Qwen3`, etc.) output a "thinking" phase that
//! can appear either as a structured `reasoning_content` SSE field or as inline
//! `<think>…</think>` tags in the message content.
//!
//! This module is the **single source of truth** for parsing those tags across
//! all surfaces (CLI, Axum, Tauri).  It supports four tag families:
//!
//! | Format | Models |
//! |---|---|
//! | `<think>…</think>` | `DeepSeek R1`, `Qwen3`, most reasoning models |
//! | `<reasoning>…</reasoning>` | Alternative / custom models |
//! | `<seed:think>…</seed:think>` | `Seed-OSS` models |
//! | `<\|START_THINKING\|>…<\|END_THINKING\|>` | `Command-R7B` style |
//!
//! # Modules
//!
//! | Module | Contents |
//! |--------|----------|
//! | [`types`] | [`ParsedThinkingContent`], [`ThinkingEvent`] |
//! | [`normalize`] | [`normalize_thinking_tags`] — variant-tag normalisation |
//! | [`parse`] | [`parse_thinking_content`], [`embed_thinking_content`], [`has_thinking_content`], [`format_thinking_duration`] |
//! | [`accumulator`] | [`ThinkingAccumulator`] — streaming FSM for split-tag detection |

pub mod accumulator;
pub mod normalize;
pub mod parse;
pub mod types;

#[cfg(test)]
mod accumulator_tests;
#[cfg(test)]
mod parse_tests;

// Re-export public API at module level for ergonomic imports.
pub use accumulator::ThinkingAccumulator;
pub use normalize::normalize_thinking_tags;
pub use parse::{
    embed_thinking_content, format_thinking_duration, has_thinking_content, parse_thinking_content,
};
pub use types::{ParsedThinkingContent, ThinkingEvent};
