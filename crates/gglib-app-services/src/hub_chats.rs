//! The hub's chats, read for a paired device: `HubChatsPort` over the chat
//! history, with each chat's live run from the daemon's runs.
//!
//! Nothing here writes a row itself, and nothing logs a title, a row or an
//! image: only ids and counts. A change a device asks for is the chat
//! history service's to make, as the page's is (ADR 0017). An image a
//! device sends is stored by the one ingest every surface uses.

use std::collections::HashMap;
use std::sync::{Arc, Weak};

use async_trait::async_trait;
use gglib_core::domain::branching::{ChatChange, ChatChanged};
use gglib_core::domain::hub_chats::{HubChat, HubChatList, HubChatOpen};
use gglib_core::domain::{AttachmentBlob, AttachmentId, AttachmentUpload};
use gglib_core::ports::{AttachmentError, ChatHistoryError, HubChatsError, HubChatsPort};
use gglib_core::services::{AppCore, ChangeError};

use crate::runs::RunRegistry;

/// The hub's chats. Weak on the runs, which outlive nothing here.
pub struct HubChats {
    core: Arc<AppCore>,
    runs: Weak<RunRegistry>,
}

impl std::fmt::Debug for HubChats {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("HubChats").finish_non_exhaustive()
    }
}

impl HubChats {
    /// The chats in `core`'s history, with live runs from `runs`.
    #[must_use]
    pub fn new(core: Arc<AppCore>, runs: &Arc<RunRegistry>) -> Self {
        Self {
            core,
            runs: Arc::downgrade(runs),
        }
    }

    /// The catalogue's name for each model id, for those it still has.
    async fn model_names(&self, ids: impl Iterator<Item = i64>) -> HashMap<i64, String> {
        let mut names = HashMap::new();
        for id in ids {
            if names.contains_key(&id) {
                continue;
            }
            if let Ok(Some(model)) = self.core.models().get_by_id(id).await {
                names.insert(id, model.name);
            }
        }
        names
    }
}

#[async_trait]
impl HubChatsPort for HubChats {
    async fn list(&self) -> Result<HubChatList, HubChatsError> {
        let conversations = self
            .core
            .chat_history()
            .list_conversations()
            .await
            .map_err(|_| HubChatsError::Unreadable)?;
        let names = self
            .model_names(conversations.iter().filter_map(|c| c.model_id))
            .await;
        let runs = self.runs.upgrade();
        let chats = conversations
            .into_iter()
            .map(|c| HubChat {
                live_run: runs.as_ref().and_then(|runs| runs.live_on(c.id)),
                model: c.model_id.and_then(|id| names.get(&id).cloned()),
                id: c.id,
                title: c.title,
                model_id: c.model_id,
                updated_at: c.updated_at,
                branch_of: c.branch_of,
            })
            .collect();
        Ok(HubChatList { chats })
    }

    async fn open(&self, id: i64) -> Result<HubChatOpen, HubChatsError> {
        let history = self.core.chat_history();
        let conversation = history
            .get_conversation(id)
            .await
            .map_err(|_| HubChatsError::Unreadable)?
            .ok_or(HubChatsError::NotFound)?;
        let thread = history.thread(id).await.map_err(|e| match e {
            ChatHistoryError::ConversationNotFound(_) => HubChatsError::NotFound,
            _ => HubChatsError::Unreadable,
        })?;
        Ok(HubChatOpen {
            conversation,
            messages: thread.messages,
            points: thread.points,
            answerable: thread.answerable,
        })
    }

    async fn change(&self, id: i64, change: &ChatChange) -> Result<ChatChanged, HubChatsError> {
        let busy = self
            .runs
            .upgrade()
            .is_some_and(|runs| runs.live_on(id).is_some());
        let changed = self.core.chat_history().change(id, change, busy).await;
        changed.map_err(|e| match e {
            ChangeError::Refused(refused) => HubChatsError::Refused(refused),
            ChangeError::History(ChatHistoryError::ConversationNotFound(_)) => {
                HubChatsError::NotFound
            }
            ChangeError::History(_) => HubChatsError::Unreadable,
        })
    }

    async fn attach(&self, bytes: &[u8]) -> Result<AttachmentUpload, AttachmentError> {
        self.core.attachments().ingest(bytes).await
    }

    async fn attachment(&self, id: &AttachmentId) -> Result<AttachmentBlob, AttachmentError> {
        self.core.attachments().blob(id).await
    }
}

#[cfg(test)]
#[path = "hub_chats_images_tests.rs"]
mod hub_chats_images_tests;
#[cfg(test)]
#[path = "hub_chats_tests.rs"]
mod hub_chats_tests;
