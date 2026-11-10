//! `PUT /api/conversations/{id}` with `settings`: a conversation's model,
//! tools and tool limits, each one sent and no other, read back through
//! `GET /api/conversations/{id}`. A refused patch changes nothing, the title
//! sent beside it included. A chat's machine is fixed once it has a saved
//! message; before that, a model chosen for it can be chosen again anywhere.

mod common;

use axum::Router;
use axum::http::{Method, StatusCode};
use gglib_core::CorsConfig;
use gglib_core::domain::chat::{ConversationSettings, MessageRole, NewConversation, NewMessage};
use gglib_core::domain::{Machine, ModelRef, NewModel};
use serde_json::{Value, json};

use common::harness::test_state_and_app;
use common::origin::call_json;

const FAR: &str = "0a1b2c3d4e5f";

/// A daemon whose catalogue holds `qwen3` and `gemma`: its state, its
/// router, and the two ids.
async fn daemon() -> (gglib_axum::AppState, Router, i64, i64) {
    let (state, app) = test_state_and_app(CorsConfig::AllowAll).await;
    let mut ids = Vec::new();
    for name in ["qwen3", "gemma"] {
        let path = std::path::PathBuf::from(format!("/models/{name}.gguf"));
        let model = NewModel::new(name.to_owned(), path, Some(7.0), chrono::Utc::now());
        ids.push(state.core.models().add(model).await.unwrap().id);
    }
    (state, app, ids[0], ids[1])
}

fn local(id: i64) -> ModelRef {
    ModelRef {
        machine: Machine::Local,
        id,
    }
}

/// A conversation that ran on `qwen3`, a saved message and its settings:
/// tools, limits and a temperature; its id.
async fn ran_on(state: &gglib_axum::AppState, qwen: i64) -> i64 {
    let id = chosen(state, qwen).await;
    let message = NewMessage {
        conversation_id: id,
        role: MessageRole::User,
        content: "Q".to_owned(),
        metadata: None,
        images: Vec::new(),
    };
    state
        .core
        .chat_history()
        .save_message(message)
        .await
        .expect("message");
    id
}

/// A conversation whose model is chosen (qwen3, here) but which has no saved
/// message yet; its id.
async fn chosen(state: &gglib_axum::AppState, qwen: i64) -> i64 {
    let settings = ConversationSettings {
        model_name: Some("qwen3".to_owned()),
        model: Some(local(qwen)),
        tools: vec!["files".to_owned()],
        max_parallel: Some(4),
        tool_timeout_ms: Some(5_000),
        temperature: Some(0.25),
        ..ConversationSettings::default()
    };
    made(
        state,
        NewConversation {
            title: "t".into(),
            settings: Some(settings),
            ..NewConversation::default()
        },
    )
    .await
}

/// A conversation with no saved message and no settings; its id.
async fn fresh(state: &gglib_axum::AppState) -> i64 {
    let conv = NewConversation {
        title: "t".into(),
        ..NewConversation::default()
    };
    made(state, conv).await
}

async fn made(state: &gglib_axum::AppState, conv: NewConversation) -> i64 {
    state
        .core
        .chat_history()
        .create_conversation(conv)
        .await
        .unwrap()
}

async fn put(app: &Router, id: i64, body: Value) -> (StatusCode, Value) {
    call_json(
        app,
        Method::PUT,
        &format!("/api/conversations/{id}"),
        Some(body),
    )
    .await
}

async fn read(app: &Router, id: i64) -> Value {
    let (status, body) =
        call_json(app, Method::GET, &format!("/api/conversations/{id}"), None).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    body
}

/// What [`ran_on`] stores, as the route reads it, with `changed` laid over
/// its settings.
fn held(qwen: i64, changed: Value) -> Value {
    let mut settings = json!({
        "model_name": "qwen3",
        "model": { "machine": { "kind": "local" }, "id": qwen },
        "tools": ["files"],
        "max_parallel": 4,
        "tool_timeout_ms": 5000,
        "temperature": 0.25,
    });
    let Value::Object(changed) = changed else {
        panic!("a change is an object");
    };
    for (key, value) in changed {
        if value.is_null() {
            settings.as_object_mut().unwrap().remove(&key);
        } else {
            settings[key] = value;
        }
    }
    settings
}

#[tokio::test]
async fn each_setting_sent_alone_changes_only_itself() {
    let (state, app, qwen, gemma) = daemon().await;
    let cases = [
        (json!({ "tools": [] }), json!({ "tools": null })),
        (json!({ "tools": ["web"] }), json!({ "tools": ["web"] })),
        (json!({ "no_tools": true }), json!({ "no_tools": true })),
        (json!({ "max_parallel": 9 }), json!({ "max_parallel": 9 })),
        (
            json!({ "tool_timeout_ms": 700 }),
            json!({ "tool_timeout_ms": 700 }),
        ),
        (
            json!({ "model": { "machine": { "kind": "local" }, "id": gemma } }),
            json!({
                "model": { "machine": { "kind": "local" }, "id": gemma },
                "model_name": "gemma",
            }),
        ),
        (
            json!({ "max_parallel": null }),
            json!({ "max_parallel": null }),
        ),
        (
            json!({ "tool_timeout_ms": null }),
            json!({ "tool_timeout_ms": null }),
        ),
    ];
    for (patch, changed) in cases {
        let id = ran_on(&state, qwen).await;
        let (status, body) = put(&app, id, json!({ "settings": patch })).await;
        assert_eq!(status, StatusCode::OK, "{patch}: {body}");
        let stored = read(&app, id).await;
        assert_eq!(stored["settings"], held(qwen, changed), "{patch}");
        assert_eq!(stored["title"], "t", "{patch}");
    }
}

/// A model of this machine names the conversation's `model_id` too, as a
/// run on it does.
#[tokio::test]
async fn a_new_model_of_this_machine_is_its_model_id_too() {
    let (state, app, qwen, gemma) = daemon().await;
    let id = ran_on(&state, qwen).await;
    assert_eq!(read(&app, id).await["model_id"], qwen);

    let patch = json!({ "settings": { "model": { "machine": { "kind": "local" }, "id": gemma } } });
    let (status, body) = put(&app, id, patch).await;

    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(read(&app, id).await["model_id"], gemma);
}

/// A cleared model, on a chat that has not run, leaves it naming none, by
/// `model_id` either, and changes nothing else.
#[tokio::test]
async fn a_cleared_model_clears_the_model_id() {
    let (state, app, qwen, _) = daemon().await;
    let id = chosen(&state, qwen).await;

    let (status, body) = put(&app, id, json!({ "settings": { "model": null } })).await;

    assert_eq!(status, StatusCode::OK, "{body}");
    let stored = read(&app, id).await;
    assert_eq!(stored["model_id"], Value::Null);
    assert_eq!(
        stored["settings"],
        held(qwen, json!({ "model": null, "model_name": null }))
    );
}

/// A chat that has run keeps a model on its machine: clearing it would
/// forget the machine and let it continue on another. Refused, and nothing
/// is written; choosing another model there is still taken.
#[tokio::test]
async fn a_chat_that_has_run_cannot_clear_its_model() {
    let (state, app, qwen, gemma) = daemon().await;
    let id = ran_on(&state, qwen).await;
    let before = read(&app, id).await;

    let (status, body) = put(&app, id, json!({ "settings": { "model": null } })).await;

    assert_eq!(status, StatusCode::CONFLICT, "{body}");
    assert_eq!(
        body["error"],
        "this chat has run, so it keeps a model on the machine it ran on: choose another \
         model there rather than none"
    );
    assert_eq!(read(&app, id).await, before);
    let far = json!({ "machine": { "kind": "paired", "fingerprint": FAR }, "id": 3 });
    let (status, _) = put(&app, id, json!({ "settings": { "model": far } })).await;
    assert_eq!(status, StatusCode::CONFLICT);
    let here = json!({ "machine": { "kind": "local" }, "id": gemma });
    let (status, body) = put(&app, id, json!({ "settings": { "model": here } })).await;
    assert_eq!(status, StatusCode::OK, "{body}");
}

/// A patch of the values already stored writes nothing: not even the time
/// the chat was last changed, which the chat lists sort by.
#[tokio::test]
async fn a_patch_of_what_is_stored_writes_nothing() {
    let (state, app, qwen, _) = daemon().await;
    let id = ran_on(&state, qwen).await;
    let before = read(&app, id).await;
    // `updated_at` is to the second: a write a second later would show.
    tokio::time::sleep(std::time::Duration::from_millis(1_100)).await;
    let same = json!({
        "model": { "machine": { "kind": "local" }, "id": qwen },
        "tools": ["files"],
        "no_tools": false,
        "max_parallel": 4,
        "tool_timeout_ms": 5000,
    });

    let (status, body) = put(&app, id, json!({ "settings": same })).await;

    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(read(&app, id).await, before);
}

#[tokio::test]
async fn a_patch_with_nothing_in_it_changes_nothing() {
    let (state, app, qwen, _) = daemon().await;
    let id = ran_on(&state, qwen).await;
    let before = read(&app, id).await;

    for body in [json!({}), json!({ "settings": {} })] {
        let (status, answer) = put(&app, id, body.clone()).await;
        assert_eq!(status, StatusCode::OK, "{body}: {answer}");
        assert_eq!(read(&app, id).await, before, "{body}");
    }
}

#[tokio::test]
async fn a_title_alone_leaves_the_settings() {
    let (state, app, qwen, _) = daemon().await;
    let id = ran_on(&state, qwen).await;

    let (status, body) = put(&app, id, json!({ "title": "renamed" })).await;

    assert_eq!(status, StatusCode::OK, "{body}");
    let stored = read(&app, id).await;
    assert_eq!(stored["title"], "renamed");
    assert_eq!(stored["settings"], held(qwen, json!({})));
    assert_eq!(stored["model_id"], qwen);
}

/// A row saved before a model was recorded stores only its `model_id`: a
/// patch that names no model keeps it, and the machine it reads.
#[tokio::test]
async fn a_limit_on_a_row_with_only_a_model_id_keeps_the_id() {
    let (state, app, qwen, _) = daemon().await;
    let id = made(
        &state,
        NewConversation {
            title: "t".into(),
            model_id: Some(qwen),
            ..NewConversation::default()
        },
    )
    .await;

    let (status, body) = put(&app, id, json!({ "settings": { "max_parallel": 2 } })).await;

    assert_eq!(status, StatusCode::OK, "{body}");
    let stored = read(&app, id).await;
    assert_eq!(stored["model_id"], qwen, "{stored}");
    assert_eq!(stored["settings"], json!({ "max_parallel": 2 }), "{stored}");
}

#[tokio::test]
async fn a_model_this_machine_does_not_hold_is_refused_and_nothing_changes() {
    let (state, app, qwen, _) = daemon().await;
    let id = ran_on(&state, qwen).await;
    let before = read(&app, id).await;

    let patch = json!({
        "title": "renamed",
        "settings": { "model": { "machine": { "kind": "local" }, "id": 999 }, "max_parallel": 2 },
    });
    let (status, body) = put(&app, id, patch).await;

    assert_eq!(status, StatusCode::BAD_REQUEST, "{body}");
    assert_eq!(body["error"], "no model has id 999 on this machine");
    assert_eq!(read(&app, id).await, before);
}

/// A conversation that has run nowhere takes the paired machine's model,
/// by that machine, and no `model_id`: that id is not this catalogue's.
#[tokio::test]
async fn a_paired_model_is_stored_as_the_paired_machines() {
    let (state, app, qwen, _) = daemon().await;
    let id = fresh(&state).await;
    let far = json!({ "machine": { "kind": "paired", "fingerprint": FAR }, "id": qwen });

    let (status, body) = put(&app, id, json!({ "settings": { "model": far } })).await;

    assert_eq!(status, StatusCode::OK, "{body}");
    let stored = read(&app, id).await;
    assert_eq!(stored["settings"], json!({ "model": far }), "{stored}");
    assert_eq!(stored["model_id"], Value::Null, "{stored}");
}

/// A conversation's machine is fixed once it has a saved message: one that
/// ran here is not moved to the paired machine, as a run there is refused.
#[tokio::test]
async fn a_model_of_another_machine_is_refused_as_a_run_on_it_is() {
    let (state, app, qwen, _) = daemon().await;
    let id = ran_on(&state, qwen).await;
    let before = read(&app, id).await;
    let far = json!({ "machine": { "kind": "paired", "fingerprint": FAR }, "id": 3 });

    let (status, body) = put(&app, id, json!({ "settings": { "model": far } })).await;

    assert_eq!(status, StatusCode::CONFLICT, "{body}");
    assert_eq!(
        body["error"],
        "this chat ran on this machine and continues here, on one of its models"
    );
    assert_eq!(read(&app, id).await, before);
}

#[tokio::test]
async fn a_limit_outside_a_runs_bounds_is_refused_with_them() {
    let (state, app, qwen, _) = daemon().await;
    let id = ran_on(&state, qwen).await;
    let before = read(&app, id).await;
    let cases = [
        (
            json!({ "max_parallel": 0 }),
            "max_parallel must be from 1 to 50, not 0",
        ),
        (
            json!({ "max_parallel": 51 }),
            "max_parallel must be from 1 to 50, not 51",
        ),
        (
            json!({ "tool_timeout_ms": 99 }),
            "tool_timeout_ms must be from 100 to 60000, not 99",
        ),
        (
            json!({ "tool_timeout_ms": 60_001 }),
            "tool_timeout_ms must be from 100 to 60000, not 60001",
        ),
    ];
    for (patch, reason) in cases {
        let body = json!({ "title": "renamed", "settings": patch });
        let (status, answer) = put(&app, id, body).await;
        assert_eq!(status, StatusCode::BAD_REQUEST, "{patch}: {answer}");
        assert_eq!(answer["error"], reason, "{patch}");
    }
    assert_eq!(read(&app, id).await, before);
}

/// A model chosen for a chat with no saved message has not fixed its
/// machine: another machine's can be chosen in its place, either way.
#[tokio::test]
async fn a_chat_with_no_message_takes_a_model_of_either_machine_in_turn() {
    let (state, app, qwen, gemma) = daemon().await;
    let far = json!({ "machine": { "kind": "paired", "fingerprint": FAR }, "id": 3 });
    let here = json!({ "machine": { "kind": "local" }, "id": qwen });
    let other = json!({ "machine": { "kind": "local" }, "id": gemma });

    for turns in [[&far, &here, &far], [&here, &far, &other]] {
        let id = fresh(&state).await;
        for model in turns {
            let (status, body) = put(&app, id, json!({ "settings": { "model": model } })).await;
            assert_eq!(status, StatusCode::OK, "{model}: {body}");
            assert_eq!(read(&app, id).await["settings"]["model"], *model);
        }
    }
}

/// A setting the patch does not name is refused, not dropped without a word,
/// and the refusal is coded as every unreadable body is.
#[tokio::test]
async fn a_setting_the_patch_does_not_name_is_refused() {
    let (state, app, qwen, _) = daemon().await;
    let id = ran_on(&state, qwen).await;
    let before = read(&app, id).await;

    let (status, body) = put(&app, id, json!({ "settings": { "temperature": 0.9 } })).await;

    assert_eq!(status, StatusCode::BAD_REQUEST, "{body}");
    assert_eq!(body["type"], "invalid_request", "{body}");
    assert!(
        body["error"]
            .as_str()
            .unwrap()
            .contains("unknown field `temperature`"),
        "{body}"
    );
    assert_eq!(read(&app, id).await, before);
}

#[tokio::test]
async fn a_conversation_that_is_not_there_is_not_found() {
    let (_state, app, _, _) = daemon().await;

    let (status, body) = put(&app, 404, json!({ "settings": { "max_parallel": 2 } })).await;

    assert_eq!(status, StatusCode::NOT_FOUND, "{body}");
}
