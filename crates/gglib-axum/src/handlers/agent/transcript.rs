//! What an agent run writes to its conversation.
//!
//! The user's message is saved once the run is accepted. When the request
//! names a row to replace (an edit, or a regenerate), that row and every
//! later row are deleted in the same transaction, so a refused run, or one
//! that never got as far, changes nothing. A conversation's machine is fixed:
//! a run on another machine than the one it ran on is refused before its
//! message is saved. Once the message is saved, the run names its model on
//! the conversation by its machine, so the chat's next turn, from either
//! door, runs on it, or is refused for being the paired machine's. The reply
//! is saved when the run ends, all rows or none, with how long each turn
//! thought: from its first reasoning event to its last, as they were logged.

use std::sync::{Arc, Mutex, PoisonError};
use std::time::Instant;

use axum::http::StatusCode;
use gglib_core::domain::agent::{AgentMessage, MADE_KEYS, rows_from_timed_frames, to_new_message};
use gglib_core::domain::runs::{RunError, RunStatus};
use gglib_core::domain::{Machine, ModelRef};
use gglib_core::ports::ChatHistoryError;
use gglib_core::services::AppCore;
use serde_json::{Map, Value};

use gglib_app_services::RunEnded;

use crate::error::HttpError;

/// When each of a run's frames was logged, in ms from the run's start;
/// shared by the loop that logs and the end that saves.
#[derive(Clone)]
pub(super) struct FrameTimes {
    start: Instant,
    logged: Arc<Mutex<Vec<u64>>>,
}

impl FrameTimes {
    pub(super) fn new() -> Self {
        Self {
            start: Instant::now(),
            logged: Arc::default(),
        }
    }

    /// A frame was logged now.
    pub(super) fn logged(&self) {
        let ms = u64::try_from(self.start.elapsed().as_millis()).unwrap_or(u64::MAX);
        self.lock().push(ms);
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, Vec<u64>> {
        self.logged.lock().unwrap_or_else(PoisonError::into_inner)
    }
}

fn coded(status: StatusCode, code: &'static str, message: impl Into<String>) -> HttpError {
    HttpError::Coded {
        status,
        code,
        message: message.into(),
    }
}

/// Refuse a run on `ran_on` in `conversation_id` when the conversation ran
/// on another machine: the one its stored model names, or this one when it
/// stores only a `model_id`. One that names no model takes the run's. Paired
/// machines are told apart by fingerprint, so a chat of a machine this one
/// has since replaced is refused as well, rather than sent to whatever has
/// its id on the new one.
///
/// # Errors
///
/// `conflict` (409) for a run on another machine; `internal_error` when the
/// conversation cannot be read.
pub(super) async fn keep_machine(
    core: &AppCore,
    conversation_id: i64,
    ran_on: &Machine,
) -> Result<(), HttpError> {
    let conversation = core
        .chat_history()
        .get_conversation(conversation_id)
        .await
        .map_err(|_| {
            coded(
                StatusCode::INTERNAL_SERVER_ERROR,
                "internal_error",
                "the conversation could not be read",
            )
        })?;
    let Some(conversation) = conversation else {
        return Ok(());
    };
    let stored = conversation
        .settings
        .and_then(|settings| settings.model)
        .map(|model| model.machine)
        .or_else(|| conversation.model_id.map(|_| Machine::Local));
    let refusal = match (stored, ran_on) {
        (None, _) => return Ok(()),
        (Some(stored), ran_on) if stored == *ran_on => return Ok(()),
        (Some(Machine::Local), _) => {
            "this chat ran on this machine and continues here, on one of its models"
        }
        (Some(Machine::Paired { .. }), Machine::Local) => {
            "this chat ran on the paired machine and continues there, on its model"
        }
        (Some(Machine::Paired { .. }), Machine::Paired { .. }) => {
            "this chat ran on a machine this one is no longer paired with, so it cannot be \
             continued here"
        }
    };
    Err(coded(StatusCode::CONFLICT, "conflict", refusal))
}

/// Save the request's last message, when it is the user's, to
/// `conversation_id`; with `replace_from`, in place of that row and every
/// later one, in one transaction. A paired `device`'s message says which.
pub(super) async fn save_user(
    core: &AppCore,
    conversation_id: i64,
    replace_from: Option<i64>,
    last: Option<&AgentMessage>,
    device: Option<&str>,
) -> Result<(), HttpError> {
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
    let saved = match replace_from {
        Some(from) => core
            .chat_history()
            .replace_from(from, row)
            .await
            .map(|_| ()),
        None => core.chat_history().save_message(row).await.map(|_| ()),
    };
    saved.map_err(|e| match e {
        ChatHistoryError::MessageNotFound(id) => coded(
            StatusCode::NOT_FOUND,
            "message_not_found",
            format!("conversation {conversation_id} has no message {id} to replace"),
        ),
        ChatHistoryError::Attachment(refusal) => refusal.into(),
        _ => coded(
            StatusCode::INTERNAL_SERVER_ERROR,
            "internal_error",
            "the user's message could not be saved",
        ),
    })
}

/// Name the model a run uses on `conversation_id`, by its machine, as the
/// settings' model: a far run's id on the paired machine, so the chat stays
/// that machine's; a local run's registry id when it is there (none when it
/// is not, which is also the conversation's `model_id`). Its name goes in
/// the settings either way. [`keep_machine`] has refused a run on another
/// machine by then, so this changes the model within the chat's machine.
/// Not saved is logged, not refused: the message is.
pub(super) async fn record_model(
    core: &AppCore,
    conversation_id: i64,
    ran_on: (Option<(u16, i64)>, Option<&ModelRef>),
    name: &str,
) {
    let model = match ran_on {
        (_, Some(far)) => Some(far.clone()),
        (Some((_, model_id)), None) => {
            let registered = core.models().get_by_id(model_id).await.ok().flatten();
            registered.map(|m| ModelRef {
                machine: Machine::Local,
                id: m.id,
            })
        }
        (None, None) => return,
    };
    let recorded = core
        .chat_history()
        .record_model(conversation_id, model, name)
        .await;
    if recorded.is_err() {
        tracing::warn!(
            conversation = conversation_id,
            "an agent run's model was not recorded on its conversation"
        );
    }
}

/// Save the reply to `conversation_id` once the run ends, whatever the end:
/// every row or none. A reply that could not be saved fails the run.
pub(super) fn save_reply(core: Arc<AppCore>, conversation_id: i64, times: FrameTimes) -> RunEnded {
    Box::new(move |info, frames| {
        Box::pin(async move {
            let finished = info.status == RunStatus::Completed;
            let logged = times.lock().clone();
            // One time per frame by construction: the loop records it right
            // after the frame is logged, with no await between the two.
            let at = |i: usize| logged.get(i).copied();
            let with_times = frames.iter().enumerate().map(|(i, f)| (&**f, at(i)));
            let rows = rows_from_timed_frames(with_times, finished, conversation_id);
            let total = rows.len();
            if core.chat_history().save_messages(rows).await.is_err() {
                tracing::warn!(run = %info.id, conversation = conversation_id, rows = total,
                    "an agent run's reply was not saved");
                return Err(RunError {
                    code: "transcript_not_saved".to_owned(),
                    message: "The reply could not be saved to its conversation.".to_owned(),
                });
            }
            tracing::debug!(run = %info.id, conversation = conversation_id, rows = total,
                "an agent run's reply was saved");
            Ok(())
        })
    })
}
