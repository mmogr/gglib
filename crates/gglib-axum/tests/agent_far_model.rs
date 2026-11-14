//! A chat on a model of the paired machine, named by that machine and its
//! id there: this machine's turn on it reaches the far branch, and with no
//! tunnel up its run fails for the honest reason, that nothing is
//! connected.
//!
//! And a conversation the page makes for a far model keeps that model, by
//! its machine, from the moment it is made, as one made for a model of this
//! machine keeps that one, by its id too.

mod common;

use axum::body::Body;
use axum::http::{Request, StatusCode};
use http_body_util::BodyExt;
use tower::ServiceExt;

use common::harness::{test_app, test_state_and_app};
use common::origin::authed;
use gglib_core::CorsConfig;

async fn body_json(response: axum::response::Response) -> serde_json::Value {
    let bytes = response.into_body().collect().await.unwrap().to_bytes();
    serde_json::from_slice(&bytes).unwrap_or_default()
}

/// This machine's turn on a chat of the paired machine's model, with no
/// tunnel up, is a run that fails `conflict` in the words that say nothing
/// is connected, having written no row.
#[tokio::test]
async fn a_turn_on_a_far_chat_with_nothing_connected_fails_for_the_honest_reason() {
    use gglib_core::contracts::http::daemon::run_path;
    use gglib_core::domain::chat::{ConversationSettings, NewConversation};
    use gglib_core::domain::runs::{RunInfo, RunStatus};
    use gglib_core::domain::{Machine, ModelRef};

    let (state, app) = test_state_and_app(CorsConfig::AllowAll).await;
    let settings = ConversationSettings {
        model: Some(ModelRef {
            machine: Machine::Paired {
                fingerprint: "0a1b2c3d4e5f".to_owned(),
            },
            id: 3,
        }),
        ..ConversationSettings::default()
    };
    let id = state
        .core
        .chat_history()
        .create_conversation(NewConversation {
            title: "t".to_owned(),
            settings: Some(settings),
            ..NewConversation::default()
        })
        .await
        .unwrap();
    let turn = serde_json::json!({ "conversation_id": id, "content": "hi" }).to_string();

    let put = format!("{}?kind=agent", run_path("f1"));
    let made = app
        .clone()
        .oneshot(request("PUT", put, &turn))
        .await
        .unwrap();
    assert_eq!(made.status(), StatusCode::CREATED);
    let ended = async {
        loop {
            let read = request("GET", run_path("f1"), "");
            let info = body_json(app.clone().oneshot(read).await.unwrap()).await;
            let info: RunInfo = serde_json::from_value(info).unwrap();
            if !matches!(info.status, RunStatus::Queued | RunStatus::InProgress) {
                return info;
            }
            tokio::time::sleep(std::time::Duration::from_millis(10)).await;
        }
    };
    let run = tokio::time::timeout(std::time::Duration::from_secs(10), ended)
        .await
        .expect("the run ends");

    assert_eq!(run.status, RunStatus::Failed);
    let error = run.error.expect("a failed run says why");
    assert_eq!(error.code, "conflict");
    assert!(error.message.contains("not connected"), "{}", error.message);
    let rows = state.core.chat_history().get_messages(id).await.unwrap();
    assert!(rows.is_empty(), "no row was written");
}

fn request(method: &str, uri: String, body: &str) -> Request<Body> {
    authed()
        .method(method)
        .uri(uri)
        .header("Host", "127.0.0.1:9887")
        .header("content-type", "application/json")
        .body(Body::from(body.to_owned()))
        .unwrap()
}

/// Make a conversation with `body`, and read back what was stored.
async fn made(body: &str) -> serde_json::Value {
    made_in(test_app(CorsConfig::AllowAll).await, body).await
}

/// [`made`], in `app`.
async fn made_in(app: axum::Router, body: &str) -> serde_json::Value {
    let created = app
        .clone()
        .oneshot(request("POST", "/api/conversations".to_owned(), body))
        .await
        .unwrap();
    assert_eq!(created.status(), StatusCode::OK);
    let id = body_json(created).await;
    let read = app
        .oneshot(request("GET", format!("/api/conversations/{id}"), ""))
        .await
        .unwrap();
    body_json(read).await
}

/// A conversation made for the paired machine's model keeps it in its
/// settings, so its next turn from either door is that machine's; it names
/// no model of this machine.
#[tokio::test]
async fn a_conversation_made_for_a_far_model_keeps_it() {
    let far = serde_json::json!({ "machine": { "kind": "paired", "fingerprint": "0a1b2c3d4e5f" }, "id": 3 });
    let body =
        serde_json::json!({ "title": "t", "model_id": null, "system_prompt": null, "model": far });

    let stored = made(&body.to_string()).await;

    assert_eq!(stored["settings"]["model"], far, "{stored}");
    assert_eq!(stored["model_id"], serde_json::Value::Null, "{stored}");
}

/// One made for no model stores no settings.
#[tokio::test]
async fn a_conversation_made_for_no_model_names_none() {
    let stored = made(r#"{"title":"t","model_id":null,"system_prompt":null}"#).await;

    assert_eq!(stored["settings"], serde_json::Value::Null, "{stored}");
}

/// A daemon with one model in its catalogue: its router, and the model's id.
async fn with_a_model() -> (axum::Router, i64) {
    let (state, app) = test_state_and_app(CorsConfig::AllowAll).await;
    let model = gglib_core::domain::NewModel::new(
        "qwen3".to_owned(),
        std::path::PathBuf::from("/models/qwen3.gguf"),
        Some(7.0),
        chrono::Utc::now(),
    );
    let id = state.core.models().add(model).await.unwrap().id;
    (app, id)
}

/// One made for a model of this machine names it in its settings and by its
/// `model_id`, and the two agree: the settings' model decides the id.
#[tokio::test]
async fn a_conversation_made_for_a_local_model_names_it_by_its_id_too() {
    let (app, id) = with_a_model().await;
    let here = serde_json::json!({ "machine": { "kind": "local" }, "id": id });
    let body =
        serde_json::json!({ "title": "t", "model_id": null, "system_prompt": "p", "model": here });

    let stored = made_in(app, &body.to_string()).await;

    assert_eq!(stored["settings"], serde_json::json!({ "model": here }));
    assert_eq!(stored["model_id"], id, "{stored}");
    assert_eq!(stored["system_prompt"], "p", "{stored}");
}

/// One made with a `model_id` and no model keeps the id, and stores no
/// settings; a far model beside the id leaves the id out, since it is not
/// that machine's.
#[tokio::test]
async fn a_model_id_is_kept_unless_the_conversation_is_made_for_a_far_model() {
    let (app, id) = with_a_model().await;
    let far = serde_json::json!({ "machine": { "kind": "paired", "fingerprint": "0a1b2c3d4e5f" }, "id": id });
    let alone = serde_json::json!({ "title": "t", "model_id": id, "system_prompt": null });
    let beside =
        serde_json::json!({ "title": "t", "model_id": id, "system_prompt": null, "model": far });

    let kept = made_in(app.clone(), &alone.to_string()).await;
    let left_out = made_in(app, &beside.to_string()).await;

    assert_eq!(kept["model_id"], id, "{kept}");
    assert_eq!(kept["settings"], serde_json::Value::Null, "{kept}");
    assert_eq!(left_out["model_id"], serde_json::Value::Null, "{left_out}");
    assert_eq!(left_out["settings"]["model"], far, "{left_out}");
}
