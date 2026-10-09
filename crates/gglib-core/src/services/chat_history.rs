//! Chat history service - thin orchestrator for chat operations.
//!
//! This service provides a clean interface for chat history operations,
//! delegating all persistence to the `ChatHistoryRepository` port.

use std::sync::Arc;

use crate::domain::branching::{
    self, ChatChange, ChatChanged, ChatThread, EDITED_KEY, Plan, Refused, Then,
};
use crate::domain::chat::{
    Conversation, ConversationSettings, ConversationUpdate, Message, MessageRole, NewConversation,
    NewMessage,
};
use crate::domain::{Machine, ModelRef, Thinking};
use crate::ports::chat_history::{ChatHistoryError, ChatHistoryRepository};

/// Why a change to a chat was not made. Nothing is written.
#[derive(Debug, thiserror::Error)]
pub enum ChangeError {
    /// The branching rules refuse it.
    #[error(transparent)]
    Refused(#[from] Refused),
    /// The chat history could not be read or written.
    #[error(transparent)]
    History(#[from] ChatHistoryError),
}

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

    /// Create a conversation: the one way one is made. Settings that name
    /// its model decide its `model_id`, as
    /// [`record_settings`](Self::record_settings) decides it: that model's
    /// id when it is this machine's, and nothing when it is the paired
    /// machine's. Settings that name no model, and no settings, leave
    /// `model_id` as it is given.
    pub async fn create_conversation(
        &self,
        mut conv: NewConversation,
    ) -> Result<i64, ChatHistoryError> {
        if let Some(model) = conv.settings.as_ref().and_then(|s| s.model.as_ref()) {
            conv.model_id = local_id(model);
        }
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

    /// Set what a conversation remembers of thinking: `thinking`, or nothing
    /// for `None`, keeping every other setting and the conversation's
    /// `model_id`. Writes nothing when it already remembers that.
    pub async fn record_thinking(
        &self,
        id: i64,
        thinking: Option<Thinking>,
    ) -> Result<(), ChatHistoryError> {
        let conversation = self
            .repo
            .get_conversation(id)
            .await?
            .ok_or(ChatHistoryError::ConversationNotFound(id))?;
        let mut settings = conversation.settings.unwrap_or_default();
        if settings.thinking == thinking {
            return Ok(());
        }
        settings.thinking = thinking;
        let update = ConversationUpdate {
            settings: Some(Some(settings)),
            ..ConversationUpdate::default()
        };
        self.repo.update_conversation(id, update).await
    }

    /// Replace a conversation's settings with `settings`, which name the
    /// model its session uses. `model_id` names the same model when it is
    /// this machine's, and nothing otherwise, so the two never disagree.
    pub async fn record_settings(
        &self,
        id: i64,
        settings: ConversationSettings,
    ) -> Result<(), ChatHistoryError> {
        let model_id = settings.model.as_ref().and_then(local_id);
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

    /// The conversation message `id` is in, if any.
    pub async fn conversation_of_message(&self, id: i64) -> Result<Option<i64>, ChatHistoryError> {
        self.repo.conversation_of_message(id).await
    }

    /// Delete a message and all subsequent messages: all or none.
    pub async fn delete_message_and_subsequent(&self, id: i64) -> Result<i64, ChatHistoryError> {
        self.repo.delete_message_and_subsequent(id).await
    }

    /// Make `change` to conversation `id`, `busy` saying whether a reply to
    /// it is being written (ADR 0017): in place, or on a new branch of it,
    /// as [`branching::plan`](crate::domain::branching::plan()) decides.
    ///
    /// # Errors
    ///
    /// `ConversationNotFound` for no conversation `id`, the rules' refusal,
    /// or a write that failed. Nothing is written.
    pub async fn change(
        &self,
        id: i64,
        change: &ChatChange,
        busy: bool,
    ) -> Result<ChatChanged, ChangeError> {
        self.require(id).await?;
        let path = self.repo.get_messages(id).await?;
        match branching::plan(&path, change, busy)? {
            Plan::Replace { question } => {
                let edited = written(id, change, MessageRole::User);
                self.repo.replace_from(question, edited).await?;
                Ok(ChatChanged {
                    conversation_id: id,
                    forked: false,
                    answer: true,
                })
            }
            Plan::Fork {
                through,
                then,
                answer,
            } => {
                let then = match then {
                    Then::Nothing => None,
                    Then::Question => Some(written(id, change, MessageRole::User)),
                    Then::EditedReply => Some(written(id, change, MessageRole::Assistant)),
                };
                let branch = self.repo.fork(id, through, then).await?;
                Ok(ChatChanged {
                    conversation_id: branch,
                    forked: true,
                    answer,
                })
            }
        }
    }

    /// Conversation `id` as a client reads it: its messages, the branch
    /// points its family holds along them, and whether it ends in a
    /// question nothing answers.
    ///
    /// # Errors
    ///
    /// `ConversationNotFound` for no conversation `id`, or a read that
    /// failed.
    pub async fn thread(&self, id: i64) -> Result<ChatThread, ChatHistoryError> {
        self.require(id).await?;
        let messages = self.repo.get_messages(id).await?;
        let family = self.repo.lineage(id).await?;
        Ok(ChatThread {
            points: branching::points(id, &family),
            answerable: branching::answerable(&messages).is_ok(),
            messages,
        })
    }

    async fn require(&self, id: i64) -> Result<(), ChatHistoryError> {
        match self.repo.get_conversation(id).await? {
            Some(_) => Ok(()),
            None => Err(ChatHistoryError::ConversationNotFound(id)),
        }
    }

    /// Get message count for a conversation.
    pub async fn get_message_count(&self, conversation_id: i64) -> Result<i64, ChatHistoryError> {
        self.repo.get_message_count(conversation_id).await
    }
}

/// The message an edit writes to `conversation_id`: its text and images, as
/// `role`. An edited reply says it was edited, and carries nothing of how
/// the model made the reply it replaces.
fn written(conversation_id: i64, change: &ChatChange, role: MessageRole) -> NewMessage {
    let (content, images) = match change {
        ChatChange::Edit {
            content, images, ..
        } => (content.clone(), images.clone()),
        ChatChange::Regenerate { .. } | ChatChange::Branch { .. } => (String::new(), Vec::new()),
    };
    NewMessage {
        conversation_id,
        role,
        content,
        metadata: (role == MessageRole::Assistant).then(|| serde_json::json!({ EDITED_KEY: true })),
        images,
    }
}

/// The `model_id` of a conversation on `model`: its id when it is this
/// machine's, and none when it is the paired machine's, whose ids are not
/// this catalogue's.
fn local_id(model: &ModelRef) -> Option<i64> {
    (model.machine == Machine::Local).then_some(model.id)
}
