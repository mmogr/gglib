//! How an agent run starts: the id reserved in the caller's scope, the
//! user's message saved, the loop started, and the reply saved when it ends.
//!
//! A turn on one of this machine's chats, a paired device's or this
//! machine's own, is reserved first ([`launch_turn`]): its model may have
//! to load, or wait behind an image render, for longer than the client
//! waits for its `PUT`, so the load happens inside the run, which says it
//! is waiting. What such a run is refused for after that ends it `failed`
//! with the code the `PUT` would have answered, and it has written nothing.
//! A turn's rows are written through [`begin_writes`].

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use futures_util::future::BoxFuture;
use tokio::sync::{OwnedSemaphorePermit, mpsc};
use tokio::time::{Duration, Instant, MissedTickBehavior};

use gglib_app_services::transcript::{FrameTimes, remember_folder, remember_thinking};
use gglib_app_services::{Reservation, RunEnded, RunLog, RunSpec};
use gglib_core::domain::Machine;
use gglib_core::domain::agent::AgentEvent;
use gglib_core::domain::runs::{RunError, RunKind};
use gglib_core::domain::thinking::Remember;
use gglib_core::ports::{Created, RunScope};

use super::compose::{Prepared, frame};
use super::run::{with_code, work};
use super::transcript::{keep_machine, record_model, save_reply, save_user};
use crate::error::HttpError;
use crate::state::AppState;

/// Where a run's transcript goes, whether the run answers a question
/// already saved there (and so saves no message of its own), what the
/// conversation is to remember of thinking (`thinking::settle`), and of
/// its folder.
#[derive(Clone)]
pub(super) struct Transcript {
    pub(super) conversation_id: Option<i64>,
    pub(super) answer_saved: bool,
    pub(super) remember: Remember,
    /// The folder the conversation is to remember once the run starts.
    /// Only this machine's own turn says one.
    pub(super) remember_folder: RememberFolder,
}

/// What a chat is to remember of its folder once a run starts: `None`
/// writes nothing, `Some(folder)` sets it to `folder`, a canonical path,
/// and `Some(None)` forgets it.
pub(super) type RememberFolder = Option<Option<String>>;

/// An end with nothing to save.
fn nothing_to_save() -> RunEnded {
    Box::new(|_, _| -> BoxFuture<'static, Result<(), RunError>> { Box::pin(async { Ok(()) }) })
}

/// What a run writes to its conversation once it may start, in order: the
/// machine it ran on (refusing a run on another machine than the
/// conversation's), the user's message (naming the device, for a device's
/// run; none for a run that answers a question already saved), the model
/// the run uses, the Thinking choice its turn said, and the folder it said.
/// Nothing for a run saved to no conversation.
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
        remember_folder: folder,
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
    if let Some(folder) = folder {
        remember_folder(state.core.chat_history(), conversation_id, folder).await;
    }
    Ok(())
}

/// Told that a turn's model is not running and has to load.
#[derive(Debug, Clone)]
pub(super) struct Loading(mpsc::UnboundedSender<String>);

impl Loading {
    /// `model`, by the name the catalogue shows, is about to be loaded: the
    /// run says it is waiting for it, and says so again each
    /// [`LOAD_TICK`] with how long the load has taken.
    pub(super) fn waiting(&self, model: &str) {
        let _ = self.0.send(model.to_owned());
    }
}

/// How often a run waiting for its model to load says so again, with the
/// time the load has taken: a client that joins part-way can then show
/// "Loading Qwen3-4B… 12 s" to the second.
pub(super) const LOAD_TICK: Duration = Duration::from_secs(1);

/// What a turn does once its run exists: find or load its model and
/// compose its loop, telling `Loading` when the model has to load.
pub(super) type LatePrepare =
    Box<dyn FnOnce(Loading) -> BoxFuture<'static, Result<Prepared, HttpError>> + Send>;

/// A turn whose loop is ready at once, for a test with a scripted loop.
#[cfg(test)]
pub(super) fn ready(prepared: Prepared) -> LatePrepare {
    Box::new(move |_| Box::pin(async move { Ok(prepared) }))
}

/// Start a turn: reserve `id` in `scope` for a run of `kind` (an agent run
/// for a turn on a hub chat, a chat run for a chat a device keeps itself;
/// either logs the agent loop's events) on `model`, and only then, as the
/// run's own work, load its model and prepare its loop (`late`), write its
/// rows ([`begin_writes`]) and run it. The caller's `PUT` is answered as
/// soon as the run is reserved.
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
    (kind, model): (RunKind, String),
    transcript: Transcript,
    late: LatePrepare,
    permit: OwnedSemaphorePermit,
) -> Result<Created, HttpError> {
    let spec = RunSpec {
        kind,
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
        // Whatever its kind, this run logs the agent loop's events.
        Reservation::New(reserved) => reserved.agent_frames(),
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
    tracing::debug!(run = %id, "a turn was reserved before its model");
    Ok(Created {
        info,
        created: true,
    })
}

/// A turn, as its run's work.
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
        // The model being loaded, and since when.
        let mut load: Option<(String, Instant)> = None;
        let mut tick = tokio::time::interval(LOAD_TICK);
        tick.set_missed_tick_behavior(MissedTickBehavior::Delay);
        let prepared = loop {
            tokio::select! {
                biased;
                Some(model) = waits.recv() => {
                    let since = Instant::now();
                    say_waiting(&log, &times, &model, since);
                    load = Some((model, since));
                    tick.reset();
                }
                _ = tick.tick(), if load.is_some() => {
                    if let Some((model, since)) = &load {
                        say_waiting(&log, &times, model, *since);
                    }
                }
                prepared = &mut preparing => break prepared,
            }
        };
        while let Ok(model) = waits.try_recv() {
            say_waiting(&log, &times, &model, Instant::now());
        }
        let mut prepared = prepared.map_err(refused)?;
        // A device's turn says so, on its message and on each turn of the reply.
        prepared.made_by.device = device;
        begin_writes(&state, transcript, &prepared)
            .await
            .map_err(refused)?;
        written.store(true, Ordering::SeqCst);
        work(prepared, permit, log, times).await
    }
}

/// Log that the run waits for `model` to load, which began at `since`.
fn say_waiting(log: &RunLog, times: &FrameTimes, model: &str, since: Instant) {
    let elapsed = u64::try_from(since.elapsed().as_millis()).unwrap_or(u64::MAX);
    let waiting = AgentEvent::model_load_wait(model, elapsed);
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
