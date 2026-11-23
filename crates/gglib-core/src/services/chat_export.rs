//! A chat read for export: its conversation, its saved messages, and each
//! image's bytes from this machine's store, read once however many messages
//! carry it.

use std::collections::{HashMap, HashSet};

use super::AppCore;
use crate::domain::chat_export::ChatExport;
use crate::ports::chat_history::ChatHistoryError;

impl AppCore {
    /// Conversation `id` as an export writes it ([`ChatExport`]). An image
    /// the store cannot give stays in the export by its info alone, and
    /// [`ChatExport::unread`] counts it; it never fails the export.
    ///
    /// # Errors
    ///
    /// `ConversationNotFound` for no conversation `id`, or a read of the
    /// chat history that failed.
    pub async fn export_chat(&self, id: i64) -> Result<ChatExport, ChatHistoryError> {
        let history = self.chat_history();
        let conversation = history
            .get_conversation(id)
            .await?
            .ok_or(ChatHistoryError::ConversationNotFound(id))?;
        let messages = history.get_messages(id).await?;
        let mut urls = HashMap::new();
        let mut seen = HashSet::new();
        for image in messages.iter().flat_map(|message| &message.images) {
            if !seen.insert(&image.id) {
                continue;
            }
            if let Ok(blob) = self.attachments().blob(&image.id).await {
                urls.insert(image.id.clone(), blob.data_url());
            }
        }
        Ok(ChatExport::new(conversation, messages, &urls))
    }
}
