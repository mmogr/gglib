//! Chat history repository port definition.
//!
//! This port defines the interface for persisting and retrieving chat
//! conversations and messages.

use async_trait::async_trait;
use thiserror::Error;

use crate::domain::chat::{
    Conversation, ConversationUpdate, Message, MessageRole, NewConversation, NewMessage,
};

/// Errors that can occur in chat history operations.
#[derive(Debug, Error)]
pub enum ChatHistoryError {
    #[error("Conversation not found: {0}")]
    ConversationNotFound(i64),

    #[error("Message not found: {0}")]
    MessageNotFound(i64),

    #[error("Invalid message role: {0}")]
    InvalidRole(String),

    #[error("Database error: {0}")]
    Database(String),
}

/// Port for chat history persistence operations.
///
/// This trait defines the interface for storing and retrieving chat
/// conversations and messages. Implementations handle the actual storage
/// mechanism (`SQLite`, etc.).
#[async_trait]
pub trait ChatHistoryRepository: Send + Sync {
    /// Create a new conversation.
    async fn create_conversation(&self, conv: NewConversation) -> Result<i64, ChatHistoryError>;

    /// List all conversations, ordered by most recently updated.
    async fn list_conversations(&self) -> Result<Vec<Conversation>, ChatHistoryError>;

    /// Get a specific conversation by ID.
    async fn get_conversation(&self, id: i64) -> Result<Option<Conversation>, ChatHistoryError>;

    /// Update conversation metadata.
    async fn update_conversation(
        &self,
        id: i64,
        update: ConversationUpdate,
    ) -> Result<(), ChatHistoryError>;

    /// Delete a conversation and all its messages.
    async fn delete_conversation(&self, id: i64) -> Result<(), ChatHistoryError>;

    /// Get conversation count.
    async fn get_conversation_count(&self) -> Result<i64, ChatHistoryError>;

    /// Get all messages for a conversation, ordered chronologically.
    async fn get_messages(&self, conversation_id: i64) -> Result<Vec<Message>, ChatHistoryError>;

    /// Save a new message and update conversation timestamp.
    async fn save_message(&self, msg: NewMessage) -> Result<i64, ChatHistoryError>;

    /// Update a message's content and optionally its metadata.
    async fn update_message(
        &self,
        id: i64,
        content: String,
        metadata: Option<serde_json::Value>,
    ) -> Result<(), ChatHistoryError>;

    /// Delete a message and all subsequent messages in the same conversation.
    /// Returns the number of messages deleted.
    async fn delete_message_and_subsequent(&self, id: i64) -> Result<i64, ChatHistoryError>;

    /// Get message count for a conversation.
    async fn get_message_count(&self, conversation_id: i64) -> Result<i64, ChatHistoryError>;
}

/// Validate a message role string.
pub fn validate_role(role: &str) -> Result<MessageRole, ChatHistoryError> {
    MessageRole::parse(role).ok_or_else(|| ChatHistoryError::InvalidRole(role.to_string()))
}
