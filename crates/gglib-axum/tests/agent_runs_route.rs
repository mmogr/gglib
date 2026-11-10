//! `PUT /api/runs/{id}?kind=agent` on the daemon's router: this machine's
//! turn on one of its chats (`LocalTurn`). Each refusal before the run is
//! made comes with a code and leaves no run behind; what is refused once
//! the run exists ends it `failed` with the code.
//!
//! The harness runs no llama-server and no tunnel, so a turn that passes
//! every check of its own is made, and then fails: its model cannot load,
//! or its paired machine is away. The run's life over a loop is
//! `handlers/agent/run_tests.rs`.

mod common;

use std::time::Duration;

use axum::Router;
use axum::http::{Method, StatusCode};
use gglib_core::CorsConfig;
use gglib_core::contracts::http::daemon::{RUNS_PATH, run_path};
use gglib_core::domain::chat::{ConversationSettings, NewConversation};
use gglib_core::domain::runs::{RunInfo, RunKind, RunList, RunStatus};
use gglib_core::domain::{Machine, ModelRef};
use serde_json::{Value, json};

use common::harness::test_state_and_app;
use common::origin::call_json;

fn agent(id: &str) -> String {
    format!("{}?kind=agent", run_path(id))
}

/// A chat whose settings are `settings`, on this machine's `qwen` by its id
/// unless they store a model.
async fn chat(state: &gglib_axum::AppState, settings: ConversationSettings) -> i64 {
    let model = match settings.model {
        Some(model) => model,
        None => qwen(state).await,
    };
    let settings = ConversationSettings {
        model: Some(model),
        model_name: settings.model_name.or_else(|| Some("qwen".to_owned())),
        ..settings
    };
    state
        .core
        .chat_history()
        .create_conversation(NewConversation {
            title: "t".to_owned(),
            settings: Some(settings),
            ..NewConversation::default()
        })
        .await
        .unwrap()
}

/// This machine's `qwen`, by its id, catalogued on first use: a chat names
/// its model by id, and a name alone is never looked up.
async fn qwen(state: &gglib_axum::AppState) -> ModelRef {
    let found = state.core.models().get("qwen").await.unwrap();
    let id = if let Some(model) = found {
        model.id
    } else {
        let model = gglib_core::domain::NewModel::new(
            "qwen".to_owned(),
            std::path::PathBuf::from("/models/qwen.gguf"),
            Some(7.0),
            chrono::Utc::now(),
        );
        state.core.models().add(model).await.unwrap().id
    };
    ModelRef {
        machine: Machine::Local,
        id,
    }
}

/// The paired machine's model 3.
fn on_desk() -> ConversationSettings {
    ConversationSettings {
        model: Some(ModelRef {
            machine: Machine::Paired {
                fingerprint: "0a1b2c3d4e5f".to_owned(),
            },
            id: 3,
        }),
        model_name: Some("qwen-desk".to_owned()),
        ..ConversationSettings::default()
    }
}

/// This machine's turn on `chat`, its message `PROMPT-SECRET`.
fn turn(chat: i64) -> Value {
    json!({ "conversation_id": chat, "content": "PROMPT-SECRET" })
}

async fn no_runs(app: &Router) {
    let (_, list) = call_json(app, Method::GET, RUNS_PATH, None).await;
    let list: RunList = serde_json::from_value(list).unwrap();
    assert!(list.runs.is_empty(), "{list:?}");
}

/// The run `id` once it has ended.
async fn ended(app: &Router, id: &str) -> RunInfo {
    let read = async {
        loop {
            let (_, info) = call_json(app, Method::GET, &run_path(id), None).await;
            let info: RunInfo = serde_json::from_value(info).unwrap();
            if !matches!(info.status, RunStatus::Queued | RunStatus::InProgress) {
                return info;
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    };
    tokio::time::timeout(Duration::from_secs(10), read)
        .await
        .expect("the run ends")
}

#[tokio::test]
async fn an_unknown_conversation_is_a_404_and_nothing_runs() {
    let (_, app) = test_state_and_app(CorsConfig::AllowAll).await;

    let (status, error) = call_json(&app, Method::PUT, &agent("a1"), Some(turn(4242))).await;

    assert_eq!(status, StatusCode::NOT_FOUND, "{error}");
    assert_eq!(error["type"], "conversation_not_found");
    no_runs(&app).await;
}

#[tokio::test]
async fn with_every_agent_slot_taken_it_is_a_429_and_nothing_runs() {
    let (state, app) = test_state_and_app(CorsConfig::AllowAll).await;
    let id = chat(&state, ConversationSettings::default()).await;
    let slots = state.agent_semaphore.available_permits();
    let _held = std::sync::Arc::clone(&state.agent_semaphore)
        .try_acquire_many_owned(u32::try_from(slots).unwrap())
        .unwrap();

    let (status, error) = call_json(&app, Method::PUT, &agent("a1"), Some(turn(id))).await;

    assert_eq!(status, StatusCode::TOO_MANY_REQUESTS, "{error}");
    assert_eq!(error["type"], "agent_busy");
    no_runs(&app).await;
}

/// The page's old body, and each key of it beside a turn's, is refused by
/// its shape: the history, the port, the tool filter and the limits are
/// the daemon's to decide now, and none is dropped without a word.
#[tokio::test]
async fn the_old_body_and_each_old_key_are_refused_without_quoting_them() {
    let (state, app) = test_state_and_app(CorsConfig::AllowAll).await;
    let id = chat(&state, ConversationSettings::default()).await;
    let old = json!({
        "conversation_id": id,
        "port": 9000,
        "messages": [{ "role": "user", "content": "BODY-SECRET" }],
    });
    // A known key of the wrong type, and an unknown key: serde's own words
    // would quote the value, or the key.
    let mut bodies = vec![
        old,
        json!({ "conversation_id": "BODY-SECRET", "content": "PROMPT-SECRET" }),
        json!({ "conversation_id": id, "content": ["BODY-SECRET"] }),
        json!({ "conversation_id": id, "content": "PROMPT-SECRET", "BODY-SECRET": 1 }),
    ];
    for key in [
        "messages",
        "port",
        "far",
        "config",
        "tool_filter",
        "model",
        "reasoning_effort",
        "reasoning_budget_tokens",
    ] {
        let mut body = turn(id);
        body[key] = json!("BODY-SECRET");
        bodies.push(body);
    }

    for body in bodies {
        let (status, error) = call_json(&app, Method::PUT, &agent("a1"), Some(body.clone())).await;

        assert_eq!(status, StatusCode::BAD_REQUEST, "{body}: {error}");
        assert_eq!(error["type"], "invalid_request", "{body}");
        assert!(
            error["error"]
                .as_str()
                .unwrap()
                .starts_with("a turn's body is"),
            "{error}"
        );
        assert!(!error.to_string().contains("BODY-SECRET"), "{error}");
        assert!(!error.to_string().contains("PROMPT-SECRET"), "{error}");
    }
    no_runs(&app).await;
}

/// A turn with nothing to say is refused before anything runs.
#[tokio::test]
async fn a_turn_with_neither_text_nor_an_image_is_refused() {
    let (state, app) = test_state_and_app(CorsConfig::AllowAll).await;
    let id = chat(&state, ConversationSettings::default()).await;

    let body = json!({ "conversation_id": id, "content": "  " });
    let (status, error) = call_json(&app, Method::PUT, &agent("a1"), Some(body)).await;

    assert_eq!(status, StatusCode::BAD_REQUEST, "{error}");
    assert_eq!(error["type"], "invalid_request");
    no_runs(&app).await;
}

/// Each key of a turn that can be refused is read, and refused before the
/// run is made: `folder` for a path that is no folder, `draw` where this
/// machine cannot draw, and `image_model` without `draw`.
#[tokio::test]
async fn the_folder_draw_and_image_model_a_turn_says_are_judged_before_it_runs() {
    let (state, app) = test_state_and_app(CorsConfig::AllowAll).await;
    let id = chat(&state, ConversationSettings::default()).await;
    let with = |key: &str, value: Value| {
        let mut body = turn(id);
        body[key] = value;
        body
    };
    let cases = [
        (with("folder", json!("relative/path")), "folder_unusable"),
        (with("draw", json!(true)), "drawing_unavailable"),
        (with("image_model", json!(3)), "invalid_request"),
    ];

    for (body, code) in cases {
        let (status, error) = call_json(&app, Method::PUT, &agent("a1"), Some(body.clone())).await;

        assert_eq!(status, StatusCode::BAD_REQUEST, "{body}: {error}");
        assert_eq!(error["type"], code, "{body}");
    }
    no_runs(&app).await;
    let rows = state.core.chat_history().get_messages(id).await.unwrap();
    assert!(rows.is_empty(), "no row was written");
}

/// A chat with no model of its own, and nothing running or set as the
/// default, has nothing to run on.
#[tokio::test]
async fn a_chat_with_no_model_and_none_to_fall_back_on_is_a_422() {
    let (state, app) = test_state_and_app(CorsConfig::AllowAll).await;
    let id = state
        .core
        .chat_history()
        .create_conversation(NewConversation {
            title: "t".to_owned(),
            ..NewConversation::default()
        })
        .await
        .unwrap();

    let (status, error) = call_json(&app, Method::PUT, &agent("a1"), Some(turn(id))).await;

    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "{error}");
    assert_eq!(error["type"], "no_model");
    no_runs(&app).await;
}

/// A turn that passes every check is made at once, `201`, before its model
/// is found: here none can load, so the run then fails, coded, and nothing
/// was written.
#[tokio::test]
async fn a_turn_that_passes_is_made_then_fails_where_its_model_cannot_load() {
    let (state, app) = test_state_and_app(CorsConfig::AllowAll).await;
    let id = chat(&state, ConversationSettings::default()).await;

    let (status, info) = call_json(&app, Method::PUT, &agent("a1"), Some(turn(id))).await;

    assert_eq!(status, StatusCode::CREATED, "{info}");
    let info: RunInfo = serde_json::from_value(info).unwrap();
    assert_eq!(
        (info.kind, info.conversation_id),
        (RunKind::Agent, Some(id))
    );
    let run = ended(&app, "a1").await;
    assert_eq!(run.status, RunStatus::Failed);
    assert_eq!(run.error.unwrap().code, "model_unavailable");
    let rows = state.core.chat_history().get_messages(id).await.unwrap();
    assert!(rows.is_empty(), "no row was written");
}

/// A chat on the paired machine's model, while that machine is away, is a
/// run that fails coded `conflict`: the page shows a failed run's reason.
#[tokio::test]
async fn a_chat_on_the_paired_machine_while_it_is_away_is_a_failed_run_coded_conflict() {
    let (state, app) = test_state_and_app(CorsConfig::AllowAll).await;
    let id = chat(&state, on_desk()).await;

    let (status, info) = call_json(&app, Method::PUT, &agent("p1"), Some(turn(id))).await;

    assert_eq!(status, StatusCode::CREATED, "{info}");
    let run = ended(&app, "p1").await;
    assert_eq!(run.status, RunStatus::Failed);
    assert_eq!(run.model.as_deref(), Some("qwen-desk"));
    assert_eq!(run.error.unwrap().code, "conflict");
    let rows = state.core.chat_history().get_messages(id).await.unwrap();
    assert!(rows.is_empty(), "no row was written");
}

/// A repeat answers with the run that has the id before anything is
/// checked, as a chat run's repeat does, so a client that lost the answer
/// can ask again even while the run holds the last slot.
#[tokio::test]
async fn a_repeated_id_answers_with_its_run_before_any_check() {
    let (_, app) = test_state_and_app(CorsConfig::AllowAll).await;
    let chat = json!({ "model": "qwen", "messages": [] });
    let (status, _) = call_json(&app, Method::PUT, &run_path("r1"), Some(chat)).await;
    assert_eq!(status, StatusCode::CREATED);

    let (status, info) = call_json(&app, Method::PUT, &agent("r1"), Some(turn(4242))).await;

    assert_eq!(status, StatusCode::OK, "{info}");
    let info: RunInfo = serde_json::from_value(info).unwrap();
    assert_eq!((info.id.as_str(), info.kind), ("r1", RunKind::Chat));
}

#[tokio::test]
async fn an_unknown_kind_is_refused() {
    let (_, app) = test_state_and_app(CorsConfig::AllowAll).await;
    let uri = format!("{}?kind=essay", run_path("a1"));

    let (status, error) = call_json(&app, Method::PUT, &uri, Some(turn(1))).await;

    assert_eq!(status, StatusCode::BAD_REQUEST, "{error}");
    assert_eq!(error["type"], "invalid_request");
    no_runs(&app).await;
}

/// `kind=chat` is the chat path, with a chat request's body.
#[tokio::test]
async fn kind_chat_starts_a_chat_run() {
    let (_, app) = test_state_and_app(CorsConfig::AllowAll).await;
    let uri = format!("{}?kind=chat", run_path("c1"));
    let chat = json!({ "model": "qwen", "messages": [] });

    let (status, info) = call_json(&app, Method::PUT, &uri, Some(chat)).await;

    assert_eq!(status, StatusCode::CREATED, "{info}");
    let info: RunInfo = serde_json::from_value(info).unwrap();
    assert_eq!(info.kind, RunKind::Chat);
}

/// `draw` and `image_model` belong in a turn's body. In the query they are
/// refused, in the proxy's words, rather than dropped: the turn would run
/// and draw nothing.
#[tokio::test]
async fn draw_in_an_agent_runs_query_is_refused_as_the_proxy_refuses_it() {
    let (_, app) = test_state_and_app(CorsConfig::AllowAll).await;
    for query in ["draw=true", "image_model=3"] {
        let uri = format!("{}&{query}", agent("a1"));

        let (status, error) = call_json(&app, Method::PUT, &uri, Some(turn(1))).await;

        assert_eq!(status, StatusCode::BAD_REQUEST, "{query}: {error}");
        assert_eq!(error["type"], "invalid_request", "{query}");
        assert_eq!(
            error["error"],
            gglib_proxy::runs::AGENT_DRAW_IN_QUERY,
            "{query}"
        );
    }
    no_runs(&app).await;
}
