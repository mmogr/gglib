//! A daemon turn for a test: a turn on one of this machine's chats or a
//! device's, or a phone-kept chat run, run as `late_on` runs it once its
//! model is loaded: through this machine's endpoint, against a server that
//! records what it is sent, standing in for the endpoint.

use std::path::PathBuf;
use std::time::Duration;

use gglib_core::domain::agent::{AgentEvent, AgentMessage};
use gglib_core::domain::chat::{ConversationSettings, NewConversation};
use gglib_core::domain::hub_chats::HubTurn;
use gglib_core::domain::{
    DefaultsOrigin, InferenceConfig, Machine, ModelCapabilities, ModelRef, NewModel,
};
use gglib_core::settings::SettingsUpdate;
use serde_json::Value;

use super::compose::{Prepared, prepare_over};
use super::loop_request::LoopRequest;
use crate::handlers::agent::remote_upstream::endpoint;
use crate::handlers::agent::run_fixture::device;
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

/// The key this machine's endpoint demands in a test's turn through it.
pub(super) const ENDPOINT_KEY: &str = "sk-this-machines-endpoint";

/// What a turn did: each request its model server received, the bearer each
/// carried, the events its loop sent, and whether the loop ended well.
pub(super) struct Turn {
    pub(super) requests: Vec<Value>,
    pub(super) bearers: Vec<Option<String>>,
    pub(super) events: Vec<AgentEvent>,
    pub(super) ended_well: bool,
}

/// Run `chat` as `hub_turn::late_on` does once model `model_id` is loaded:
/// through this machine's endpoint, which a server answering every request
/// with `reply` stands in for, demanding [`ENDPOINT_KEY`], with no folder.
pub(super) async fn turn(state: &AppState, model_id: i64, chat: LoopRequest, reply: &str) -> Turn {
    turn_in(state, model_id, chat, reply, None).await
}

/// [`turn`], with gglib's own file tools reading `folder`.
async fn turn_in(
    state: &AppState,
    model_id: i64,
    chat: LoopRequest,
    reply: &str,
    folder: Option<PathBuf>,
) -> Turn {
    let (fake, at) = far(200, reply).await;
    *fake.content_type.lock().unwrap() = "text/event-stream";
    let root = at.server_root();
    let port: u16 = root.rsplit(':').next().unwrap().parse().unwrap();
    let door = gglib_runtime::Door {
        root_url: root,
        key: Some(ENDPOINT_KEY.to_owned()),
        name: None,
    };
    let loaded = super::hub_model::Loaded { id: model_id, port };
    let upstream = endpoint(state, &chat, loaded, door).await.unwrap();
    let prepared = prepare_over(state, chat, upstream, folder).await;
    let Prepared {
        agent_loop,
        messages,
        config,
        tx,
        mut rx,
        ..
    } = prepared;
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
        bearers: seen.into_iter().map(|request| request.bearer).collect(),
        events,
        ended_well: ended.is_ok(),
    }
}

/// The body of the one request a turn that ends well makes.
pub(super) async fn sent(state: &AppState, model_id: i64, chat: LoopRequest) -> Value {
    sent_in(state, model_id, chat, None).await
}

/// [`sent`], for a run whose folder is `folder`.
pub(super) async fn sent_in(
    state: &AppState,
    model_id: i64,
    chat: LoopRequest,
    folder: Option<PathBuf>,
) -> Value {
    let mut turn = turn_in(state, model_id, chat, REPLY, folder).await;
    assert!(turn.ended_well, "{:?}", turn.events);
    assert_eq!(turn.requests.len(), 1, "one request");
    turn.requests.remove(0)
}

/// The two requests of a turn sent with Draw, to a model that answers every
/// request with a call for the picture: the one held to that call, and the
/// one made once the tool has run. There are only two: the second reply's
/// call is not run, and the turn ends there.
pub(super) async fn drawn(state: &AppState, model_id: i64, chat: LoopRequest) -> [Value; 2] {
    let mut turn = turn(state, model_id, chat, DRAWS).await;
    assert_eq!(turn.requests.len(), 2, "{:?}", turn.events);
    let later = turn.requests.remove(1);
    [turn.requests.remove(0), later]
}

/// Whether a request to the model offers it nothing to call: no `tools`,
/// no `tool_choice` and no grammar.
pub(super) fn offers_nothing(body: &Value) -> bool {
    ["tools", "tool_choice", "grammar"]
        .iter()
        .all(|key| body.get(key).is_none())
}

/// Add a model that can call tools to the catalogue, as `edit` leaves it.
pub(super) async fn model(state: &AppState, edit: impl FnOnce(&mut NewModel)) -> i64 {
    let path = PathBuf::from("/models/served.gguf");
    let mut entry = NewModel::new(
        "catalogue-name".to_owned(),
        path,
        Some(7.0),
        chrono::Utc::now(),
    );
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

/// A loop's request as a test names it: `{"messages", "tool_filter",
/// "draw", "image_model", "reasoning_budget_tokens", "far"}`, any of them,
/// over one user message that says "hi" and every tool. A key it does not
/// know fails the test, so none is dropped in silence.
pub(super) fn page(more: Value) -> LoopRequest {
    let mut chat = LoopRequest {
        messages: vec![AgentMessage::User {
            content: "hi".to_owned(),
            images: Vec::new(),
        }],
        ..LoopRequest::default()
    };
    let Value::Object(more) = more else {
        panic!("a loop's request is an object: {more}");
    };
    for (key, value) in more {
        match key.as_str() {
            "messages" => chat.messages = serde_json::from_value(value).unwrap(),
            "tool_filter" => chat.tool_filter = serde_json::from_value(value).unwrap(),
            "draw" => chat.draw = serde_json::from_value(value).unwrap(),
            "image_model" => chat.image_model = serde_json::from_value(value).unwrap(),
            "reasoning_budget_tokens" => {
                chat.reasoning_budget_tokens = serde_json::from_value(value).unwrap();
            }
            "far" => chat.far = serde_json::from_value(value).unwrap(),
            other => panic!("a loop's request has no `{other}`"),
        }
    }
    chat
}

/// A paired device's turn on a chat of `model_id`, read as its door reads it.
pub(super) async fn phone(state: &AppState, model_id: i64) -> LoopRequest {
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
    super::hub_turn::plan(state, &device("phone"), hi(id), None)
        .await
        .unwrap()
        .chat
}

/// A paired device's turn on chat `conversation_id` that says "hi".
pub(super) fn hi(conversation_id: i64) -> HubTurn {
    HubTurn {
        conversation_id,
        content: "hi".to_owned(),
        images: Vec::new(),
        thinking: None,
        answer_saved: false,
        draw: false,
        image_model: None,
    }
}

/// Every sampling parameter a request carries, read as the pipeline reads a
/// client's.
pub(super) fn sampling_of(body: &Value) -> InferenceConfig {
    InferenceConfig::extract_client_sampling(body).0
}
