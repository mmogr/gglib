//! How an agent run starts: the id reserved in the caller's scope, the
//! user's message saved, the loop started, and the reply saved when it ends.
//!
//! This machine's door prepares its loop first and reserves after
//! ([`launch`]). A paired device's turn on a hub chat is reserved first
//! ([`launch_turn`]): its model may have to load, or wait behind an image
//! render, for longer than the device waits for its `PUT`, so the load
//! happens inside the run, which says it is waiting. What such a run is
//! refused for after that ends it `failed` with the code the `PUT` would
//! have answered, and it has written nothing. Both write a turn's rows
//! through [`begin_writes`].

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use axum::http::StatusCode;
use futures_util::future::BoxFuture;
use tokio::sync::{OwnedSemaphorePermit, mpsc};

use gglib_app_services::transcript::{FrameTimes, remember_thinking};
use gglib_app_services::{Reservation, RunEnded, RunLog, RunSpec};
use gglib_core::domain::Machine;
use gglib_core::domain::agent::{AgentEvent, WaitingFor};
use gglib_core::domain::runs::{RunError, RunKind};
use gglib_core::domain::thinking::Remember;
use gglib_core::ports::{Created, RunScope};

use super::compose::{Prepared, frame};
use super::remote_upstream;
use super::run::{with_code, work};
use super::transcript::{keep_machine, record_model, save_reply, save_user};
use crate::error::HttpError;
use crate::state::AppState;

/// Where a run's transcript goes, whether the run answers a question
/// already saved there (and so saves no message of its own), and what the
/// conversation is to remember of thinking (`thinking::settle`).
#[derive(Clone, Copy)]
pub(super) struct Transcript {
    pub(super) conversation_id: Option<i64>,
    pub(super) answer_saved: bool,
    pub(super) remember: Remember,
}

/// Reserve the id in `scope`, refuse a run on another machine than its
/// conversation's, save the user's message (naming the device, for a
/// device's run), the model the run uses and the Thinking choice its turn
/// said, and start the loop,
/// in one task of its own: a request dropped part-way cannot split them, so
/// a retry finds the run rather than saving the message, or replacing rows,
/// again. A run that is refused, or whose id is already a run, writes none
/// of them.
pub(super) async fn launch(
    state: &AppState,
    id: &str,
    scope: RunScope,
    transcript: Transcript,
    prepared: Prepared,
    permit: OwnedSemaphorePermit,
) -> Result<Created, HttpError> {
    let task = tokio::spawn(reserve_and_start(
        Arc::clone(state),
        id.to_owned(),
        scope,
        transcript,
        prepared,
        permit,
    ));
    task.await.unwrap_or_else(|_| {
        Err(HttpError::Coded {
            status: StatusCode::INTERNAL_SERVER_ERROR,
            code: "internal_error",
            message: "the run could not be started".to_owned(),
        })
    })
}

async fn reserve_and_start(
    state: AppState,
    id: String,
    scope: RunScope,
    transcript: Transcript,
    mut prepared: Prepared,
    permit: OwnedSemaphorePermit,
) -> Result<Created, HttpError> {
    let conversation_id = transcript.conversation_id;
    let id = id.as_str();
    let state = &state;
    // A device's turn says so, on its message and on each turn of the reply.
    prepared.made_by.device = scope.device().map(str::to_owned);
    let spec = RunSpec {
        kind: RunKind::Agent,
        model: Some(prepared.model.clone()),
        conversation_id,
    };
    let reserved = match state.runs.reserve(scope, id, spec)? {
        Reservation::Existing(info) => {
            return Ok(Created {
                info,
                created: false,
            });
        }
        Reservation::New(reserved) => reserved,
    };
    let times = FrameTimes::new();
    // Dropping `reserved` on the way out leaves no run behind. Held, it
    // keeps any other run off the conversation, so the machine read here is
    // still the conversation's when the model is named.
    begin_writes(state, transcript, &prepared).await?;
    let ended = conversation_id.map_or_else(nothing_to_save, |conversation_id| {
        save_reply(Arc::clone(&state.core), conversation_id, times.clone())
    });
    let info = reserved.start(|log| Box::pin(work(prepared, permit, log, times)), ended);
    tracing::debug!(run = %id, saved = conversation_id.is_some(), "agent run started");
    Ok(Created {
        info,
        created: true,
    })
}

/// An end with nothing to save.
fn nothing_to_save() -> RunEnded {
    Box::new(|_, _| -> BoxFuture<'static, Result<(), RunError>> { Box::pin(async { Ok(()) }) })
}

/// What a run writes to its conversation once it may start, in order: the
/// machine it ran on (refusing a run on another machine than the
/// conversation's), the user's message (naming the device, for a device's
/// run; none for a run that answers a question already saved), the model
/// the run uses, and the Thinking choice its turn said. Nothing for a run
/// saved to no conversation.
///
/// # Errors
///
/// `conflict` (409) for another machine than the conversation's;
/// `internal_error` when the message cannot be saved. A refusal has written
/// no row.
pub(super) async fn begin_writes(
    state: &AppState,
    transcript: Transcript,
    prepared: &Prepared,
) -> Result<(), HttpError> {
    let Transcript {
        conversation_id: Some(conversation_id),
        answer_saved,
        remember,
    } = transcript
    else {
        return Ok(());
    };
    let machine = prepared
        .far_model
        .as_ref()
        .map_or(Machine::Local, |far| far.machine.clone());
    keep_machine(&state.core, conversation_id, &machine).await?;
    if !answer_saved {
        save_user(
            &state.core,
            conversation_id,
            prepared.messages.last(),
            prepared.made_by.device.as_deref(),
        )
        .await?;
    }
    let ran_on = (prepared.local_model, prepared.far_model.as_ref());
    record_model(
        &state.core,
        conversation_id,
        ran_on,
        &prepared.made_by.model,
    )
    .await;
    if let Some(choice) = remember {
        remember_thinking(state.core.chat_history(), conversation_id, choice).await;
    }
    Ok(())
}

/// Told that a turn's model is not running and has to load.
#[derive(Debug, Clone)]
pub(super) struct Loading(mpsc::UnboundedSender<()>);

impl Loading {
    /// The model is about to be loaded: the run says it is waiting.
    pub(super) fn waiting(&self) {
        let _ = self.0.send(());
    }
}

/// What a device's turn does once its run exists: find or load its model
/// and compose its loop, telling `Loading` when the model has to load.
pub(super) type LatePrepare =
    Box<dyn FnOnce(Loading) -> BoxFuture<'static, Result<Prepared, HttpError>> + Send>;

/// A turn whose loop is ready at once, for a test with a scripted loop.
#[cfg(test)]
pub(super) fn ready(prepared: Prepared) -> LatePrepare {
    Box::new(move |_| Box::pin(async move { Ok(prepared) }))
}

/// Start a device's turn: reserve `id` in `scope` for a run on `model`, and
/// only then, as the run's own work, prepare its loop (`late`), hold its
/// model, write its rows ([`begin_writes`]) and run it. The caller's `PUT`
/// is answered as soon as the run is reserved.
///
/// A refusal after the reservation ends the run `failed` with the
/// refusal's code and message, having written no row; its end then saves no
/// reply. A run whose id is already `scope`'s is answered as it stands.
///
/// # Errors
///
/// The reservation's own: `conflict` (409) while the chat has a live
/// reply, and an id another scope holds.
pub(super) fn launch_turn(
    state: &AppState,
    id: &str,
    scope: RunScope,
    model: String,
    transcript: Transcript,
    late: LatePrepare,
    permit: OwnedSemaphorePermit,
) -> Result<Created, HttpError> {
    let spec = RunSpec {
        kind: RunKind::Agent,
        model: Some(model),
        conversation_id: transcript.conversation_id,
    };
    let device = scope.device().map(str::to_owned);
    let reserved = match state.runs.reserve(scope, id, spec)? {
        Reservation::Existing(info) => {
            return Ok(Created {
                info,
                created: false,
            });
        }
        Reservation::New(reserved) => reserved,
    };
    let times = FrameTimes::new();
    // Set once the turn's writes have begun: only then is there a reply to save.
    let written = Arc::new(AtomicBool::new(false));
    let ended = transcript
        .conversation_id
        .map_or_else(nothing_to_save, |conversation_id| {
            let save = save_reply(Arc::clone(&state.core), conversation_id, times.clone());
            let written = Arc::clone(&written);
            let guarded: RunEnded = Box::new(move |info, frames| {
                if written.load(Ordering::SeqCst) {
                    save(info, frames)
                } else {
                    Box::pin(async { Ok(()) })
                }
            });
            guarded
        });
    let turn = Turn {
        state: Arc::clone(state),
        transcript,
        device,
        late,
        permit,
        times,
        written,
    };
    let info = reserved.start(|log| Box::pin(turn.run(log)), ended);
    tracing::debug!(run = %id, "a device's turn was reserved before its model");
    Ok(Created {
        info,
        created: true,
    })
}

/// A device's turn, as its run's work.
struct Turn {
    state: AppState,
    transcript: Transcript,
    device: Option<String>,
    late: LatePrepare,
    permit: OwnedSemaphorePermit,
    times: FrameTimes,
    written: Arc<AtomicBool>,
}

impl Turn {
    async fn run(self, log: RunLog) -> Result<(), RunError> {
        let Self {
            state,
            transcript,
            device,
            late,
            permit,
            times,
            written,
        } = self;
        let (loading, mut waits) = mpsc::unbounded_channel();
        let preparing = late(Loading(loading));
        tokio::pin!(preparing);
        let prepared = loop {
            tokio::select! {
                biased;
                Some(()) = waits.recv() => say_waiting(&log, &times),
                prepared = &mut preparing => break prepared,
            }
        };
        while waits.try_recv().is_ok() {
            say_waiting(&log, &times);
        }
        let mut prepared = prepared.map_err(refused)?;
        // A device's turn says so, on its message and on each turn of the reply.
        prepared.made_by.device = device;
        remote_upstream::hold_model(state.runtime.as_ref(), &mut prepared)
            .await
            .map_err(refused)?;
        begin_writes(&state, transcript, &prepared)
            .await
            .map_err(refused)?;
        written.store(true, Ordering::SeqCst);
        work(prepared, permit, log, times).await
    }
}

/// Log that the run waits for its model to load.
fn say_waiting(log: &RunLog, times: &FrameTimes) {
    let waiting = AgentEvent::Waiting {
        reason: WaitingFor::ModelLoad,
        step: 0,
        total: 0,
        position: 0,
    };
    if log.append(frame(&waiting)).is_ok() {
        times.logged();
    }
}

/// A refusal after the run exists, as the error it ends with: the code and
/// message the `PUT` would have answered.
fn refused(error: HttpError) -> RunError {
    match with_code(error) {
        HttpError::Coded { code, message, .. } => RunError {
            code: code.to_owned(),
            message,
        },
        other => RunError {
            code: "internal_error".to_owned(),
            message: other.to_string(),
        },
    }
}
