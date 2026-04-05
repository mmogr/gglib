//! Chat domain types.
//!
//! These types represent chat conversations and messages in the domain model,
//! independent of any infrastructure concerns.
//!
//! [`ConversationSettings`] captures CLI/GUI session parameters (sampling,
//! context, tools) so conversations can be faithfully resumed.

use serde::{Deserialize, Serialize};

/// A chat conversation.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Conversation {
    pub id: i64,
    pub title: String,
    pub model_id: Option<i64>,
    pub system_prompt: Option<String>,
    /// Session parameters captured at creation for resume.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub settings: Option<ConversationSettings>,
    pub created_at: String,
    pub updated_at: String,
}

/// A chat message within a conversation.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Message {
    pub id: i64,
    pub conversation_id: i64,
    pub role: MessageRole,
    pub content: String,
    pub created_at: String,
    /// Optional JSON metadata for deep research state, tool usage, etc.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub metadata: Option<serde_json::Value>,
}

/// The role of a message sender.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
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
#[derive(Debug, Clone)]
pub struct NewConversation {
    pub title: String,
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
    /// Optional JSON metadata for deep research state, tool usage, etc.
    pub metadata: Option<serde_json::Value>,
}

/// Data for updating an existing conversation.
#[derive(Debug, Clone, Default)]
pub struct ConversationUpdate {
    pub title: Option<String>,
    /// Use `Some(Some(prompt))` to set, `Some(None)` to clear, `None` to leave unchanged.
    pub system_prompt: Option<Option<String>>,
    /// Use `Some(Some(settings))` to set, `Some(None)` to clear, `None` to leave unchanged.
    pub settings: Option<Option<ConversationSettings>>,
}

/// Session parameters captured at conversation creation for resume.
///
/// Stores sampling, context, and tool configuration so a CLI or GUI session
/// can be faithfully restored. Serialized as a JSON column in the database.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub struct ConversationSettings {
    /// Model name or identifier used for this session.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub model_name: Option<String>,
    /// Sampling temperature (0.0–2.0).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub temperature: Option<f32>,
    /// Nucleus sampling threshold (0.0–1.0).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub top_p: Option<f32>,
    /// Top-K sampling limit.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub top_k: Option<i32>,
    /// Maximum tokens per response.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_tokens: Option<u32>,
    /// Repetition penalty.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub repeat_penalty: Option<f32>,
    /// Context window size (numeric or "max").
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ctx_size: Option<String>,
    /// Whether memory locking was enabled.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub mlock: Option<bool>,
    /// Tool allowlist (empty = all tools).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub tools: Vec<String>,
    /// Per-tool timeout in milliseconds.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_timeout_ms: Option<u64>,
    /// Maximum parallel tool calls.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_parallel: Option<usize>,
    /// Maximum agent loop iterations.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_iterations: Option<usize>,
    /// Whether tools were disabled entirely.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub no_tools: Option<bool>,
}
