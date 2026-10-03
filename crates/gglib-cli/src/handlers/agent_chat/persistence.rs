//! Conversation persistence for CLI agent sessions.
//!
//! Saves agent messages to the `chat_conversations` / `chat_messages` tables
//! so they appear in the GUI conversation list and can later be resumed.

use anyhow::{Result, anyhow};
use chrono::Local;

use gglib_core::domain::agent::{AgentMessage, to_new_message};
use gglib_core::domain::chat::{self, ConversationSettings, NewConversation};
use gglib_core::services::ChatHistoryService;

/// The conversation `--continue` names, when it names one.
///
/// # Errors
///
/// An id no conversation has, and a read that failed.
pub(crate) async fn continued(
    service: &ChatHistoryService,
    id: Option<i64>,
) -> Result<Option<chat::Conversation>> {
    let Some(id) = id else {
        return Ok(None);
    };
    let found = service.get_conversation(id).await?;
    found
        .map(Some)
        .ok_or_else(|| anyhow!("conversation {id} not found"))
}

/// Tracks a persisted conversation and the number of messages already saved,
/// so subsequent calls to [`Conversation::save_new`] only write the delta.
pub(crate) struct Conversation<'a> {
    service: &'a ChatHistoryService,
    pub id: i64,
    saved: usize,
}

impl<'a> Conversation<'a> {
    /// Create a new conversation with a timestamp-based title.
    pub(crate) async fn create(
        service: &'a ChatHistoryService,
        system_prompt: Option<String>,
        model_id: Option<i64>,
        settings: Option<ConversationSettings>,
    ) -> Result<Conversation<'a>> {
        let title = format!("Agent session {}", Local::now().format("%Y-%m-%d %H:%M"));
        let id = service
            .create_conversation_with_settings(NewConversation {
                title,
                model_id,
                system_prompt,
                settings,
            })
            .await?;
        Ok(Conversation {
            service,
            id,
            saved: 0,
        })
    }

    /// Resume an existing conversation for continued persistence.
    ///
    /// Loads the existing message count so [`Conversation::save_new`] only
    /// persists the delta.
    #[allow(
        clippy::unused_async,
        reason = "grandfathered at lint inheritance, #1157"
    )]
    pub(crate) async fn resume(
        service: &'a ChatHistoryService,
        id: i64,
        existing_message_count: usize,
    ) -> Conversation<'a> {
        Conversation {
            service,
            id,
            saved: existing_message_count,
        }
    }

    /// Replace the conversation's settings with `settings`, when there are
    /// new ones: the model a resumed session moved it to. Logged and
    /// swallowed, as a message that is not saved is.
    pub(crate) async fn record_settings(self, settings: Option<ConversationSettings>) -> Self {
        let Some(settings) = settings else {
            return self;
        };
        if let Err(e) = self.service.record_settings(self.id, settings).await {
            tracing::warn!("failed to record the session's model on its conversation: {e}");
        }
        self
    }

    /// Persist any messages added since the last call.
    ///
    /// System messages are **not** persisted — the system prompt lives on the
    /// `chat_conversations` row (`system_prompt` column) and is the canonical
    /// source for both CLI and GUI resume.  Persisting it as a message row
    /// would cause duplicates when the GUI hydrates from both sources.
    ///
    /// Errors are logged as warnings and swallowed — persistence must never
    /// break the interactive session.
    pub(crate) async fn save_new(&mut self, messages: &[AgentMessage]) {
        for msg in messages.iter().skip(self.saved) {
            if matches!(msg, AgentMessage::System { .. }) {
                continue;
            }
            let new_msg = to_new_message(msg, self.id);
            if let Err(e) = self.service.save_message(new_msg).await {
                tracing::warn!("failed to persist agent message: {e}");
            }
        }
        self.saved = messages.len();
    }
}
