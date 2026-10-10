//! A paired device's turn on one of the hub's chats: the proxy's
//! `PUT /v1/runs/{id}?kind=agent`, run here because the agent loop is
//! composed here.
//!
//! The device sends only its message, with any image named by the id its
//! upload answered, or, after a change that leaves the chat ending in a
//! question (ADR 0017), no message and `answer_saved`, and the reply
//! answers that question. The hub rebuilds the history from its
//! own record, as the chat page would send it: the conversation's system
//! prompt, then every saved row but a system one, then the new message,
//! with the limits the conversation's settings name (`prepare` takes an
//! iteration limit they do not name from this machine's settings, as it
//! does for the page), and with thinking off
//! when the turn says so or the chat remembers it (`thinking`). It calls no
//! tool unless
//! this machine lets the tunnel reach its MCP tools, and then only those the
//! settings name. The reply runs on the chat's
//! model, loaded as `/v1/models/{name}/load` loads it when it is not
//! running, as an agent run in the device's scope, saved to the chat.
//!
//! Nothing here logs or returns the message, a row or a title: only ids.

use std::sync::{Arc, Weak};

use async_trait::async_trait;
use axum::http::StatusCode;
use axum::response::IntoResponse as _;

use gglib_core::domain::agent::{AgentMessage, saved_history};
use gglib_core::domain::branching;
use gglib_core::domain::chat::ConversationSettings;
use gglib_core::domain::hub_chats::HubTurn;
use gglib_core::domain::thinking;
use gglib_core::ports::{
    AgentRunStarter, Created, RemoteGatewayPort as _, RunScope, RunsError, TurnRefused,
};
use gglib_core::services::ChangeError;
use tokio::sync::OwnedSemaphorePermit;

use super::AgentChatRequest;
use super::compose::{Prepared, prepare, take_permit};
use super::dto::AgentRequestConfig;
use super::hub_model::{model_for, on_model};
use super::launch::{Transcript, launch};
use super::remote_upstream;
use super::run::{coded, with_code};
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
        start(&state, device, id, turn).await.map_err(refusal)
    }
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

/// Start `device`'s turn under `id`, or answer with its run that has the id.
///
/// # Errors
///
/// `invalid_request` (400) for a message with neither text nor an image, or
/// a turn that answers the saved question and carries one; `nothing_to_answer`
/// (409) for such a turn on a chat that ends in no question;
/// `attachment_not_found` (400) for an image the turn or the chat's history
/// names that is not stored; `request_images_too_large` (400) when they are
/// over 16 MiB together;
/// `model_cannot_read_images` (400); `image_model_cannot_chat` (400) when
/// the chat's model draws images; `conversation_not_found` (404);
/// `conflict` (409) while the chat has a live reply, or for a chat that ran
/// on the machine this one is paired with; `no_model`
/// (422) when nothing names the chat's model and nothing runs on the hub;
/// `agent_busy` (429); `model_unavailable` (503) when it cannot be loaded;
/// and whatever the daemon's own door refuses the same run with. A refusal
/// writes nothing.
pub(super) async fn start(
    state: &AppState,
    device: &str,
    id: &str,
    turn: HubTurn,
) -> Result<Created, HttpError> {
    let scope = RunScope::Device(device.to_owned());
    if let Some(info) = state.runs.existing(&scope, id)? {
        return Ok(Created {
            info,
            created: false,
        });
    }
    let plan = plan(state, turn).await?;
    let permit = take_permit(state).ok_or_else(|| {
        coded(
            StatusCode::TOO_MANY_REQUESTS,
            "agent_busy",
            "all agent loop slots are in use; try again later",
        )
    })?;
    let chat = on_model(state, &plan.model, plan.chat).await?;
    let prepared = prepare(state, chat).await.map_err(with_code)?;
    begin(state, device, id, plan.transcript, prepared, permit).await
}

/// Start `device`'s run `id` with its prepared loop: held on its model,
/// reserved in the device's scope, and saved as `transcript` says: its
/// message and reply to the chat, which then remembers what the turn said
/// of thinking.
pub(super) async fn begin(
    state: &AppState,
    device: &str,
    id: &str,
    transcript: Transcript,
    mut prepared: Prepared,
    permit: OwnedSemaphorePermit,
) -> Result<Created, HttpError> {
    remote_upstream::hold_model(state.runtime.as_ref(), &mut prepared).await?;
    let scope = RunScope::Device(device.to_owned());
    launch(state, id, scope, transcript, prepared, permit).await
}

/// A turn read against the hub's record: the model it runs on, the chat
/// request the page would send, less the port, and where the run is saved,
/// with what the chat is to remember of thinking once it starts.
pub(super) struct Plan {
    pub(super) model: String,
    pub(super) chat: AgentChatRequest,
    pub(super) transcript: Transcript,
}

/// Read `turn` against the chat it names, with the tools the tunnel's owner
/// lets a device's turn reach.
pub(super) async fn plan(state: &AppState, turn: HubTurn) -> Result<Plan, HttpError> {
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
    let model = model_for(state, &conversation, &rows).await?;
    // Before anything else is read for it, and before `on_model` would load
    // it: a model that draws is served by sd-server, which cannot chat.
    super::image_gate::chats_named(state, &model).await?;
    // The prompt comes from the conversation, as the page takes it. A turn
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
    super::image_gate::named(state, &model, &messages).await?;
    let settings = conversation.settings.unwrap_or_default();
    // A device's turn has no budget of its own: off, or the model's default.
    let thinking = thinking::settle(turn.thinking, settings.thinking, None);
    let chat = AgentChatRequest {
        port: 0,
        far: None,
        messages,
        config: config_of(&settings),
        tool_filter: Some(tools_of(&settings, state.remote.gateway().mcp_allowed())),
        model: None,
        reasoning_effort: None,
        reasoning_budget_tokens: thinking.budget,
    };
    let transcript = Transcript {
        conversation_id: Some(id),
        answer_saved: turn.answer_saved,
        remember: thinking.remember,
    };
    Ok(Plan {
        model,
        chat,
        transcript,
    })
}

/// The loop's limits the conversation's settings name, if any.
fn config_of(settings: &ConversationSettings) -> Option<AgentRequestConfig> {
    let config = AgentRequestConfig {
        max_iterations: settings.max_iterations,
        max_parallel_tools: settings.max_parallel,
        tool_timeout_ms: settings.tool_timeout_ms,
        ..AgentRequestConfig::default()
    };
    let named = config.max_iterations.is_some()
        || config.max_parallel_tools.is_some()
        || config.tool_timeout_ms.is_some();
    named.then_some(config)
}

/// The tools a device's turn may call. None unless this machine lets the
/// tunnel reach its MCP tools (`gglib remote enable --allow-mcp`), as `/mcp`
/// itself does: a leaked key must not run a shell server. Then only those
/// the conversation's settings name, and none when it names none or turned
/// them off. Never every tool, which the page's own turns may call.
pub(super) fn tools_of(settings: &ConversationSettings, mcp_allowed: bool) -> Vec<String> {
    if !mcp_allowed || settings.no_tools == Some(true) {
        return Vec::new();
    }
    settings.tools.clone()
}

#[cfg(test)]
#[path = "hub_turn_answer_tests.rs"]
mod hub_turn_answer_tests;
#[cfg(test)]
#[path = "hub_turn_forget_tests.rs"]
mod hub_turn_forget_tests;
#[cfg(test)]
#[path = "hub_turn_images_tests.rs"]
mod hub_turn_images_tests;
#[cfg(test)]
#[path = "hub_turn_limits_tests.rs"]
mod hub_turn_limits_tests;
#[cfg(test)]
#[path = "hub_turn_plan_tests.rs"]
mod hub_turn_plan_tests;
#[cfg(test)]
#[path = "hub_turn_tests.rs"]
mod hub_turn_tests;
#[cfg(test)]
#[path = "hub_turn_thinking_tests.rs"]
mod hub_turn_thinking_tests;
