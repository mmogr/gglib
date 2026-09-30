//! An agent run: `/api/agent/chat`'s loop, owned by the daemon from start to
//! end, so closing the page no longer stops the reply.
//!
//! Started only here, at the daemon's own door (`PUT /api/runs/{id}?kind=agent`);
//! the proxy's door holds only `RunsPort`, whose create makes chat runs. The
//! request is prepared exactly as the chat route prepares it, and a slot of
//! the same semaphore (and, for a local model, a hold on it) is kept until
//! the run ends. Each event is logged as the route's `data:` text. With a
//! `conversation_id`, the user's message is saved when the run is created
//! (in place of the rows from `replace_from` on, when the request names
//! one) and the reply when it ends, whatever the end, rebuilt from the
//! logged events: see `transcript`.
//!
//! Nothing here logs or returns a frame, a request body or a tool argument:
//! only ids, statuses and counts.

use std::sync::Arc;

use axum::http::StatusCode;
use futures_util::future::BoxFuture;
use serde_json::Value;
use tokio::sync::OwnedSemaphorePermit;

use gglib_app_services::{Reservation, RunLog, RunSpec};
use gglib_core::domain::agent::AgentMessage;
use gglib_core::domain::runs::{RunError, RunKind};
use gglib_core::ports::{AgentError, Created};

use super::compose::{Prepared, frame, prepare, take_permit};
use super::dto::AgentRunRequest;
use super::remote_upstream;
use super::transcript::{FrameTimes, save_reply, save_user};
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
/// `agent_busy` (429) when every agent slot is taken; `message_not_found`
/// (404) for a `replace_from` not in the conversation; `conflict` (409)
/// while the conversation has a live reply; and the runs' own. A refusal
/// writes nothing.
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
    if req.replace_from.is_some()
        && (req.conversation_id.is_none()
            || !matches!(req.chat.messages.last(), Some(AgentMessage::User { .. })))
    {
        return Err(coded(
            StatusCode::BAD_REQUEST,
            "invalid_request",
            "replace_from needs a conversation_id and a last message that is the user's",
        ));
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
    let (remote, port) = (req.chat.remote, req.chat.port);
    let mut prepared = prepare(state, req.chat).await.map_err(with_code)?;
    prepared.hold = remote_upstream::hold(state.runtime.as_ref(), remote, port);
    let transcript = Transcript {
        conversation_id: req.conversation_id,
        replace_from: req.replace_from,
    };
    launch(state, id, transcript, prepared, permit).await
}

/// Where a run's transcript goes, and the rows its user's message replaces.
#[derive(Clone, Copy)]
pub(super) struct Transcript {
    pub(super) conversation_id: Option<i64>,
    pub(super) replace_from: Option<i64>,
}

/// Reserve the id, save the user's message, and start the loop, in one
/// task of its own: a request dropped part-way cannot split them, so a
/// retry finds the run rather than saving the message, or replacing rows,
/// again.
pub(super) async fn launch(
    state: &AppState,
    id: &str,
    transcript: Transcript,
    prepared: Prepared,
    permit: OwnedSemaphorePermit,
) -> Result<Created, HttpError> {
    let task = tokio::spawn(reserve_and_start(
        Arc::clone(state),
        id.to_owned(),
        transcript,
        prepared,
        permit,
    ));
    task.await.unwrap_or_else(|_| {
        Err(coded(
            StatusCode::INTERNAL_SERVER_ERROR,
            "internal_error",
            "the run could not be started",
        ))
    })
}

async fn reserve_and_start(
    state: AppState,
    id: String,
    transcript: Transcript,
    prepared: Prepared,
    permit: OwnedSemaphorePermit,
) -> Result<Created, HttpError> {
    let Transcript {
        conversation_id,
        replace_from,
    } = transcript;
    let id = id.as_str();
    let state = &state;
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
    let times = FrameTimes::new();
    let ended = match conversation_id {
        Some(conversation_id) => {
            // Dropping `reserved` on the way out leaves no run behind.
            save_user(
                &state.core,
                conversation_id,
                replace_from,
                prepared.messages.last(),
            )
            .await?;
            save_reply(Arc::clone(&state.core), conversation_id, times.clone())
        }
        None => Box::new(|_, _| -> BoxFuture<'static, Result<(), RunError>> {
            Box::pin(async { Ok(()) })
        }),
    };
    let info = reserved.start(|log| Box::pin(work(prepared, permit, log, times)), ended);
    tracing::debug!(run = %id, saved = conversation_id.is_some(), "agent run started");
    Ok(Created {
        info,
        created: true,
    })
}

/// Run the loop, logging each event as the chat route frames it. Dropped
/// when the run is cancelled, which aborts the loop and any tool call in
/// flight and releases the permit and the model's hold.
async fn work(
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
            made_by.stamp(&mut event);
            if log.append(frame(&event)).is_err() {
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
