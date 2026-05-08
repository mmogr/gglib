//! Format-tag constants used to select a normalization parser.
//!
//! These tags are stored on a [`crate::domain::Model`] and consulted by
//! [`super::registry::get_parser`] to decide which dialect-specific parser
//! to instantiate for a given model.
//!
//! Adding a new dialect is a two-step process:
//!
//! 1. Add a new `pub const FORMAT_*: &str = "format:..."` here.
//! 2. Add a single match arm to [`super::registry::get_parser`] mapping the
//!    tag to a parser implementation.
//!
//! No other crate should hard-code these strings — always go through the
//! constants so the registry remains the single source of truth.

/// Qwen-style XML tool calls: `<tool_call>{json}</tool_call>` markup
/// embedded inside `TextDelta` or `ReasoningDelta` content.
///
/// Models tagged with this string emit the legacy Qwen 2/2.5/3 tool-call
/// dialect that pre-dates OpenAI-compatible `tool_calls`.  The
/// [`super::parsers::qwen_xml::QwenXmlParser`] rewrites these into proper
/// `LlmStreamEvent::ToolCallDelta` events.
pub const FORMAT_QWEN_XML: &str = "format:qwen-xml";

/// Bare `<think>...</think>` reasoning tags emitted in the text channel.
///
/// Models tagged with this string emit chain-of-thought reasoning inline in
/// the text channel rather than via the dedicated `reasoning_content` field.
/// V1 reserves this constant for forward compatibility; the corresponding
/// parser is delivered in a follow-up PR.
pub const FORMAT_THINK_TAG: &str = "format:think-tag";
