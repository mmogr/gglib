//! A paired device's turn on one of the hub's chats: the proxy's
//! `PUT /v1/runs/{id}?kind=agent`, run here because the agent loop is
//! composed here.
//!
//! The device sends only its message. The hub rebuilds the history from its
//! own record, as the chat page would send it: the conversation's system
//! prompt, then every saved row, then the new message, with the limits and
//! tools the conversation's settings name. The reply runs on the chat's
//! model, loaded as `/v1/models/{name}/load` loads it when it is not
//! running, as an agent run in the device's scope, saved to the chat.
//!
//! Nothing here logs or returns the message, a row or a title: only ids.

use std::sync::{Arc, Weak};

use async_trait::async_trait;
use axum::http::StatusCode;
use axum::response::IntoResponse as _;

use gglib_core::domain::agent::AgentMessage;
use gglib_core::domain::chat::{ConversationSettings, Message};
use gglib_core::domain::hub_chats::HubTurn;
use gglib_core::ports::{AgentRunStarter, Created, RunScope, RunsError, TurnRefused};

use super::AgentChatRequest;
use super::compose::{prepare, take_permit};
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
/// `invalid_request` (400) for an empty message; `conversation_not_found`
/// (404); `conflict` (409) while the chat has a live reply; `no_model`
/// (409) when nothing names the chat's model; `agent_busy` (429);
/// `model_unavailable` (503) when it cannot be loaded; and whatever the
/// daemon's own door refuses the same run with. A refusal writes nothing.
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
    let mut prepared = prepare(state, chat).await.map_err(with_code)?;
    prepared.hold = remote_upstream::hold(state.runtime.as_ref(), prepared.local_model)?;
    let transcript = Transcript {
        conversation_id: Some(plan.conversation_id),
        replace_from: None,
    };
    launch(state, id, scope, transcript, prepared, permit).await
}

/// A turn read against the hub's record: the chat request the page would
/// send, less the port, and the model it runs on.
pub(super) struct Plan {
    pub(super) conversation_id: i64,
    pub(super) model: String,
    pub(super) chat: AgentChatRequest,
}

/// Read `turn` against the chat it names.
pub(super) async fn plan(state: &AppState, turn: HubTurn) -> Result<Plan, HttpError> {
    if turn.content.trim().is_empty() {
        return Err(coded(
            StatusCode::BAD_REQUEST,
            "invalid_request",
            "a turn's content is the user's message, and it is empty",
        ));
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
    if let Some(run) = state.runs.live_on(id) {
        return Err(RunsError::ConversationBusy {
            conversation_id: id,
            run,
        }
        .into());
    }
    let rows = history.get_messages(id).await.map_err(unreadable)?;
    let model = model_for(state, &conversation, &rows).await?;
    let prompt = conversation.system_prompt.as_deref().map(str::trim);
    let mut messages: Vec<AgentMessage> = prompt
        .filter(|p| !p.is_empty())
        .map(|p| AgentMessage::System {
            content: p.to_owned(),
        })
        .into_iter()
        .collect();
    messages.extend(rows.iter().map(Message::to_agent_message));
    messages.push(AgentMessage::User {
        content: turn.content,
    });
    let settings = conversation.settings.unwrap_or_default();
    let chat = AgentChatRequest {
        port: 0,
        remote: false,
        messages,
        config: config_of(&settings),
        tool_filter: tools_of(&settings),
        model: None,
        reasoning_effort: None,
        reasoning_budget_tokens: None,
    };
    Ok(Plan {
        conversation_id: id,
        model,
        chat,
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

/// The tools the conversation's settings allow: none when it turned them
/// off, its list when it names one, and otherwise every tool.
fn tools_of(settings: &ConversationSettings) -> Option<Vec<String>> {
    if settings.no_tools == Some(true) {
        return Some(Vec::new());
    }
    Some(settings.tools.clone()).filter(|tools| !tools.is_empty())
}

#[cfg(test)]
#[path = "hub_turn_tests.rs"]
mod hub_turn_tests;
