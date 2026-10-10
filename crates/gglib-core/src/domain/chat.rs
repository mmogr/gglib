//! Chat domain types.
//!
//! These types represent chat conversations and messages in the domain model,
//! independent of any infrastructure concerns.
//!
//! [`ConversationSettings`] captures CLI/GUI session parameters (sampling,
//! context, tools) so conversations can be faithfully resumed.

use serde::{Deserialize, Serialize};

use super::agent::messages::AgentMessage;
use super::agent::messages::AssistantContent;
use super::agent::tool_types::ToolCall;
use super::attachment::{AttachmentId, AttachmentInfo};
use super::machine::{Machine, ModelRef};
use super::thinking::Thinking;

/// A chat conversation.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "ts-bindings", derive(ts_rs::TS), ts(export))]
pub struct Conversation {
    #[cfg_attr(feature = "ts-bindings", ts(type = "number"))]
    pub id: i64,
    pub title: String,
    #[cfg_attr(feature = "ts-bindings", ts(type = "number | null"))]
    pub model_id: Option<i64>,
    pub system_prompt: Option<String>,
    /// Session parameters captured at creation for resume.
    #[cfg_attr(feature = "ts-bindings", ts(optional))]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub settings: Option<ConversationSettings>,
    pub created_at: String,
    pub updated_at: String,
    /// The chat this one was branched from (ADR 0017). Absent for a chat
    /// that is no branch; it still names a chat that has since been deleted.
    #[cfg_attr(feature = "ts-bindings", ts(type = "number", optional))]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub branch_of: Option<i64>,
    /// The first chat of its family, the chats branched from one another;
    /// `None` for that chat itself. Kept on this machine.
    #[cfg_attr(feature = "ts-bindings", ts(skip))]
    #[serde(skip)]
    pub lineage_id: Option<i64>,
}

impl Conversation {
    /// The id of its family: the first chat's, of the chats branched from
    /// one another.
    #[must_use]
    pub fn family(&self) -> i64 {
        self.lineage_id.unwrap_or(self.id)
    }

    /// The machine the conversation ran on: its stored model's, or this one
    /// for a row that stores only a `model_id`, which is this catalogue's.
    /// `None` for a conversation that stores neither.
    #[must_use]
    pub fn machine(&self) -> Option<Machine> {
        let stored = self.settings.as_ref().and_then(|s| s.model.as_ref());
        stored
            .map(|model| model.machine.clone())
            .or_else(|| self.model_id.map(|_| Machine::Local))
    }
}

/// A chat message within a conversation.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "ts-bindings", derive(ts_rs::TS), ts(export))]
pub struct Message {
    #[cfg_attr(feature = "ts-bindings", ts(type = "number"))]
    pub id: i64,
    #[cfg_attr(feature = "ts-bindings", ts(type = "number"))]
    pub conversation_id: i64,
    pub role: MessageRole,
    pub content: String,
    pub created_at: String,
    /// Optional JSON metadata for tool usage, etc.
    #[cfg_attr(
        feature = "ts-bindings",
        ts(type = "Record<string, unknown>", optional)
    )]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub metadata: Option<serde_json::Value>,
    /// The images the message carries, in order, without their bytes. Left
    /// out of the JSON when there are none.
    #[cfg_attr(
        feature = "ts-bindings",
        ts(as = "Option<Vec<AttachmentInfo>>", optional)
    )]
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub images: Vec<AttachmentInfo>,
    /// The message this one is a copy of, as it was first written, when a
    /// branch copied it (ADR 0017); `None` for a message written here. Kept
    /// on this machine.
    #[cfg_attr(feature = "ts-bindings", ts(skip))]
    #[serde(skip)]
    pub origin_id: Option<i64>,
}

impl Message {
    /// The id of the message as first written: two messages of a family
    /// with the same key are the same message, and the chats that hold them
    /// are the same chat up to there.
    #[must_use]
    pub fn key(&self) -> i64 {
        self.origin_id.unwrap_or(self.id)
    }

    /// Convert a persisted message back into an [`AgentMessage`] for resume.
    ///
    /// Tool call metadata is faithfully restored from the JSON `"tool_calls"` key
    /// (assistant messages) or `"tool_call_id"` key (tool messages). A user
    /// message keeps its images, by id.
    #[must_use]
    pub fn to_agent_message(&self) -> AgentMessage {
        match self.role {
            MessageRole::System => AgentMessage::System {
                content: self.content.clone(),
            },
            MessageRole::User => AgentMessage::User {
                content: self.content.clone(),
                images: self.images.iter().map(|image| image.id.clone()).collect(),
            },
            MessageRole::Assistant => {
                let tool_calls: Vec<ToolCall> = self
                    .metadata
                    .as_ref()
                    .and_then(|m| m.get("tool_calls"))
                    .and_then(|v| serde_json::from_value(v.clone()).ok())
                    .unwrap_or_default();
                AgentMessage::Assistant {
                    content: AssistantContent {
                        text: if self.content.is_empty() {
                            None
                        } else {
                            Some(self.content.clone())
                        },
                        tool_calls,
                    },
                }
            }
            MessageRole::Tool => {
                let tool_call_id = self
                    .metadata
                    .as_ref()
                    .and_then(|m| m.get("tool_call_id"))
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_string();
                AgentMessage::Tool {
                    tool_call_id,
                    content: self.content.clone(),
                }
            }
        }
    }
}

/// The role of a message sender.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
#[cfg_attr(feature = "ts-bindings", derive(ts_rs::TS), ts(export))]
pub enum MessageRole {
    System,
    User,
    Assistant,
    Tool,
}

impl MessageRole {
    /// Parse a role from a string.
    #[must_use]
    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "system" => Some(Self::System),
            "user" => Some(Self::User),
            "assistant" => Some(Self::Assistant),
            "tool" => Some(Self::Tool),
            _ => None,
        }
    }

    /// Convert role to string representation.
    #[must_use]
    pub const fn as_str(&self) -> &'static str {
        match self {
            Self::System => "system",
            Self::User => "user",
            Self::Assistant => "assistant",
            Self::Tool => "tool",
        }
    }
}

impl std::fmt::Display for MessageRole {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.as_str())
    }
}

/// Data for creating a new conversation.
#[derive(Debug, Clone, Default)]
pub struct NewConversation {
    pub title: String,
    /// The model it is for, of this machine's. Settings that name a model
    /// decide it instead, when the conversation is made.
    pub model_id: Option<i64>,
    pub system_prompt: Option<String>,
    /// Session parameters to persist for resume.
    pub settings: Option<ConversationSettings>,
}

/// Data for creating a new message.
#[derive(Debug, Clone)]
pub struct NewMessage {
    pub conversation_id: i64,
    pub role: MessageRole,
    pub content: String,
    /// Optional JSON metadata for tool usage, etc.
    pub metadata: Option<serde_json::Value>,
    /// The images the message carries, by id, in order. Each must be in the
    /// attachment store when the message is saved.
    pub images: Vec<AttachmentId>,
}

/// Data for updating an existing conversation.
#[derive(Debug, Clone, Default)]
pub struct ConversationUpdate {
    pub title: Option<String>,
    /// Use `Some(Some(prompt))` to set, `Some(None)` to clear, `None` to leave unchanged.
    pub system_prompt: Option<Option<String>>,
    /// Use `Some(Some(settings))` to set, `Some(None)` to clear, `None` to leave unchanged.
    pub settings: Option<Option<ConversationSettings>>,
    /// Use `Some(Some(id))` to set, `Some(None)` to clear, `None` to leave unchanged.
    pub model_id: Option<Option<i64>>,
}

/// Session parameters captured at conversation creation for resume.
///
/// Stores sampling, context, and tool configuration so a CLI or GUI session
/// can be faithfully restored. Serialized as a JSON column in the database.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
#[cfg_attr(feature = "ts-bindings", derive(ts_rs::TS), ts(export))]
pub struct ConversationSettings {
    /// The name the session's model goes by, as it is shown. Resolved again
    /// on resume only in a row that stores no `model`.
    #[cfg_attr(feature = "ts-bindings", ts(optional))]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub model_name: Option<String>,
    /// The session's model, named by its machine. A conversation resumes on
    /// that machine, by that id. A run on another of this machine's models,
    /// or a CLI resume that names another model, replaces it. Absent in rows
    /// saved before it was recorded, and for a model the catalogue did not
    /// hold.
    #[cfg_attr(feature = "ts-bindings", ts(optional))]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub model: Option<ModelRef>,
    /// Sampling temperature (0.0–2.0).
    #[cfg_attr(feature = "ts-bindings", ts(optional))]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub temperature: Option<f32>,
    /// Nucleus sampling threshold (0.0–1.0).
    #[cfg_attr(feature = "ts-bindings", ts(optional))]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub top_p: Option<f32>,
    /// Top-K sampling limit.
    #[cfg_attr(feature = "ts-bindings", ts(optional))]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub top_k: Option<i32>,
    /// Maximum tokens per response.
    #[cfg_attr(feature = "ts-bindings", ts(optional))]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_tokens: Option<u32>,
    /// Repetition penalty.
    #[cfg_attr(feature = "ts-bindings", ts(optional))]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub repeat_penalty: Option<f32>,
    /// Name of the inference profile the session sampled with, one
    /// configured on `model`'s machine. Absent in rows saved before
    /// it was recorded.
    #[cfg_attr(feature = "ts-bindings", ts(optional))]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub profile: Option<String>,
    /// Context window size (numeric or "max").
    #[cfg_attr(feature = "ts-bindings", ts(optional))]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ctx_size: Option<String>,
    /// Whether memory locking was enabled.
    #[cfg_attr(feature = "ts-bindings", ts(optional))]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub mlock: Option<bool>,
    /// Tool allowlist (empty = all tools).
    #[cfg_attr(feature = "ts-bindings", ts(type = "Array<string>", optional))]
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub tools: Vec<String>,
    /// Per-tool timeout in milliseconds.
    #[cfg_attr(feature = "ts-bindings", ts(type = "number", optional))]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_timeout_ms: Option<u64>,
    /// Maximum parallel tool calls.
    #[cfg_attr(feature = "ts-bindings", ts(optional))]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_parallel: Option<usize>,
    /// Maximum agent loop iterations.
    #[cfg_attr(feature = "ts-bindings", ts(optional))]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_iterations: Option<usize>,
    /// Whether tools were disabled entirely.
    #[cfg_attr(feature = "ts-bindings", ts(optional))]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub no_tools: Option<bool>,
    /// The chat's Thinking choice, which a turn through gglib's turn routes
    /// runs with: a paired device's turn, a far turn, and this machine's own
    /// agent run. `gglib chat --continue` reads it by the same rule: a chat
    /// switched off runs there with a budget of 0. Only `off` is stored: a
    /// turn that says `default` removes the key, and an absent key is a chat
    /// that remembers nothing.
    #[cfg_attr(feature = "ts-bindings", ts(optional))]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub thinking: Option<Thinking>,
}

#[cfg(test)]
#[path = "chat_tests.rs"]
mod chat_tests;
