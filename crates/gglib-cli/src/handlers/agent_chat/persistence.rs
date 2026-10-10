//! Conversation persistence for CLI agent sessions.
//!
//! A session's turns go to the `chat_conversations` / `chat_messages` tables
//! so they appear in the GUI conversation list and can later be resumed.
//! The rows are `gglib_app_services::transcript`'s to write, as they are for
//! the daemon's agent runs: the user's message when it is sent, and the
//! reply when its turn ends, finished or not, rebuilt from the events the
//! turn sent ([`Reply`]). The loop's own history is never what is saved: it
//! is pruned to the context budget, so it holds neither every row of a long
//! turn nor a count of the rows already saved.

use anyhow::{Result, anyhow};
use chrono::Local;

use gglib_app_services::transcript::{self, FrameTimes, MadeBy};
use gglib_core::domain::Thinking;
use gglib_core::domain::agent::{AgentEvent, AgentMessage};
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

/// A turn's reply as it arrives: each event as the frame an agent run logs
/// for it, its usage naming the model the turn is made by, and when it came.
pub(crate) struct Reply {
    made_by: MadeBy,
    frames: Vec<String>,
    times: FrameTimes,
}

impl Reply {
    /// The reply of a turn that starts now.
    pub(crate) fn new(made_by: MadeBy) -> Self {
        Self {
            made_by,
            frames: Vec::new(),
            times: FrameTimes::new(),
        }
    }

    /// `event` arrived now.
    pub(crate) fn heard(&mut self, event: &mut AgentEvent) {
        self.made_by.stamp(event);
        // One time per frame: an event that will not serialise logs neither.
        if let Ok(frame) = serde_json::to_string(event) {
            self.frames.push(frame);
            self.times.logged();
        }
    }
}

/// The saved conversation a session's turns are written to, and the model
/// those turns are made by.
pub(crate) struct Conversation<'a> {
    service: &'a ChatHistoryService,
    pub id: i64,
    made_by: MadeBy,
}

impl<'a> Conversation<'a> {
    /// Create a new conversation with a timestamp-based title. Its
    /// `model_id` is the one its `settings` name, as the service decides it.
    pub(crate) async fn create(
        service: &'a ChatHistoryService,
        system_prompt: Option<String>,
        settings: Option<ConversationSettings>,
        made_by: MadeBy,
    ) -> Result<Conversation<'a>> {
        let title = format!("Agent session {}", Local::now().format("%Y-%m-%d %H:%M"));
        let id = service
            .create_conversation(NewConversation {
                title,
                model_id: None,
                system_prompt,
                settings,
            })
            .await?;
        Ok(Conversation {
            service,
            id,
            made_by,
        })
    }

    /// The existing conversation `id`, for a session that continues it.
    pub(crate) fn resume(service: &'a ChatHistoryService, id: i64, made_by: MadeBy) -> Self {
        Self {
            service,
            id,
            made_by,
        }
    }

    /// The same session's turns, saved to chat `id` instead: the branch a
    /// change made, which the session goes on in.
    pub(crate) fn moved_to(&self, id: i64) -> Self {
        Self {
            service: self.service,
            id,
            made_by: self.made_by.clone(),
        }
    }

    /// The chat history the conversation is kept in, which makes a change
    /// to it as the branching rules say.
    pub(crate) const fn history(&self) -> &'a ChatHistoryService {
        self.service
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

    /// Have the conversation remember `choice` of thinking, as a daemon's
    /// run does when its turn says one: `off`, or nothing for `on`. Every
    /// other setting stays, and a write that fails is logged there.
    pub(crate) async fn remember_thinking(&self, choice: Option<Thinking>) {
        transcript::remember_thinking(self.service, self.id, choice).await;
    }

    /// An empty [`Reply`] for a turn of this session's that starts now.
    pub(crate) fn reply(&self) -> Reply {
        Reply::new(self.made_by.clone())
    }

    /// Save `message`, the one a turn starts with, when it is the user's.
    ///
    /// The system prompt is never a row: it lives on the conversation, which
    /// is where every surface reads it back from.
    ///
    /// Errors are logged as warnings and swallowed: persistence must never
    /// break the interactive session.
    pub(crate) async fn save_user(&self, message: Option<&AgentMessage>) {
        let saved = transcript::save_user(self.service, self.id, message, None).await;
        if let Err(e) = saved {
            tracing::warn!("failed to persist the user's message: {e}");
        }
    }

    /// Save a turn's `reply` once the turn has ended: every row or none.
    /// `finished` is whether it gave its answer; the reply of a turn that
    /// failed or was cancelled is saved as far as it got, and says so.
    ///
    /// Errors are logged as warnings and swallowed, as a message's are.
    pub(crate) async fn save_reply(&self, reply: &Reply, finished: bool) {
        let frames = reply.frames.iter().map(String::as_str);
        let saved =
            transcript::save_reply(self.service, self.id, frames, &reply.times, finished).await;
        if let Err(e) = saved {
            tracing::warn!("failed to persist the agent's reply: {e}");
        }
    }
}
