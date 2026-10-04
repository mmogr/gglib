//! [`AgentMessage`] — A single message in the agent conversation.
//!
//! This module contains pure domain structs and enums.  All custom
//! [`Serialize`] / [`Deserialize`] implementations live in the sibling
//! [`super::messages_serde`] module to keep domain types free of
//! serialisation noise.

use serde::{Deserialize, Serialize};

use super::tool_types::ToolCall;
use crate::domain::attachment::AttachmentId;
use crate::request_pipeline::{CHARS_PER_TOKEN_APPROX, MAX_IMAGE_TOKENS};

/// What one image is charged against the context budget, in characters: the
/// cap on an image's tokens, at the ratio the budget is measured in.
pub const IMAGE_CHARGE_CHARS: usize = MAX_IMAGE_TOKENS * CHARS_PER_TOKEN_APPROX;

/// Content carried by an [`AgentMessage::Assistant`] turn.
///
/// A flat struct with optional `text` and a (possibly empty) `tool_calls` vec.
/// At the wire level, at least one of the two fields must be present — the
/// hand-rolled [`Deserialize`] impl (in [`super::messages_serde`]) enforces
/// this.
///
/// # Serde
///
/// Serializes/deserializes as a flat map so it can be `#[serde(flatten)]`-ed
/// directly into the parent [`AgentMessage`] object:
///
/// | State | JSON fields |
/// |-------|-------------|
/// | text only | `"content": "..."` |
/// | tool calls only | `"tool_calls": [...]` |
/// | both | `"content": "...", "tool_calls": [...]` |
///
/// Custom `Serialize` and `Deserialize` impls are in
/// [`super::messages_serde`].
#[derive(Debug, Clone)]
#[cfg_attr(feature = "ts-bindings", derive(ts_rs::TS), ts(export))]
pub struct AssistantContent {
    /// Optional text content from the model.  `None` when the model produced
    /// only tool calls with no text preamble.
    ///
    /// Both annotations below restate what [`super::messages_serde`]'s
    /// hand-written impls do, because ts-rs reads *fields* and cannot see a
    /// manual `Serialize`. Without them the binding claims a `text` key that
    /// no payload has ever carried. The table above is the contract.
    #[cfg_attr(feature = "ts-bindings", ts(rename = "content", optional))]
    pub text: Option<String>,
    /// Tool calls requested by the model.  Empty when the model produced a
    /// text-only response (final answer).
    ///
    /// `as Option<…>` because the impl omits the key entirely for an empty
    /// vec, which is a state a bare `Vec` cannot express in TypeScript.
    #[cfg_attr(feature = "ts-bindings", ts(as = "Option<Vec<ToolCall>>", optional))]
    pub tool_calls: Vec<ToolCall>,
}

impl AssistantContent {
    /// Consume `self` and return a new value with `calls` as the tool-call
    /// list, preserving any existing text content.
    #[must_use]
    pub fn with_replaced_tool_calls(self, calls: Vec<ToolCall>) -> Self {
        Self {
            tool_calls: calls,
            ..self
        }
    }
}

/// A single message in the agent conversation.
///
/// The closed enum prevents invalid states that a flat struct with `role: String`
/// would allow (e.g. a `User` message carrying `tool_calls`, or a `Tool` message
/// without a `tool_call_id`).
///
/// # Wire format
///
/// `#[serde(tag = "role", rename_all = "lowercase")]` produces JSON identical to
/// the TypeScript `ChatMessage` interface in the frontend:
///
/// ```json
/// { "role": "user", "content": "What files are in the project?" }
/// { "role": "assistant", "content": null, "tool_calls": [...] }
/// { "role": "tool", "tool_call_id": "call_abc", "content": "src/\nlib/" }
/// ```
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "role", rename_all = "lowercase")]
#[cfg_attr(feature = "ts-bindings", derive(ts_rs::TS), ts(export))]
pub enum AgentMessage {
    /// A system-level instruction that sets the model's persona and constraints.
    System {
        /// Instruction text.
        content: String,
    },

    /// A message from the human user.
    User {
        /// Message text.
        content: String,
        /// The images the message carries, by id, in order. Left out of the
        /// JSON when there are none.
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        #[cfg_attr(feature = "ts-bindings", ts(type = "Array<string>", optional))]
        images: Vec<AttachmentId>,
    },

    /// A response from the assistant model.
    ///
    /// `content` always carries either text, tool calls, or both — a vacuous
    /// all-`None` state, which an `Option<String>` + `Option<Vec<ToolCall>>`
    /// pair would allow, is impossible to construct.
    Assistant {
        /// Content of the assistant turn.
        #[serde(flatten)]
        content: AssistantContent,
    },

    /// The result of a tool call, to be sent back to the model.
    Tool {
        /// Must match the [`ToolCall::id`] from the preceding `Assistant` message.
        tool_call_id: String,

        /// Serialised output of the tool (or error description if it failed).
        content: String,
    },
}

impl AgentMessage {
    /// A user message of text alone.
    #[must_use]
    pub fn user(content: impl Into<String>) -> Self {
        Self::User {
            content: content.into(),
            images: Vec::new(),
        }
    }

    /// Whether the message carries an image.
    #[must_use]
    pub fn has_images(&self) -> bool {
        matches!(self, Self::User { images, .. } if !images.is_empty())
    }

    /// Estimate the Unicode scalar-value count of this message.
    ///
    /// An image a user message carries is charged [`IMAGE_CHARGE_CHARS`]:
    /// the most an image can cost, with no look at its size.
    ///
    /// Uses `str::chars().count()` rather than [`str::len`] (byte count) so
    /// that multi-byte characters are counted as one unit, matching how LLMs
    /// typically measure context length.
    ///
    /// # Performance
    ///
    /// This is an **O(n)** scan — it iterates over every Unicode scalar value
    /// in every `str` field of the message. Avoid calling it inside tight or
    /// nested loops. For repeated measurements over the same message set,
    /// accumulate the total once and update it incrementally (the agent loop
    /// does exactly this via its `running_chars` counter).
    pub fn char_count(&self) -> usize {
        match self {
            Self::System { content } => content.chars().count(),
            Self::User { content, images } => {
                content.chars().count() + images.len() * IMAGE_CHARGE_CHARS
            }
            Self::Assistant { content } => {
                content.text.as_ref().map_or(0, |s| s.chars().count())
                    + content
                        .tool_calls
                        .iter()
                        .map(|c| {
                            // Include `id` so the context-budget estimate
                            // matches what llama-server actually tokenises
                            // (a typical id like "call_abc123" is ~15 chars).
                            c.id.chars().count()
                                + c.name.chars().count()
                                + c.arguments.to_string().chars().count()
                        })
                        .sum::<usize>()
            }
            Self::Tool {
                tool_call_id,
                content,
            } => tool_call_id.chars().count() + content.chars().count(),
        }
    }
}

#[cfg(test)]
#[path = "messages_tests.rs"]
mod messages_tests;
