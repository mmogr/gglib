//! Chat history service - thin orchestrator for chat operations.
//!
//! This service provides a clean interface for chat history operations,
//! delegating all persistence to the `ChatHistoryRepository` port.

use std::sync::Arc;

use crate::domain::chat::{Conversation, ConversationUpdate, Message, NewConversation, NewMessage};
use crate::ports::chat_history::{ChatHistoryError, ChatHistoryRepository};

/// Service for managing chat history.
///
/// This is a thin orchestration layer over the `ChatHistoryRepository` port.
/// It provides a clean API and handles any business logic that doesn't
/// belong in the repository layer.
pub struct ChatHistoryService {
    repo: Arc<dyn ChatHistoryRepository>,
}

impl ChatHistoryService {
    /// Create a new chat history service.
    pub fn new(repo: Arc<dyn ChatHistoryRepository>) -> Self {
        Self { repo }
    }

    /// Create a new conversation.
    pub async fn create_conversation(
        &self,
        title: String,
        model_id: Option<i64>,
        system_prompt: Option<String>,
    ) -> Result<i64, ChatHistoryError> {
        self.repo
            .create_conversation(NewConversation {
                title,
                model_id,
                system_prompt,
            })
            .await
    }

    /// List all conversations, ordered by most recently updated.
    pub async fn list_conversations(&self) -> Result<Vec<Conversation>, ChatHistoryError> {
        self.repo.list_conversations().await
    }

    /// Get a specific conversation by ID.
    pub async fn get_conversation(
        &self,
        id: i64,
    ) -> Result<Option<Conversation>, ChatHistoryError> {
        self.repo.get_conversation(id).await
    }

    /// Update conversation metadata.
    pub async fn update_conversation(
        &self,
        id: i64,
        new_title: Option<String>,
        system_prompt: Option<Option<String>>,
    ) -> Result<(), ChatHistoryError> {
        self.repo
            .update_conversation(
                id,
                ConversationUpdate {
                    title: new_title,
                    system_prompt,
                },
            )
            .await
    }

    /// Delete a conversation and all its messages.
    pub async fn delete_conversation(&self, id: i64) -> Result<(), ChatHistoryError> {
        self.repo.delete_conversation(id).await
    }

    /// Get conversation count.
    pub async fn get_conversation_count(&self) -> Result<i64, ChatHistoryError> {
        self.repo.get_conversation_count().await
    }

    /// Get all messages for a conversation.
    pub async fn get_messages(
        &self,
        conversation_id: i64,
    ) -> Result<Vec<Message>, ChatHistoryError> {
        self.repo.get_messages(conversation_id).await
    }

    /// Save a new message.
    pub async fn save_message(&self, msg: NewMessage) -> Result<i64, ChatHistoryError> {
        self.repo.save_message(msg).await
    }

    /// Update a message's content and optionally its metadata.
    pub async fn update_message(
        &self,
        id: i64,
        content: String,
        metadata: Option<serde_json::Value>,
    ) -> Result<(), ChatHistoryError> {
        self.repo.update_message(id, content, metadata).await
    }

    /// Delete a message and all subsequent messages.
    pub async fn delete_message_and_subsequent(&self, id: i64) -> Result<i64, ChatHistoryError> {
        self.repo.delete_message_and_subsequent(id).await
    }

    /// Get message count for a conversation.
    pub async fn get_message_count(&self, conversation_id: i64) -> Result<i64, ChatHistoryError> {
        self.repo.get_message_count(conversation_id).await
    }
}
