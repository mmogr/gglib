//! A turn on one of this machine's chats: a paired device's, at the proxy's
//! `PUT /v1/gglib/runs/{id}?kind=agent`, or this machine's own, in its own scope
//! ([`RunScope::Local`]). Both run here because the agent loop is composed
//! here, and both go through [`start`].
//!
//! The turn sends only its message, with any image named by the id its
//! upload answered, or, after a change that leaves the chat ending in a
//! question (ADR 0017), no message and `answer_saved`, and the reply
//! answers that question. The hub rebuilds the history from its
//! own record, as the chat page would send it: the conversation's system
//! prompt, then every saved row but a system one, then the new message,
//! with the limits the conversation's settings name (`prepare_over` takes an
//! iteration limit they do not name from this machine's settings), and with
//! thinking off
//! when the turn says so or the chat remembers it (`thinking`). The reply
//! runs on the chat's model, loaded as `/v1/gglib/models/{name}/load` loads it
//! when it is not running, as an agent run in the turn's scope whose every
//! request goes through this machine's endpoint, saved to the chat.
//!
//! The scopes differ in three things only:
//!
//! - **Model.** A chat that stored the paired machine's model runs there on
//!   this machine's own turn, through the tunnel; a device's turn on it is
//!   refused once the chat has run (`hub_model`).
//! - **Tools.** A device's turn calls no MCP tool but those of the servers
//!   open to devices (ADR 0025), narrowed by the tools the chat's settings
//!   name, and the image tool with `draw` (`device_tools`). This machine's
//!   own turn is offered every tool here, narrowed by the same list, and
//!   none when the chat turned them off or the catalogue says its model
//!   calls none (`Asked::of_local`).
//! - **Folder.** This machine's own turn may say the chat's folder, settled
//!   by `run::settle_folder`, and gglib's file
//!   tools read there. A device's turn never has one, whatever its chat
//!   remembers (ADR 0023), and is refused one it says.
//!
//! Nothing here logs or returns the message, a row or a title: only ids.

use std::collections::BTreeSet;
use std::path::PathBuf;
use std::sync::{Arc, Weak};

use async_trait::async_trait;
use axum::http::StatusCode;
use axum::response::IntoResponse as _;

use gglib_core::domain::Machine;
use gglib_core::domain::agent::{AgentMessage, saved_history};
use gglib_core::domain::branching;
use gglib_core::domain::chat::ConversationSettings;
use gglib_core::domain::hub_chats::{HubTurn, LocalTurn};
use gglib_core::domain::runs::RunKind;
use gglib_core::domain::thinking;
use gglib_core::ports::{AgentRunStarter, Created, RunScope, RunsError, TurnRefused};
use gglib_core::services::ChangeError;
use tokio::sync::OwnedSemaphorePermit;

use super::chat_sampling::ChatSampling;
use super::chat_turn::{ChatQuery, Drawing};
use super::compose::{prepare_over, refuse_unavailable_drawing, take_permit};
use super::device_tools::{self, Asked};
use super::hub_model::{calls_tools, on_model, paired, turn_model};
use super::launch::{LatePrepare, Transcript, launch_turn};
use super::loop_request::{LoopRequest, NamedLimits};
use super::remote_upstream;
use super::run::{coded, settle_folder, with_code};
use crate::bootstrap::AxumContext;
use crate::error::HttpError;
use crate::state::AppState;

/// The daemon's [`AgentRunStarter`]. Weak, because the proxy it is handed
/// to belongs to the same context.
pub(crate) struct HubTurns(Weak<AxumContext>);

impl std::fmt::Debug for HubTurns {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("HubTurns").finish_non_exhaustive()
    }
}

#[cfg(test)]
impl HubTurns {
    /// The starter [`bind`] hands a proxy, for a test that drives it as the
    /// proxy does.
    pub(super) fn of(state: &AppState) -> Self {
        Self(Arc::downgrade(state))
    }
}

/// Hand every proxy this daemon starts what starts a device's turn.
pub(crate) fn bind(state: &AppState) {
    state
        .proxy
        .bind_turns(Arc::new(HubTurns(Arc::downgrade(state))));
}

#[async_trait]
impl AgentRunStarter for HubTurns {
    async fn start(&self, device: &str, id: &str, turn: HubTurn) -> Result<Created, TurnRefused> {
        let Some(state) = self.0.upgrade() else {
            return Err(refusal(RunsError::ShuttingDown.into()));
        };
        let scope = RunScope::Device(device.to_owned());
        // A device's turn says no folder: `HubTurn` has no key for one.
        start(&state, scope, id, turn, None).await.map_err(refusal)
    }

    async fn start_chat(
        &self,
        device: &str,
        id: &str,
        body: serde_json::Value,
        draw: bool,
        image_model: Option<u32>,
        servers: BTreeSet<i64>,
    ) -> Result<Created, TurnRefused> {
        let Some(state) = self.0.upgrade() else {
            return Err(refusal(RunsError::ShuttingDown.into()));
        };
        let query = ChatQuery {
            drawing: Drawing { draw, image_model },
            servers,
        };
        super::chat_turn::start(&state, device, id, body, query)
            .await
            .map_err(refusal)
    }
}

/// What a body that is not this machine's turn is refused with: the shape,
/// never the body, which can hold the message.
const LOCAL_TURN_BODY: &str = "a turn's body is {\"conversation_id\": <number>, \"content\": <text>, \
     \"images\": [<id>], \"thinking\": \"off\" or \"default\", \"answer_saved\": true or false, \
     \"draw\": true, \"image_model\": <id>, \"folder\": <path> or \"\"}, and no other key";

/// `PUT /api/runs/{id}?kind=agent`: start this machine's own turn on one of
/// its chats ([`LocalTurn`]), or answer with its run that has the id.
///
/// # Errors
///
/// `invalid_request` (400) for a body that is not a turn, an unknown key
/// included; then what [`start`] refuses, coded.
pub(crate) async fn start_local(
    state: &AppState,
    id: &str,
    body: serde_json::Value,
) -> Result<Created, HttpError> {
    let turn: LocalTurn = serde_json::from_value(body)
        .map_err(|_| coded(StatusCode::BAD_REQUEST, "invalid_request", LOCAL_TURN_BODY))?;
    let (turn, folder) = turn.split();
    start(state, RunScope::Local, id, turn, folder)
        .await
        .map_err(with_code)
}

/// A refusal as the daemon's door would answer it.
fn refusal(error: HttpError) -> TurnRefused {
    match with_code(error) {
        HttpError::Coded {
            status,
            code,
            message,
        } => TurnRefused {
            status: status.as_u16(),
            code: code.to_owned(),
            message,
        },
        other => TurnRefused {
            message: other.to_string(),
            status: other.into_response().status().as_u16(),
            code: "internal_error".to_owned(),
        },
    }
}

/// Start a turn in `scope` under `id`, or answer with its run that has the
/// id. `folder` is what this machine's own turn says of its chat's folder
/// (`""` forgets it, and none keeps the one the chat remembers); a device's
/// turn says none.
///
/// # Errors
///
/// `invalid_request` (400) for a message with neither text nor an image, or
/// a turn that answers the saved question and carries one, or a device's
/// turn that says a folder; `folder_unusable` (400) and `folder_missing`
/// (409) for this machine's; `nothing_to_answer`
/// (409) for such a turn on a chat that ends in no question;
/// `attachment_not_found` (400) for an image the turn or the chat's history
/// names that is not stored; `request_images_too_large` (400) when they are
/// over 16 MiB together;
/// `model_cannot_read_images` (400); `image_model_cannot_chat` (400) when
/// the chat's model draws images; `conversation_not_found` (404);
/// `conflict` (409) while the chat has a live reply, or, on a device's turn,
/// for a chat that ran on the machine this one is paired with; `no_model`
/// (422) when nothing names the chat's model and nothing runs on the hub;
/// `drawing_unavailable` (400) for a turn sent with Draw pressed that the
/// hub cannot draw for, `invalid_request` (400) for an `image_model`
/// without `draw`, and `model_not_found`, `not_an_image_model` or
/// `image_model_incomplete` (400) for one that cannot draw;
/// `agent_busy` (429). A refusal writes nothing.
///
/// The run is reserved before its model is found or loaded, so the `PUT`
/// answers at once and a cold load is the run's `waiting` event, not a
/// hanging request. What is refused after that ends the run `failed`,
/// having written nothing: `model_not_found`, naming the model, when this
/// machine does not hold it; `model_unavailable` when it cannot load it; what
/// the daemon's own door refuses (`unavailable`, `conflict`); and, on this
/// machine's turn on the paired machine's model, what that machine refuses.
pub(super) async fn start(
    state: &AppState,
    scope: RunScope,
    id: &str,
    turn: HubTurn,
    folder: Option<String>,
) -> Result<Created, HttpError> {
    if let Some(info) = state.runs.existing(&scope, id)? {
        return Ok(Created {
            info,
            created: false,
        });
    }
    let plan = plan(state, &scope, turn, folder).await?;
    let permit = take_permit(state).ok_or_else(|| {
        coded(
            StatusCode::TOO_MANY_REQUESTS,
            "agent_busy",
            "all agent loop slots are in use; try again later",
        )
    })?;
    // The run exists before its model does: found on its port or loaded,
    // and its loop composed, as the run's own work.
    let Plan {
        model,
        chat,
        transcript,
        asked,
        folder,
    } = plan;
    // The paired machine's model is listed by the name the chat has for it:
    // this machine's catalogue does not hold it.
    let shown = if chat.far.is_some() {
        model.clone()
    } else {
        shown_model(state, &model).await
    };
    let late = late_on(Arc::clone(state), model, chat, asked, folder);
    begin(state, scope, id, shown, transcript, late, permit)
}

/// What a run on `model` does once it exists: find the model on its port
/// or load it, unless it is the paired machine's (`chat.far`), settle the
/// tools it is offered as `asked` (starting an MCP server open to devices
/// that is not running, for a device's run), then compose `chat`'s loop
/// with gglib's file tools reading `folder`. A device's run has none: a
/// folder is this machine's choice, and a device's turn reaches no tool of
/// this machine's but the image tool its Draw asks for and the tools of the
/// servers open to devices (ADR 0025), which name no file tool.
///
/// A loop on this machine's model dials this machine's endpoint, turning it
/// on when it is off, as any client does (`remote_upstream::endpoint`): its
/// requests are shaped, guarded and counted there, and are in its ledger.
/// The model is loaded first, so the wait has a name, then the run holds
/// nothing: each request is admitted by the endpoint, and a model swapped
/// out between two of them is loaded again by the next.
pub(super) fn late_on(
    state: AppState,
    model: String,
    chat: LoopRequest,
    asked: Asked,
    folder: Option<PathBuf>,
) -> LatePrepare {
    Box::new(move |loading| {
        Box::pin(async move {
            let loaded = match &chat.far {
                Some(far) => Err(far.clone()),
                None => Ok(on_model(&state, &model, &loading).await?),
            };
            let chat = device_tools::settle(&state, chat, &asked).await;
            let upstream = match loaded {
                Err(far) => {
                    let reach = async |machine: &Machine| state.remote.far_for(machine).await;
                    remote_upstream::far(reach, &far, &chat.saved).await
                }
                Ok(loaded) => remote_upstream::through_endpoint(&state, &chat, loaded).await,
            };
            let upstream = upstream.map_err(with_code)?;
            Ok(prepare_over(&state, chat, upstream, folder).await)
        })
    })
}

/// The name a run on `model` is listed under: the catalogue's name for it,
/// or the identifier when the catalogue has none.
pub(super) async fn shown_model(state: &AppState, model: &str) -> String {
    let found = state.core.models().get(model).await.ok().flatten();
    found.map_or_else(|| model.to_owned(), |m| m.name)
}

/// Start the run `id` on `model`, reserved in `scope` before `late` finds
/// or loads the model and composes the loop. The run is then saved as
/// `transcript` says: its message and reply to the chat, which then
/// remembers what the turn said of thinking. What `late` or the first write
/// refuses ends the run `failed` with that code, with nothing written.
pub(super) fn begin(
    state: &AppState,
    scope: RunScope,
    id: &str,
    model: String,
    transcript: Transcript,
    late: LatePrepare,
    permit: OwnedSemaphorePermit,
) -> Result<Created, HttpError> {
    let run = (RunKind::Agent, model);
    launch_turn(state, id, scope, run, transcript, late, permit)
}

/// A turn read against the hub's record: the model it runs on, the
/// request the loop is composed from, and where the run is saved,
/// with what the chat is to remember of thinking and of its folder once it
/// starts, what it asks of this machine's tools, settled once the run
/// starts, and the folder gglib's file tools read in.
pub(super) struct Plan {
    /// This machine's model, as its catalogue resolves it; or, when
    /// `chat.far` names the paired machine's, the name the chat has for it.
    pub(super) model: String,
    pub(super) chat: LoopRequest,
    pub(super) transcript: Transcript,
    pub(super) asked: Asked,
    /// A canonical path, on this machine's own turn only.
    pub(super) folder: Option<PathBuf>,
}

/// Read `turn` in `scope` against the chat it names, with the `folder` it
/// says ([`start`]). A device's tool filter is the floor (`Asked::floor`)
/// until the run starts and the servers open to devices are read: nothing
/// is started for a `PUT` that is then refused. This machine's is final
/// here (`Asked::planned`).
pub(super) async fn plan(
    state: &AppState,
    scope: &RunScope,
    turn: HubTurn,
    folder: Option<String>,
) -> Result<Plan, HttpError> {
    let local = scope.device().is_none();
    if !local && folder.is_some() {
        return Err(coded(
            StatusCode::BAD_REQUEST,
            "invalid_request",
            "a paired device's turn has no folder: gglib's file tools read only in this \
             machine's own",
        ));
    }
    let said = !turn.content.trim().is_empty() || !turn.images.is_empty();
    if turn.answer_saved == said {
        let why = if said {
            "a turn that answers the question already saved carries no message of its own"
        } else {
            "a turn is the user's message, and it has neither text nor an image"
        };
        return Err(coded(StatusCode::BAD_REQUEST, "invalid_request", why));
    }
    let id = turn.conversation_id;
    let history = state.core.chat_history();
    let unreadable = |_| {
        coded(
            StatusCode::INTERNAL_SERVER_ERROR,
            "internal_error",
            "the conversation could not be read",
        )
    };
    let conversation = history
        .get_conversation(id)
        .await
        .map_err(unreadable)?
        .ok_or_else(|| {
            coded(
                StatusCode::NOT_FOUND,
                "conversation_not_found",
                format!("no conversation has id {id}"),
            )
        })?;
    // Before a model is loaded for it: the reservation refuses the same.
    state.runs.refuse_if_live(id)?;
    let rows = history.get_messages(id).await.map_err(unreadable)?;
    if turn.answer_saved {
        branching::answerable(&rows).map_err(ChangeError::from)?;
    }
    let far = paired(scope, &conversation);
    let model = turn_model(state, far.as_ref(), &conversation, &rows).await?;
    let settings = conversation.settings.clone().unwrap_or_default();
    // Before the run is reserved: a folder the turn says that is no folder
    // here, or the one the chat remembers that is gone.
    let (folder, remember_folder) = if local {
        let saved = settings.folder.as_deref();
        settle_folder(folder.as_deref(), Some(id), saved)?
    } else {
        (None, None)
    };
    // The prompt comes from the conversation, as the page shows it. A turn
    // that answers the question already saved adds none (ADR 0017).
    let mut messages = saved_history(conversation.system_prompt.as_deref(), &rows);
    if !turn.answer_saved {
        messages.push(AgentMessage::User {
            content: turn.content,
            images: turn.images,
        });
    }
    // Before the model is loaded, over this turn and the history: an image
    // not stored, images over the cap together, and a model that cannot
    // read them.
    state.core.attachments().check_request(&messages).await?;
    if far.is_none() {
        super::image_gate::named(state, &model, &messages).await?;
    }
    // A turn has no budget of its own: off, or the model's default.
    let thinking = thinking::settle(turn.thinking, settings.thinking);
    let asked = if local {
        // The paired machine's catalogue says what its model calls; the
        // request pipeline there strips tools from one that calls none.
        let calls = far.is_some() || calls_tools(state, &model).await;
        Asked::of_local(&settings, turn.draw, calls)
    } else {
        Asked::of_chat(&settings, turn.draw)
    };
    let chat = LoopRequest {
        far,
        messages,
        limits: limits_of(&settings),
        tool_filter: asked.planned(),
        reasoning_budget_tokens: thinking.budget,
        draw: turn.draw,
        image_model: turn.image_model,
        saved: ChatSampling::of(&settings),
    };
    // A turn sent with Draw pressed that the hub cannot draw for, or with
    // the image model it chose, is refused
    // here, before a slot is taken, a run made or a row written.
    refuse_unavailable_drawing(state, &chat).await?;
    let transcript = Transcript {
        conversation_id: Some(id),
        answer_saved: turn.answer_saved,
        remember: thinking.remember,
        remember_folder,
    };
    Ok(Plan {
        model,
        chat,
        transcript,
        asked,
        folder,
    })
}

/// The loop's limits the conversation's settings name.
const fn limits_of(settings: &ConversationSettings) -> NamedLimits {
    NamedLimits {
        max_iterations: settings.max_iterations,
        max_parallel_tools: settings.max_parallel,
        tool_timeout_ms: settings.tool_timeout_ms,
    }
}

#[cfg(test)]
#[path = "hub_turn_answer_tests.rs"]
mod hub_turn_answer_tests;
#[cfg(test)]
#[path = "hub_turn_drawn_tests.rs"]
mod hub_turn_drawn_tests;
#[cfg(test)]
#[path = "hub_turn_endpoint_tests.rs"]
mod hub_turn_endpoint_tests;
#[cfg(test)]
#[path = "hub_turn_forget_tests.rs"]
mod hub_turn_forget_tests;
#[cfg(test)]
#[path = "hub_turn_images_tests.rs"]
mod hub_turn_images_tests;
#[cfg(test)]
#[path = "hub_turn_late_tests.rs"]
mod hub_turn_late_tests;
#[cfg(test)]
#[path = "hub_turn_limits_tests.rs"]
mod hub_turn_limits_tests;
#[cfg(test)]
#[path = "hub_turn_local_tests.rs"]
mod hub_turn_local_tests;
#[cfg(test)]
#[path = "hub_turn_plan_tests.rs"]
mod hub_turn_plan_tests;
#[cfg(test)]
#[path = "hub_turn_sampling_tests.rs"]
mod hub_turn_sampling_tests;
#[cfg(test)]
#[path = "hub_turn_tests.rs"]
mod hub_turn_tests;
#[cfg(test)]
#[path = "hub_turn_thinking_tests.rs"]
mod hub_turn_thinking_tests;
