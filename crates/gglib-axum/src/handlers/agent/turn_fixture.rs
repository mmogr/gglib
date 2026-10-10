//! A daemon turn for a test: the page's run and a paired device's turn, each
//! read as its door reads it, run as `prepare` runs it once its model is
//! found on a port, against a model server that records what it is sent.

use std::path::PathBuf;
use std::time::Duration;

use gglib_app_services::types::ServerInfo;
use gglib_core::domain::agent::AgentEvent;
use gglib_core::domain::chat::{ConversationSettings, NewConversation};
use gglib_core::domain::hub_chats::HubTurn;
use gglib_core::domain::{
    DefaultsOrigin, InferenceConfig, Machine, ModelCapabilities, ModelRef, NewModel,
};
use gglib_core::settings::SettingsUpdate;
use serde_json::{Value, json};

use super::compose::{Prepared, prepare_over};
use crate::handlers::agent::AgentChatRequest;
use crate::handlers::agent::dto::AgentRunRequest;
use crate::handlers::agent::remote_upstream::local;
use crate::handlers::remote::fake_far::far;
use crate::state::AppState;

/// One short reply, as llama-server streams it.
pub(super) const REPLY: &str = concat!(
    "data: {\"choices\":[{\"delta\":{\"content\":\"ok\"}}]}\n\n",
    "data: {\"choices\":[{\"delta\":{},\"finish_reason\":\"stop\"}]}\n\n",
    "data: [DONE]\n\n",
);

/// One reply that calls the image tool, as llama-server streams a native
/// call.
pub(super) const DRAWS: &str = concat!(
    "data: {\"choices\":[{\"delta\":{\"tool_calls\":[{\"index\":0,\"id\":\"c1\",",
    "\"type\":\"function\",\"function\":{\"name\":\"builtin:generate_image\",",
    "\"arguments\":\"{\\\"prompt\\\":\\\"a fox\\\"}\"}}]}}]}\n\n",
    "data: {\"choices\":[{\"delta\":{},\"finish_reason\":\"tool_calls\"}]}\n\n",
    "data: [DONE]\n\n",
);

/// The built-in tool, as a tool filter names it.
pub(super) const TOOL: &str = "builtin:get_current_time";

/// What a turn did: each request its model server received, the events its
/// loop sent, and whether the loop ended well.
pub(super) struct Turn {
    pub(super) requests: Vec<Value>,
    pub(super) events: Vec<AgentEvent>,
    pub(super) ended_well: bool,
}

/// Run `chat` as `prepare` does once model `model_id` is found on its port,
/// against a model server that answers every request with `reply`.
pub(super) async fn turn(
    state: &AppState,
    model_id: i64,
    mut chat: AgentChatRequest,
    reply: &str,
) -> Turn {
    let (fake, at) = far(200, reply).await;
    *fake.content_type.lock().unwrap() = "text/event-stream";
    let root = at.server_root();
    let port: u16 = root.rsplit(':').next().unwrap().parse().unwrap();
    chat.port = port;
    let server = ServerInfo {
        model_id,
        model_name: "served".to_owned(),
        pid: None,
        port,
        started_at: 0,
        runtime: gglib_core::domain::RuntimeKind::Llama,
    };
    let upstream = local(state, &chat, server).await.unwrap();
    let Prepared {
        agent_loop,
        messages,
        config,
        tx,
        mut rx,
        ..
    } = prepare_over(state, chat, upstream).await;
    let ended = tokio::time::timeout(
        Duration::from_secs(30),
        agent_loop.run(messages, config, tx),
    )
    .await
    .expect("the turn ends");
    let mut events = Vec::new();
    while let Ok(event) = rx.try_recv() {
        events.push(event);
    }
    let seen = fake.seen.lock().unwrap().clone();
    let requests = seen
        .iter()
        .map(|request| {
            assert_eq!(request.uri, "/v1/chat/completions");
            serde_json::from_str(&request.body).unwrap()
        })
        .collect();
    Turn {
        requests,
        events,
        ended_well: ended.is_ok(),
    }
}

/// The body of the one request a turn that ends well makes.
pub(super) async fn sent(state: &AppState, model_id: i64, chat: AgentChatRequest) -> Value {
    let mut turn = turn(state, model_id, chat, REPLY).await;
    assert!(turn.ended_well, "{:?}", turn.events);
    assert_eq!(turn.requests.len(), 1, "one request");
    turn.requests.remove(0)
}

/// The first two requests of a turn sent with Draw, to a model that answers
/// every request with a call for the picture: the one held to that call,
/// and the one made once the tool has run. (The loop's guard ends such a
/// turn after a few; only these two are read.)
pub(super) async fn drawn(state: &AppState, model_id: i64, chat: AgentChatRequest) -> [Value; 2] {
    let mut turn = turn(state, model_id, chat, DRAWS).await;
    assert!(turn.requests.len() >= 2, "{:?}", turn.events);
    let later = turn.requests.remove(1);
    [turn.requests.remove(0), later]
}

/// Add a model that can call tools to the catalogue, as `edit` leaves it.
pub(super) async fn model(state: &AppState, edit: impl FnOnce(&mut NewModel)) -> i64 {
    let path = PathBuf::from("/models/served.gguf");
    let mut entry = NewModel::new("catalogue-name".to_owned(), path, 7.0, chrono::Utc::now());
    entry.capabilities =
        ModelCapabilities::SUPPORTS_TOOL_CALLS | ModelCapabilities::SUPPORTS_SYSTEM_ROLE;
    edit(&mut entry);
    state.core.models().add(entry).await.unwrap().id
}

/// `config` as the model's stored defaults, set as `origin` says.
pub(super) fn stored(
    config: InferenceConfig,
    origin: DefaultsOrigin,
) -> impl FnOnce(&mut NewModel) {
    move |entry| {
        entry.inference_defaults = Some(config);
        entry.defaults_origin = Some(origin);
    }
}

/// A reasoning-tagged model with the recipe its import wrote.
pub(super) fn reasoning(entry: &mut NewModel) {
    entry.tags = vec!["reasoning".to_owned()];
    stored(
        InferenceConfig::reasoning_profile(),
        DefaultsOrigin::AutoDetected,
    )(entry);
}

pub(super) async fn global(state: &AppState, config: InferenceConfig) {
    let update = SettingsUpdate {
        inference_defaults: Some(Some(config)),
        ..SettingsUpdate::default()
    };
    state.core.settings().update(update).await.unwrap();
}

/// Store the agentic sampling switch as `stored`: on, off, or not stored.
pub(super) async fn agentic_sampling(state: &AppState, stored: Option<bool>) {
    let update = SettingsUpdate {
        agentic_sampling: Some(stored),
        ..SettingsUpdate::default()
    };
    state.core.settings().update(update).await.unwrap();
}

pub(super) fn temperature(value: f32) -> InferenceConfig {
    InferenceConfig {
        temperature: Some(value),
        ..InferenceConfig::default()
    }
}

/// The page's run, read as its door reads it.
pub(super) async fn page(state: &AppState, more: Value) -> AgentChatRequest {
    let mut body = json!({
        "port": 0,
        "messages": [{ "role": "user", "content": "hi" }],
        "model": null,
    });
    body.as_object_mut()
        .unwrap()
        .extend(more.as_object().unwrap().clone());
    let request: AgentRunRequest = serde_json::from_value(body).unwrap();
    super::run::plan(state, request).await.unwrap().0
}

/// A paired device's turn on a chat of `model_id`, read as its door reads it.
pub(super) async fn phone(state: &AppState, model_id: i64) -> AgentChatRequest {
    let settings = ConversationSettings {
        model: Some(ModelRef {
            machine: Machine::Local,
            id: model_id,
        }),
        ..ConversationSettings::default()
    };
    let id = state
        .core
        .chat_history()
        .create_conversation(NewConversation {
            title: "t".to_owned(),
            model_id: None,
            system_prompt: None,
            settings: Some(settings),
        })
        .await
        .unwrap();
    let turn = HubTurn {
        conversation_id: id,
        content: "hi".to_owned(),
        images: Vec::new(),
        thinking: None,
        answer_saved: false,
        draw: false,
    };
    super::hub_turn::plan(state, turn).await.unwrap().chat
}

/// The same turn from each door: the page's run, and a paired device's on a
/// chat of `model_id`. With [`TOOL`] when `tools`, and otherwise none.
pub(super) async fn doors(
    state: &AppState,
    model_id: i64,
    tools: bool,
) -> [(&'static str, AgentChatRequest); 2] {
    let filter: Vec<String> = tools.then(|| TOOL.to_owned()).into_iter().collect();
    let page = page(state, json!({ "tool_filter": &filter })).await;
    let mut phone = phone(state, model_id).await;
    // As a device's plan holds it once `remote enable --allow-mcp` has opened
    // the tunnel to this machine's tools and the chat names this one.
    phone.tool_filter = Some(filter);
    [("the page", page), ("a paired device", phone)]
}

pub(super) fn carries_tools(body: &Value) -> bool {
    body["tools"]
        .as_array()
        .is_some_and(|tools| !tools.is_empty())
}

/// Every sampling parameter a request carries, read as the pipeline reads a
/// client's.
pub(super) fn sampling_of(body: &Value) -> InferenceConfig {
    InferenceConfig::extract_client_sampling(body).0
}
