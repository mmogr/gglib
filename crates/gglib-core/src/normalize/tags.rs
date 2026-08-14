//! Format-tag constants used to select a normalization parser.
//!
//! These tags are stored on a [`crate::domain::Model`] and consulted by
//! [`super::registry::dialect_for_tags`] to map legacy catalog rows — and
//! models whose spec could not be derived from their chat template — to a
//! built-in [`crate::domain::dialect::DialectSpec`].
//!
//! Adding a new *builtin* dialect is a two-step process:
//!
//! 1. Add a new `pub const FORMAT_*: &str = "format:..."` here.
//! 2. Add the tag to a match arm in [`super::registry::dialect_for_tags`],
//!    mapping it to a spec.
//!
//! Template-derived dialects need neither: detection persists a
//! `DialectSpec` on the model row and no tag is consulted.
//!
//! No other crate should hard-code these strings — always go through the
//! constants so the registry remains the single source of truth.

/// Qwen-style XML tool calls: `<tool_call>{json}</tool_call>` markup
/// embedded inside `TextDelta` or `ReasoningDelta` content.
///
/// Models tagged with this string emit the legacy Qwen 2/2.5/3 tool-call
/// dialect that pre-dates OpenAI-compatible `tool_calls`.  The
/// [`super::parsers::delimited::DelimitedToolCallParser`], configured with
/// the built-in Qwen [`crate::domain::dialect::DialectSpec`], rewrites these
/// into proper `LlmStreamEvent::ToolCallDelta` events.
pub const FORMAT_QWEN_XML: &str = "format:qwen-xml";

/// Hermes-family `<tool_call>{json}</tool_call>` markup in the text channel.
///
/// Detection has always emitted this tag for non-Qwen models whose chat
/// template carries Hermes-style `<tool_call>` markup, but no parser was
/// wired to it — the markup leaked to clients raw. The dialect is the same
/// envelope-plus-JSON shape as [`FORMAT_QWEN_XML`], so both tags map to the
/// built-in Qwen [`crate::domain::dialect::DialectSpec`] in
/// [`super::registry::dialect_for_tags`].
pub const FORMAT_HERMES: &str = "format:hermes";

/// Bare `<think>...</think>` reasoning tags emitted in the text channel.
///
/// Models tagged with this string emit chain-of-thought reasoning inline in
/// the text channel rather than via the dedicated `reasoning_content` field.
/// V1 reserves this constant for forward compatibility; the corresponding
/// parser is delivered in a follow-up PR.
pub const FORMAT_THINK_TAG: &str = "format:think-tag";
