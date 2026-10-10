//! An agent run: `/api/agent/chat`'s loop, owned by the daemon from start to
//! end, so closing the page no longer stops the reply.
//!
//! Started only here, at the daemon's own door (`PUT /api/runs/{id}?kind=agent`);
//! the proxy's door holds only `RunsPort`, whose create makes chat runs. The
//! request is prepared exactly as the chat route prepares it, and a slot of
//! the same semaphore (and, for a local model, a hold on it) is kept until
//! the run ends. Each event is logged as the route's `data:` text. With a
//! `conversation_id`, the user's message is saved when the run is created
//! and the reply when it ends, whatever the end, rebuilt from the logged
//! events: see `transcript`. A run that answers the question the
//! conversation ends in (`answer_saved`) saves no message of its own and
//! runs from the conversation's saved history.
//!
//! Nothing here logs or returns a frame, a request body or a tool argument:
//! only ids, statuses and counts.

use axum::http::StatusCode;
use serde_json::Value;
use tokio::sync::OwnedSemaphorePermit;

use gglib_app_services::RunLog;
use gglib_app_services::transcript::{FrameTimes, answer_history};
use gglib_core::domain::agent::AgentEvent;
use gglib_core::domain::runs::RunError;
use gglib_core::domain::thinking;
use gglib_core::ports::{AgentError, Created, RunScope};

use super::AgentChatRequest;
use super::compose::{Prepared, frame, prepare, refuse_unavailable_drawing, take_permit};
use super::dto::AgentRunRequest;
use super::launch::{Transcript, launch};
use super::remote_upstream;
use crate::error::HttpError;
use crate::state::AppState;

pub(super) fn coded(
    status: StatusCode,
    code: &'static str,
    message: impl Into<String>,
) -> HttpError {
    HttpError::Coded {
        status,
        code,
        message: message.into(),
    }
}

/// The chat route's refusals, with the code a run's client matches on.
pub(super) fn with_code(error: HttpError) -> HttpError {
    match error {
        HttpError::BadRequest(m) => coded(StatusCode::BAD_REQUEST, "invalid_request", m),
        HttpError::NotFound(m) => coded(StatusCode::NOT_FOUND, "not_found", m),
        HttpError::Conflict(m) => coded(StatusCode::CONFLICT, "conflict", m),
        HttpError::ServiceUnavailable(m) => {
            coded(StatusCode::SERVICE_UNAVAILABLE, "unavailable", m)
        }
        HttpError::Internal(m) => coded(StatusCode::INTERNAL_SERVER_ERROR, "internal_error", m),
        other => other,
    }
}

/// `PUT /api/runs/{id}?kind=agent`: start an agent run, or answer with this
/// machine's run that already has the id.
///
/// # Errors
///
/// `invalid_request` for a body that is not an agent chat request, and
/// whatever the chat route refuses, coded; `conversation_not_found` (404);
/// `attachment_not_found` (400) for an image a message names, history
/// included, that is not stored, and `request_images_too_large` (400) when
/// they are over 16 MiB together; `drawing_unavailable` (400) for `draw` on
/// a machine that cannot draw for it; `agent_busy` (429) when every agent slot
/// is taken; `nothing_to_answer` (409) for an answer run on a conversation
/// that does not end in a question with no reply; `conflict` (409)
/// while the conversation has a live reply, or when it ran on another
/// machine than the request's; and the runs' own. A refusal writes nothing.
pub(crate) async fn create_run(
    state: &AppState,
    id: &str,
    body: Value,
) -> Result<Created, HttpError> {
    // Fixed text and a position: serde's own message can quote the body.
    let req: AgentRunRequest = serde_json::from_value(body).map_err(|e| {
        coded(
            StatusCode::BAD_REQUEST,
            "invalid_request",
            format!(
                "an agent run's body is an agent chat request (line {}, column {})",
                e.line(),
                e.column()
            ),
        )
    })?;
    if let Some(info) = state.runs.existing(&RunScope::Local, id)? {
        return Ok(Created {
            info,
            created: false,
        });
    }
    let (chat, transcript) = plan(state, req).await?;
    // Before a slot is taken or a model is held, for a run on this
    // machine's model or the paired machine's: their images are read here.
    state
        .core
        .attachments()
        .check_request(&chat.messages)
        .await?;
    // Likewise before a slot is taken, and before anything is written: a
    // message sent with Draw pressed that cannot draw here.
    refuse_unavailable_drawing(state, &chat).await?;
    let permit = take_permit(state).ok_or_else(|| {
        coded(
            StatusCode::TOO_MANY_REQUESTS,
            "agent_busy",
            "all agent loop slots are in use; try again later",
        )
    })?;
    let mut prepared = prepare(state, chat).await.map_err(with_code)?;
    // Untested: `create_run` cannot be driven without a running llama-server.
    remote_upstream::hold_model(state.runtime.as_ref(), &mut prepared).await?;
    launch(state, id, RunScope::Local, transcript, prepared, permit).await
}

/// A run's request read against the conversation it names: the chat request
/// with the thinking budget the run uses (`thinking::settle`, over what the
/// request says, what the conversation remembers and the request's own
/// budget) and, where it names no iteration limit, the one the conversation
/// saved; and what the run writes to the conversation once it starts.
///
/// # Errors
///
/// `invalid_request` (400) for an answer run with no conversation, or one
/// that sends messages of its own; `conversation_not_found` (404);
/// `nothing_to_answer` (409); `internal_error` when the conversation cannot
/// be read.
pub(super) async fn plan(
    state: &AppState,
    req: AgentRunRequest,
) -> Result<(AgentChatRequest, Transcript), HttpError> {
    let AgentRunRequest {
        mut chat,
        conversation_id,
        answer_saved,
        thinking: said,
    } = req;
    if answer_saved && (conversation_id.is_none() || !chat.messages.is_empty()) {
        return Err(coded(
            StatusCode::BAD_REQUEST,
            "invalid_request",
            "an answer run names its conversation and sends no messages: it answers the \
             conversation's own",
        ));
    }
    let saved = if let Some(conversation_id) = conversation_id {
        let found = state
            .core
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
        let Some(conversation) = found else {
            return Err(coded(
                StatusCode::NOT_FOUND,
                "conversation_not_found",
                format!("no conversation has id {conversation_id}"),
            ));
        };
        if answer_saved {
            let history = state.core.chat_history();
            chat.messages = answer_history(history, conversation_id).await?;
        }
        conversation.settings
    } else {
        None
    };
    let remembered = saved.as_ref().and_then(|settings| settings.thinking);
    let settled = thinking::settle(said, remembered, chat.reasoning_budget_tokens);
    chat.reasoning_budget_tokens = settled.budget;
    // The conversation's saved limit stands in where the request names none,
    // as a device's turn has it (`hub_turn::config_of`) and the CLI's resume;
    // `config_for` then gives the stored setting and the default their turn.
    if let Some(limit) = saved.and_then(|settings| settings.max_iterations) {
        let config = chat.config.get_or_insert_default();
        config.max_iterations.get_or_insert(limit);
    }
    let transcript = Transcript {
        conversation_id,
        answer_saved,
        remember: settled.remember,
    };
    Ok((chat, transcript))
}

/// Run the loop, logging each event as the chat route frames it. Dropped
/// when the run is cancelled, which aborts the loop and any tool call in
/// flight and releases the permit and the model's hold.
///
/// A tool's preview frame is never logged: it is kept beside the log as the
/// run's latest (`RunLog::preview`) and forgotten in the step that logs that
/// call's own completion (`RunLog::append_completing`); another call
/// finishing leaves it in place.
pub(super) async fn work(
    prepared: Prepared,
    permit: OwnedSemaphorePermit,
    log: RunLog,
    times: FrameTimes,
) -> Result<(), RunError> {
    let _permit = permit;
    let Prepared {
        agent_loop,
        messages,
        config,
        tx,
        mut rx,
        made_by,
        hold: _hold,
        ..
    } = prepared;
    let run = async move {
        let outcome = agent_loop.run(messages, config, tx).await;
        // Its retry notices hold a sender, so the events end only once the
        // loop itself is gone.
        drop(agent_loop);
        outcome
    };
    let forward = async {
        let mut first = true;
        while let Some(mut event) = rx.recv().await {
            if std::mem::take(&mut first) {
                log.started();
            }
            if let AgentEvent::ToolPreview {
                tool_call_id,
                frame: preview,
            } = &event
            {
                log.preview(tool_call_id, preview);
                continue;
            }
            made_by.stamp(&mut event);
            // A call's completion and the end of its preview are one step,
            // so no reader gets the frame after reading the completion.
            let logged = match &event {
                AgentEvent::ToolCallComplete { result, .. } => {
                    log.append_completing(frame(&event), &result.tool_call_id)
                }
                _ => log.append(frame(&event)),
            };
            if logged.is_err() {
                break;
            }
            times.logged();
        }
    };
    let (outcome, ()) = tokio::join!(run, forward);
    outcome.map(|_| ()).map_err(|e| run_error(&e))
}

/// The error a failed loop ends its run with: fixed text, since the loop's
/// own message can quote the model or a tool. The reason in full is the
/// run's last `error` event.
fn run_error(error: &AgentError) -> RunError {
    let (code, message) = match error {
        AgentError::MaxIterationsReached(_) => (
            "max_iterations",
            "The agent reached its iteration limit without a final answer.",
        ),
        AgentError::LoopDetected { .. } => (
            "loop_detected",
            "The agent repeated the same tool calls, so it was stopped.",
        ),
        AgentError::ParallelToolLimitExceeded { .. } => (
            "too_many_tool_calls",
            "The model asked for more tool calls at once than are allowed.",
        ),
        AgentError::StagnationDetected { .. } => (
            "stagnation_detected",
            "The agent kept giving the same reply, so it was stopped.",
        ),
        AgentError::Internal(_) => (
            "agent_error",
            "The agent loop failed; the reply's last event says why.",
        ),
    };
    RunError {
        code: code.to_owned(),
        message: message.to_owned(),
    }
}
