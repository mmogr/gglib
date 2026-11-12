//! The `format:*` tags model detection stores on a [`crate::domain::Model`]:
//! how the model's chat template writes tool calls or reasoning.
//!
//! No other crate should hard-code these strings — always go through the
//! constants.

/// Qwen-style XML tool calls: `<tool_call>{json}</tool_call>` markup
/// embedded inside `TextDelta` or `ReasoningDelta` content.
pub const FORMAT_QWEN_XML: &str = "format:qwen-xml";

/// Hermes-family `<tool_call>{json}</tool_call>` markup in the text channel.
pub const FORMAT_HERMES: &str = "format:hermes";

/// Bare `<think>...</think>` reasoning tags emitted in the text channel.
///
/// Models tagged with this string emit chain-of-thought reasoning inline in
/// the text channel rather than via the dedicated `reasoning_content` field.
/// V1 reserves this constant for forward compatibility; the corresponding
/// parser is delivered in a follow-up PR.
pub const FORMAT_THINK_TAG: &str = "format:think-tag";
