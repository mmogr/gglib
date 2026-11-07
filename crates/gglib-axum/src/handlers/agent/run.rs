//! What an agent run shares, whichever door started it (`hub_turn`, for
//! this machine's turn and a paired device's): the codes its refusals
//! carry, how a chat's folder is settled, and the run's work, the loop
//! `/api/agent/chat` runs, owned by the daemon from start to end so closing
//! the page no longer stops the reply. A slot of the agent semaphore (and,
//! for a local model, a hold on it) is kept until the run ends, and each
//! event is logged as the chat route's `data:` text.
//!
//! Nothing here logs or returns a frame, a request body or a tool argument:
//! only ids, statuses and counts, and in a `folder_unusable` refusal the
//! folder the turn said.

use std::path::{Path, PathBuf};

use axum::http::StatusCode;
use tokio::sync::OwnedSemaphorePermit;
use tokio::time::{Duration, Instant};

use gglib_app_services::RunLog;
use gglib_app_services::transcript::FrameTimes;
use gglib_core::domain::agent::AgentEvent;
use gglib_core::domain::folder;
use gglib_core::domain::runs::RunError;
use gglib_core::ports::{AdmissionLease, AgentError};

use super::compose::{Prepared, frame};
use super::launch::RememberFolder;
use crate::error::HttpError;

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

/// A run's folder, from what its request said (`said`) and what its
/// conversation remembers (`saved`), and what the conversation is to
/// remember of it (`launch::Transcript::remember_folder`).
///
/// `""` forgets the folder, and needs no checking: the run has none. A path
/// is the run's once [`folder::chosen`] takes it, and the conversation's
/// from then on, by its canonical path. Absent, the run reads the folder
/// the conversation remembers, checked again, since it may be gone. Without
/// a conversation, a path is this run's alone and `""` is the same as
/// absent.
///
/// # Errors
///
/// `folder_unusable` (400) for a said path that is not an absolute path to
/// a folder here, or, in a conversation, whose canonical path is not UTF-8
/// and so cannot be remembered; `folder_missing` (409) for a remembered one that no longer
/// resolves to a folder. Either is refused before anything is written.
pub(super) fn settle_folder(
    said: Option<&str>,
    conversation_id: Option<i64>,
    saved: Option<&str>,
) -> Result<(Option<PathBuf>, RememberFolder), HttpError> {
    let remembers = conversation_id.is_some();
    match said {
        Some("") => Ok((None, remembers.then_some(None))),
        Some(path) => {
            let unusable =
                |message: String| coded(StatusCode::BAD_REQUEST, "folder_unusable", message);
            let chosen = folder::chosen(Path::new(path)).map_err(|e| unusable(e.to_string()))?;
            // Only a folder that is remembered must be text, to be stored.
            if !remembers {
                return Ok((Some(chosen), None));
            }
            let shown = chosen.to_str().map(str::to_owned).ok_or_else(|| {
                unusable(format!("'{path}' resolves to a path that is not UTF-8"))
            })?;
            Ok((Some(chosen), Some(Some(shown))))
        }
        None => {
            let Some(path) = saved else {
                return Ok((None, None));
            };
            let chosen = folder::chosen(Path::new(path)).map_err(|e| {
                coded(
                    StatusCode::CONFLICT,
                    "folder_missing",
                    format!("the conversation's folder is gone: {e}; choose another or clear it"),
                )
            })?;
            Ok((Some(chosen), None))
        }
    }
}

/// How often a held run reports progress on its hold: each report takes the
/// queue's lock and wakes every waiter, so not once per event.
pub(super) const PROGRESS_EVERY: Duration = Duration::from_secs(5);

/// Run the loop, logging each event as the chat route frames it. Dropped
/// when the run is cancelled, which aborts the loop and any tool call in
/// flight and releases the permit and the model's hold.
///
/// Each event is progress on the hold, reported at most once every
/// [`PROGRESS_EVERY`], so a request waiting behind the run is not given up
/// as stalled while the run streams.
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
        hold,
        ..
    } = prepared;
    let progress = hold.as_ref().map(AdmissionLease::progress_handle);
    let run = async move {
        let outcome = agent_loop.run(messages, config, tx).await;
        // Its retry notices hold a sender, so the events end only once the
        // loop itself is gone.
        drop(agent_loop);
        outcome
    };
    let forward = async {
        let mut first = true;
        let mut reported: Option<Instant> = None;
        while let Some(mut event) = rx.recv().await {
            if let Some(progress) = &progress
                && reported.is_none_or(|at| at.elapsed() >= PROGRESS_EVERY)
            {
                reported = Some(Instant::now());
                progress.report();
            }
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
///
/// A first reply that had to call a tool and did not call it is the one error
/// that carries its words: they are the composer's, fixed, and only a run
/// sent with Draw is held to a first call, so its code is the one a picture
/// that was not drawn has.
pub(super) fn run_error(error: &AgentError) -> RunError {
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
        AgentError::FirstCallMissing { message } => ("image_generation_failed", message.as_str()),
        AgentError::TurnTooLong { .. } => (
            "turn_too_long",
            "The model's turn ran too long and was cut off, so the run was stopped.",
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
