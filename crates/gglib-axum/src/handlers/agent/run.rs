//! An agent run: `/api/agent/chat`'s loop, owned by the daemon from start to
//! end, so closing the page no longer stops the reply.
//!
//! Started only here, at the daemon's own door (`PUT /api/runs/{id}?kind=agent`);
//! the proxy's door holds only `RunsPort`, whose create makes chat runs. The
//! request is prepared exactly as the chat route prepares it, and a slot of
//! the same semaphore is held until the run ends. Each event is logged as
//! the route's `data:` text. With a `conversation_id`, the user's message is
//! saved when the run is created and the reply when it ends, whatever the
//! end, rebuilt from the logged events.
//!
//! Nothing here logs or returns a frame, a request body or a tool argument:
//! only ids, statuses and counts.

use std::sync::Arc;

use axum::http::StatusCode;
use futures_util::future::BoxFuture;
use serde_json::Value;
use tokio::sync::OwnedSemaphorePermit;

use gglib_app_services::{Reservation, RunEnded, RunLog, RunSpec};
use gglib_core::domain::agent::{AgentMessage, rows_from_frames, to_new_message};
use gglib_core::domain::runs::{RunError, RunKind, RunStatus};
use gglib_core::ports::{AgentError, Created};
use gglib_core::services::AppCore;

use super::compose::{Prepared, frame, prepare, take_permit};
use super::dto::AgentRunRequest;
use crate::error::HttpError;
use crate::state::AppState;

fn coded(status: StatusCode, code: &'static str, message: impl Into<String>) -> HttpError {
    HttpError::Coded {
        status,
        code,
        message: message.into(),
    }
}

/// The chat route's refusals, with the code a run's client matches on.
fn with_code(error: HttpError) -> HttpError {
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
/// `agent_busy` (429) when every agent slot is taken; and the runs' own.
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
    if let Some(info) = state.runs.existing(id)? {
        return Ok(Created {
            info,
            created: false,
        });
    }
    if let Some(conversation_id) = req.conversation_id {
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
        if found.is_none() {
            return Err(coded(
                StatusCode::NOT_FOUND,
                "conversation_not_found",
                format!("no conversation has id {conversation_id}"),
            ));
        }
    }
    let permit = take_permit(state).ok_or_else(|| {
        coded(
            StatusCode::TOO_MANY_REQUESTS,
            "agent_busy",
            "all agent loop slots are in use; try again later",
        )
    })?;
    let prepared = prepare(state, req.chat).await.map_err(with_code)?;
    launch(state, id, req.conversation_id, prepared, permit).await
}

/// Reserve the id, save the user's message, and start the loop.
pub(super) async fn launch(
    state: &AppState,
    id: &str,
    conversation_id: Option<i64>,
    prepared: Prepared,
    permit: OwnedSemaphorePermit,
) -> Result<Created, HttpError> {
    let spec = RunSpec {
        kind: RunKind::Agent,
        model: Some(prepared.model.clone()),
        conversation_id,
    };
    let reserved = match state.runs.reserve(id, spec)? {
        Reservation::Existing(info) => {
            return Ok(Created {
                info,
                created: false,
            });
        }
        Reservation::New(reserved) => reserved,
    };
    let ended = match conversation_id {
        Some(conversation_id) => {
            if let Some(user @ AgentMessage::User { .. }) = prepared.messages.last() {
                let row = to_new_message(user, conversation_id);
                // Dropping `reserved` on the way out leaves no run behind.
                state
                    .core
                    .chat_history()
                    .save_message(row)
                    .await
                    .map_err(|_| {
                        coded(
                            StatusCode::INTERNAL_SERVER_ERROR,
                            "internal_error",
                            "the user's message could not be saved",
                        )
                    })?;
            }
            save_reply(Arc::clone(&state.core), conversation_id)
        }
        None => Box::new(|_, _| -> BoxFuture<'static, Result<(), RunError>> {
            Box::pin(async { Ok(()) })
        }),
    };
    let info = reserved.start(|log| Box::pin(work(prepared, permit, log)), ended);
    tracing::debug!(run = %id, saved = conversation_id.is_some(), "agent run started");
    Ok(Created {
        info,
        created: true,
    })
}

/// Run the loop, logging each event as the chat route frames it. Dropped
/// when the run is cancelled, which aborts the loop and any tool call in
/// flight and releases the permit.
async fn work(
    prepared: Prepared,
    permit: OwnedSemaphorePermit,
    log: RunLog,
) -> Result<(), RunError> {
    let _permit = permit;
    let Prepared {
        agent_loop,
        messages,
        config,
        tx,
        mut rx,
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
        while let Some(event) = rx.recv().await {
            if std::mem::take(&mut first) {
                log.started();
            }
            if log.append(frame(&event)).is_err() {
                break;
            }
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

/// Save the reply to `conversation_id` once the run ends, whatever the end.
fn save_reply(core: Arc<AppCore>, conversation_id: i64) -> RunEnded {
    Box::new(move |info, frames| {
        Box::pin(async move {
            let finished = info.status == RunStatus::Completed;
            let rows = rows_from_frames(frames.iter().map(|f| &**f), finished, conversation_id);
            let total = rows.len();
            let mut failed = 0_usize;
            for row in rows {
                if core.chat_history().save_message(row).await.is_err() {
                    failed += 1;
                }
            }
            if failed > 0 {
                tracing::warn!(run = %info.id, conversation = conversation_id, failed, total,
                    "an agent run's reply was not fully saved");
            } else {
                tracing::debug!(run = %info.id, conversation = conversation_id, rows = total,
                    "an agent run's reply was saved");
            }
            Ok(())
        })
    })
}
