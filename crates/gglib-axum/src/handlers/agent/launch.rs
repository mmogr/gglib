//! How an agent run starts once its loop is prepared: the id reserved in
//! the caller's scope, the user's message saved, the loop started, and the
//! reply saved when it ends. Shared by this machine's door and a paired
//! device's turn on a hub chat.

use std::sync::Arc;

use axum::http::StatusCode;
use futures_util::future::BoxFuture;
use tokio::sync::OwnedSemaphorePermit;

use gglib_app_services::{Reservation, RunSpec};
use gglib_core::domain::Machine;
use gglib_core::domain::runs::{RunError, RunKind};
use gglib_core::ports::{Created, RunScope};

use super::compose::Prepared;
use super::run::work;
use super::transcript::{FrameTimes, keep_machine, record_model, save_reply, save_user};
use crate::error::HttpError;
use crate::state::AppState;

/// Where a run's transcript goes, and the rows its user's message replaces.
#[derive(Clone, Copy)]
pub(super) struct Transcript {
    pub(super) conversation_id: Option<i64>,
    pub(super) replace_from: Option<i64>,
}

/// Reserve the id in `scope`, refuse a run on another machine than its
/// conversation's, save the user's message (naming the device, for a
/// device's run) and the model the run uses, and start the loop,
/// in one task of its own: a request dropped part-way cannot split them, so
/// a retry finds the run rather than saving the message, or replacing rows,
/// again.
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
    let Transcript {
        conversation_id,
        replace_from,
    } = transcript;
    let id = id.as_str();
    let state = &state;
    // A device's turn says so, on its message and on each turn of the reply.
    let device = scope.device().map(str::to_owned);
    prepared.made_by.device.clone_from(&device);
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
    let ended = match conversation_id {
        Some(conversation_id) => {
            // Dropping `reserved` on the way out leaves no run behind. Held,
            // it keeps any other run off the conversation, so the machine
            // read here is still the conversation's when the model is named.
            let machine = prepared
                .far_model
                .as_ref()
                .map_or(Machine::Local, |far| far.machine.clone());
            keep_machine(&state.core, conversation_id, &machine).await?;
            save_user(
                &state.core,
                conversation_id,
                replace_from,
                prepared.messages.last(),
                device.as_deref(),
            )
            .await?;
            let ran_on = (prepared.local_model, prepared.far_model.as_ref());
            record_model(
                &state.core,
                conversation_id,
                ran_on,
                &prepared.made_by.model,
            )
            .await;
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
