//! Chat domain types.
//!
//! These types represent chat conversations and messages in the domain model,
//! independent of any infrastructure concerns.

use serde::{Deserialize, Serialize};

/// A chat conversation.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Conversation {
    pub id: i64,
    pub title: String,
    pub model_id: Option<i64>,
    pub system_prompt: Option<String>,
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
}

/// The role of a message sender.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum MessageRole {
    System,
    User,
    Assistant,
}

impl MessageRole {
    /// Parse a role from a string.
    #[must_use]
    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "system" => Some(Self::System),
            "user" => Some(Self::User),
            "assistant" => Some(Self::Assistant),
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
}

/// Data for creating a new message.
#[derive(Debug, Clone)]
pub struct NewMessage {
    pub conversation_id: i64,
    pub role: MessageRole,
    pub content: String,
}

/// Data for updating an existing conversation.
#[derive(Debug, Clone, Default)]
pub struct ConversationUpdate {
    pub title: Option<String>,
    /// Use `Some(Some(prompt))` to set, `Some(None)` to clear, `None` to leave unchanged.
    pub system_prompt: Option<Option<String>>,
}
