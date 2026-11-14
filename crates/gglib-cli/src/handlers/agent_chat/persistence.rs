//! The saved conversation a session's turns are added to.
//!
//! The daemon saves each turn, as it runs it: the user's message when the
//! turn starts, and the reply when it ends, finished or not. What a session
//! writes here is the conversation's row: the conversation itself, the
//! settings this command line names, which the daemon reads on every turn,
//! and its system prompt.

use anyhow::{Result, anyhow};
use chrono::Local;

use gglib_core::domain::chat::{self, ConversationSettings, NewConversation};
use gglib_core::domain::conversation_patch::ConversationSettingsPatch;
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

/// The saved conversation a session's turns are added to.
pub(crate) struct Conversation<'a> {
    service: &'a ChatHistoryService,
    pub id: i64,
    /// The name of the paired machine its model is on; `None` for this one.
    pub machine: Option<String>,
}

impl<'a> Conversation<'a> {
    /// Create a new conversation with a timestamp-based title. Its
    /// `model_id` is the one its `settings` name, as the service decides it.
    ///
    /// # Errors
    ///
    /// The write failed: a session has no turn to send without one.
    pub(crate) async fn create(
        service: &'a ChatHistoryService,
        system_prompt: Option<String>,
        settings: ConversationSettings,
        machine: Option<String>,
    ) -> Result<Conversation<'a>> {
        let title = format!("Agent session {}", Local::now().format("%Y-%m-%d %H:%M"));
        let id = service
            .create_conversation(NewConversation {
                title,
                model_id: None,
                system_prompt,
                settings: Some(settings),
            })
            .await?;
        Ok(Conversation {
            service,
            id,
            machine,
        })
    }

    /// The existing conversation `id`, for a session that continues it.
    pub(crate) const fn resume(
        service: &'a ChatHistoryService,
        id: i64,
        machine: Option<String>,
    ) -> Self {
        Self {
            service,
            id,
            machine,
        }
    }

    /// The same session's turns, added to chat `id` instead: the branch a
    /// change made, which the session goes on in.
    pub(crate) fn moved_to(&self, id: i64) -> Self {
        Self {
            service: self.service,
            id,
            machine: self.machine.clone(),
        }
    }

    /// The chat history the conversation is kept in, which makes a change
    /// to it as the branching rules say.
    pub(crate) const fn history(&self) -> &'a ChatHistoryService {
        self.service
    }

    /// Make the conversation's settings `settings`, when there are new ones,
    /// before the session's first turn reads them: only the keys in which
    /// they differ from `held`, the settings they were made from, are
    /// written, so a key this build cannot read stays.
    ///
    /// # Errors
    ///
    /// The write failed: the turn would run on what the chat saved before.
    pub(crate) async fn record_settings(
        &self,
        held: &ConversationSettings,
        settings: Option<ConversationSettings>,
    ) -> Result<()> {
        if let Some(settings) = settings {
            self.service
                .record_settings(self.id, held, settings)
                .await?;
        }
        Ok(())
    }

    /// Replace the conversation's system prompt with `prompt`, which every
    /// later turn is then sent with.
    ///
    /// # Errors
    ///
    /// The write failed.
    pub(crate) async fn record_system_prompt(&self, prompt: &str) -> Result<()> {
        let patch = ConversationSettingsPatch::default();
        let prompt = Some(Some(prompt.to_owned()));
        self.service
            .update_conversation(self.id, None, prompt, patch, None)
            .await?;
        Ok(())
    }
}
