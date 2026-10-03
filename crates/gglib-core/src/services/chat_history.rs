//! Chat history service - thin orchestrator for chat operations.
//!
//! This service provides a clean interface for chat history operations,
//! delegating all persistence to the `ChatHistoryRepository` port.

use std::sync::Arc;

use crate::domain::chat::{
    Conversation, ConversationSettings, ConversationUpdate, Message, NewConversation, NewMessage,
};
use crate::domain::{Machine, ModelRef};
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
                settings: None,
            })
            .await
    }

    /// Create a conversation for `model`, by its machine, which its settings
    /// keep as the model it runs on: a conversation's machine is fixed when
    /// it is made. A model of this machine's is its `model_id` too, and one of
    /// the paired machine's leaves that empty, as
    /// [`record_settings`](Self::record_settings) does. With no `model`, this
    /// is [`create_conversation`](Self::create_conversation).
    pub async fn create_conversation_on(
        &self,
        title: String,
        model_id: Option<i64>,
        model: Option<ModelRef>,
        system_prompt: Option<String>,
    ) -> Result<i64, ChatHistoryError> {
        let model_id = model.as_ref().map_or(model_id, |model| {
            (model.machine == Machine::Local).then_some(model.id)
        });
        let settings = model.map(|model| ConversationSettings {
            model: Some(model),
            ..ConversationSettings::default()
        });
        self.repo
            .create_conversation(NewConversation {
                title,
                model_id,
                system_prompt,
                settings,
            })
            .await
    }

    /// Create a new conversation with session settings for resume.
    pub async fn create_conversation_with_settings(
        &self,
        conv: NewConversation,
    ) -> Result<i64, ChatHistoryError> {
        self.repo.create_conversation(conv).await
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
                    ..ConversationUpdate::default()
                },
            )
            .await
    }

    /// Name the model a run used on its conversation: `model`, by its
    /// machine (`None` for one of this machine's that is not in the
    /// registry), and the settings' `model_name`, keeping every other
    /// setting.
    pub async fn record_model(
        &self,
        id: i64,
        model: Option<ModelRef>,
        model_name: &str,
    ) -> Result<(), ChatHistoryError> {
        let conversation = self
            .repo
            .get_conversation(id)
            .await?
            .ok_or(ChatHistoryError::ConversationNotFound(id))?;
        let mut settings = conversation.settings.unwrap_or_default();
        settings.model_name = Some(model_name.to_owned());
        settings.model = model;
        self.record_settings(id, settings).await
    }

    /// Replace a conversation's settings with `settings`, which name the
    /// model its session uses. `model_id` names the same model when it is
    /// this machine's, and nothing otherwise, so the two never disagree.
    pub async fn record_settings(
        &self,
        id: i64,
        settings: ConversationSettings,
    ) -> Result<(), ChatHistoryError> {
        let model_id = settings
            .model
            .as_ref()
            .filter(|model| model.machine == Machine::Local)
            .map(|model| model.id);
        self.repo
            .update_conversation(
                id,
                ConversationUpdate {
                    settings: Some(Some(settings)),
                    model_id: Some(model_id),
                    ..ConversationUpdate::default()
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

    /// Save every message, in order, or none of them.
    pub async fn save_messages(&self, msgs: Vec<NewMessage>) -> Result<(), ChatHistoryError> {
        self.repo.save_messages(msgs).await
    }

    /// Delete message `from` and every later one, then save `msg`: all or
    /// none.
    pub async fn replace_from(&self, from: i64, msg: NewMessage) -> Result<i64, ChatHistoryError> {
        self.repo.replace_from(from, msg).await
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
