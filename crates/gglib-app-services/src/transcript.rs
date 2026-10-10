//! What a turn writes to its conversation, on every surface that runs one:
//! the daemon's agent runs and the CLI's chat both save through here.
//!
//! The user's message is saved when the turn starts, after the rest; a turn
//! that answers a question already saved saves none, and runs from the
//! chat's own history ([`answer_history`]). A change to what is saved is
//! never made by a turn: it is the chat history service's (ADR 0017). The
//! reply is saved when the turn ends, whatever the end, every row or none,
//! with how long each model turn thought: from its first reasoning event to
//! its last, as they were logged.
//! A Thinking choice the turn said is remembered on the conversation by
//! [`remember_thinking`], whichever surface said it.
//!
//! The reply's rows are rebuilt from the events the turn logged, each named
//! for the model that made it ([`MadeBy`]), and never from the loop's own
//! history: that is pruned to its context budget, so it is no count of what
//! is already saved, and a turn that fails or is cancelled returns none.

use std::sync::{Arc, Mutex, MutexGuard, PoisonError};
use std::time::Instant;

use gglib_core::domain::Thinking;
use gglib_core::domain::agent::{
    AgentEvent, AgentMessage, MADE_KEYS, rows_from_timed_frames, saved_history, to_new_message,
};
use gglib_core::domain::branching;
use gglib_core::ports::ChatHistoryError;
use gglib_core::services::{ChangeError, ChatHistoryService};
use serde_json::{Map, Value};

/// When each of a turn's frames was logged, in ms from the turn's start;
/// shared by the loop that logs and the end that saves.
#[derive(Clone)]
pub struct FrameTimes {
    start: Instant,
    logged: Arc<Mutex<Vec<u64>>>,
}

impl FrameTimes {
    pub fn new() -> Self {
        Self {
            start: Instant::now(),
            logged: Arc::default(),
        }
    }

    /// A frame was logged now.
    pub fn logged(&self) {
        let ms = u64::try_from(self.start.elapsed().as_millis()).unwrap_or(u64::MAX);
        self.lock().push(ms);
    }

    fn lock(&self) -> MutexGuard<'_, Vec<u64>> {
        self.logged.lock().unwrap_or_else(PoisonError::into_inner)
    }
}

impl Default for FrameTimes {
    fn default() -> Self {
        Self::new()
    }
}

/// The model a turn drives, the context it was launched with, and the paired
/// device whose turn it answers, which the loop does not know: stamped on
/// each turn's usage before it is logged.
#[derive(Clone)]
pub struct MadeBy {
    pub model: String,
    pub quantization: Option<String>,
    /// Absent for this machine's own turns.
    pub device: Option<String>,
    /// The context the model was launched with. Absent when that is not
    /// known: a paired machine's model, one outside the primary slot, or a
    /// turn the CLI runs, which does not hold the model.
    pub context_size: Option<u64>,
}

impl MadeBy {
    /// Name the model and its context's size on a `turn_usage` event; any
    /// other passes unchanged.
    pub fn stamp(&self, event: &mut AgentEvent) {
        if let AgentEvent::TurnUsage(usage) = event {
            usage.model = Some(self.model.clone());
            usage.quantization.clone_from(&self.quantization);
            usage.device.clone_from(&self.device);
            usage.reading.context_size = self.context_size;
        }
    }
}

/// Save the turn's last message, when it is the user's, to
/// `conversation_id`, after the rest. A paired `device`'s message says
/// which.
///
/// # Errors
///
/// [`ChatHistoryError::Attachment`] for an image the message names that is
/// not stored, and a write that failed. Nothing is saved.
pub async fn save_user(
    history: &ChatHistoryService,
    conversation_id: i64,
    last: Option<&AgentMessage>,
    device: Option<&str>,
) -> Result<(), ChatHistoryError> {
    let Some(user @ AgentMessage::User { .. }) = last else {
        return Ok(());
    };
    let mut row = to_new_message(user, conversation_id);
    if let Some(device) = device {
        let mut fields = match row.metadata.take() {
            Some(Value::Object(fields)) => fields,
            _ => Map::new(),
        };
        fields.insert(MADE_KEYS.device.to_owned(), Value::from(device));
        row.metadata = Some(Value::Object(fields));
    }
    history.save_message(row).await.map(|_| ())
}

/// The history a turn that answers `conversation_id`'s last question runs
/// from: its prompt, then every saved message, as a resumed chat is sent
/// ([`saved_history`]). The turn saves no message of its own.
///
/// # Errors
///
/// `ConversationNotFound` for no such conversation, `NothingToAnswer` when
/// it does not end in a question with no reply, and a read that failed.
pub async fn answer_history(
    history: &ChatHistoryService,
    conversation_id: i64,
) -> Result<Vec<AgentMessage>, ChangeError> {
    let conversation = history
        .get_conversation(conversation_id)
        .await?
        .ok_or(ChatHistoryError::ConversationNotFound(conversation_id))?;
    let rows = history.get_messages(conversation_id).await?;
    branching::answerable(&rows)?;
    Ok(saved_history(conversation.system_prompt.as_deref(), &rows))
}

/// Save the reply a turn logged as `frames` to `conversation_id`, once the
/// turn has ended, whatever the end: every row or none.
///
/// `finished` is whether it completed; one that did not has its last
/// assistant row say so. `times` holds when each frame was logged, one per
/// frame in the frames' order: whoever logs a frame records its time right
/// after. Answers with how many rows the reply was.
///
/// # Errors
///
/// A write that failed. Nothing is saved.
pub async fn save_reply<'a>(
    history: &ChatHistoryService,
    conversation_id: i64,
    frames: impl IntoIterator<Item = &'a str>,
    times: &FrameTimes,
    finished: bool,
) -> Result<usize, ChatHistoryError> {
    let logged = times.lock().clone();
    let at = |i: usize| logged.get(i).copied();
    let with_times = frames.into_iter().enumerate().map(|(i, f)| (f, at(i)));
    let rows = rows_from_timed_frames(with_times, finished, conversation_id);
    let total = rows.len();
    history.save_messages(rows).await.map(|()| total)
}

/// Set what `conversation_id` remembers of thinking to `choice`, as a turn
/// said: `off`, or nothing once it said `default`.
///
/// One field of the conversation's settings; every other stays. Not saved
/// is logged, not refused: the turn runs by the choice either way.
pub async fn remember_thinking(
    history: &ChatHistoryService,
    conversation_id: i64,
    choice: Option<Thinking>,
) {
    let recorded = history.record_thinking(conversation_id, choice).await;
    if recorded.is_err() {
        tracing::warn!(
            conversation = conversation_id,
            "a turn's thinking choice was not recorded on its conversation"
        );
    }
}

#[cfg(test)]
#[path = "transcript_tests.rs"]
mod tests;
