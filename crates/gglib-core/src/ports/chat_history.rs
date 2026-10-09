//! Chat history repository port definition.
//!
//! This port defines the interface for persisting and retrieving chat
//! conversations and messages.

use async_trait::async_trait;
use thiserror::Error;

use super::attachment_store::AttachmentError;
use crate::domain::branching::LineChat;
use crate::domain::chat::{Conversation, ConversationUpdate, Message, NewConversation, NewMessage};

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

    /// A message names an image the attachment store lacks
    /// ([`AttachmentError::NotFound`]). Nothing was saved.
    #[error(transparent)]
    Attachment(#[from] AttachmentError),
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

    /// Delete a conversation and all its messages. The links from those
    /// messages to their images go with them; the images stay in the store.
    async fn delete_conversation(&self, id: i64) -> Result<(), ChatHistoryError>;

    /// Get conversation count.
    async fn get_conversation_count(&self) -> Result<i64, ChatHistoryError>;

    /// Get all messages for a conversation, in the order they were saved,
    /// each with the images it carries, in order, without their bytes.
    async fn get_messages(&self, conversation_id: i64) -> Result<Vec<Message>, ChatHistoryError>;

    /// Save a new message, with a link to each image it carries, and update
    /// the conversation timestamp: one transaction.
    ///
    /// `Attachment(NotFound)` when the store lacks one of `msg.images`;
    /// nothing is saved. So it is of every method here that saves a message.
    async fn save_message(&self, msg: NewMessage) -> Result<i64, ChatHistoryError>;

    /// Save every message, in order, or none of them: one transaction, so a
    /// failed row or a call dropped part-way writes nothing.
    async fn save_messages(&self, msgs: Vec<NewMessage>) -> Result<(), ChatHistoryError>;

    /// Delete message `from` and every later message of its conversation,
    /// then save `msg` to that conversation, in one transaction: all of it
    /// or none. Returns the saved message's id.
    ///
    /// `MessageNotFound` when `from` is not a message of `msg`'s
    /// conversation; nothing is changed.
    async fn replace_from(&self, from: i64, msg: NewMessage) -> Result<i64, ChatHistoryError>;

    /// The conversation message `id` is in, or `None` when no message has
    /// that id.
    async fn conversation_of_message(&self, id: i64) -> Result<Option<i64>, ChatHistoryError>;

    /// Delete a message and all subsequent messages in the same conversation,
    /// and update the conversation timestamp: one transaction, all of it or
    /// none. Returns the number of messages deleted.
    ///
    /// `MessageNotFound` when no message has that id; nothing is changed.
    async fn delete_message_and_subsequent(&self, id: i64) -> Result<i64, ChatHistoryError>;

    /// Get message count for a conversation.
    async fn get_message_count(&self, conversation_id: i64) -> Result<i64, ChatHistoryError>;

    /// Branch `source` (ADR 0017): a new conversation of its family, with
    /// its title and settings, holding a copy of each of its messages but
    /// system ones as far as `through` (none for `None`), then `then`, in one
    /// transaction. A copy keeps its text, metadata, images and time, and
    /// remembers the message it copies as first written. `source` is not
    /// changed. Returns the new conversation's id.
    ///
    /// `ConversationNotFound` for no `source`, `MessageNotFound` when
    /// `through` is not one of its messages, and `Attachment(NotFound)` for
    /// an image `then` names that is not stored: nothing is written.
    async fn fork(
        &self,
        source: i64,
        through: Option<i64>,
        then: Option<NewMessage>,
    ) -> Result<i64, ChatHistoryError>;

    /// Every conversation of `conversation_id`'s family, each with its
    /// messages but system ones, oldest first, as the branch points read
    /// them. Empty when no conversation has that id.
    async fn lineage(&self, conversation_id: i64) -> Result<Vec<LineChat>, ChatHistoryError>;
}
