//! What an agent run writes to its conversation.
//!
//! The user's message is saved once the run is accepted. When the request
//! names a row to replace (an edit, or a regenerate), that row and every
//! later row are deleted in the same transaction, so a refused run, or one
//! that never got as far, changes nothing. The reply is saved when the run
//! ends, all rows or none.

use std::sync::Arc;

use axum::http::StatusCode;
use gglib_core::domain::agent::{AgentMessage, rows_from_frames, to_new_message};
use gglib_core::domain::runs::{RunError, RunStatus};
use gglib_core::ports::ChatHistoryError;
use gglib_core::services::AppCore;

use gglib_app_services::RunEnded;

use crate::error::HttpError;

fn coded(status: StatusCode, code: &'static str, message: impl Into<String>) -> HttpError {
    HttpError::Coded {
        status,
        code,
        message: message.into(),
    }
}

/// Save the request's last message, when it is the user's, to
/// `conversation_id`; with `replace_from`, in place of that row and every
/// later one, in one transaction.
pub(super) async fn save_user(
    core: &AppCore,
    conversation_id: i64,
    replace_from: Option<i64>,
    last: Option<&AgentMessage>,
) -> Result<(), HttpError> {
    let Some(user @ AgentMessage::User { .. }) = last else {
        return Ok(());
    };
    let row = to_new_message(user, conversation_id);
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
        _ => coded(
            StatusCode::INTERNAL_SERVER_ERROR,
            "internal_error",
            "the user's message could not be saved",
        ),
    })
}

/// Save the reply to `conversation_id` once the run ends, whatever the end:
/// every row or none. A reply that could not be saved fails the run.
pub(super) fn save_reply(core: Arc<AppCore>, conversation_id: i64) -> RunEnded {
    Box::new(move |info, frames| {
        Box::pin(async move {
            let finished = info.status == RunStatus::Completed;
            let rows = rows_from_frames(frames.iter().map(|f| &**f), finished, conversation_id);
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
