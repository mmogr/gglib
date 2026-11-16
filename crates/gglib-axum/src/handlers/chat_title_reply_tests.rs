//! Tests for [`super`]: the request as the server on the port is posted it,
//! and what is read from that server's reply.

use std::sync::{Arc, Mutex};

use axum::Router;
use axum::http::StatusCode;
use axum::routing::post;
use gglib_core::domain::{DefaultsOrigin, NewModel};
use serde_json::json;

use super::*;
use crate::handlers::agent::run_fixture::state;

/// What a stand-in llama-server was posted, once it has been.
type Seen = Arc<Mutex<Option<Value>>>;

/// A llama-server on a port of its own that answers every completion with
/// `status` and `reply`, and keeps the body it was posted.
async fn llama_server(status: StatusCode, reply: &str) -> (u16, Seen) {
    let seen = Seen::default();
    let (keep, reply) = (Arc::clone(&seen), reply.to_owned());
    let app = Router::new().route(
        "/v1/chat/completions",
        post(move |Json(body): Json<Value>| async move {
            *keep.lock().expect("the lock") = Some(body);
            (status, reply)
        }),
    );
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .expect("a loopback port");
    let port = listener.local_addr().expect("its address").port();
    tokio::spawn(async move { axum::serve(listener, app).await });
    (port, seen)
}

/// A reply whose text is `text`.
fn reply(text: &str) -> String {
    json!({ "choices": [{ "message": { "role": "assistant", "content": text } }] }).to_string()
}

/// The handler from the port check on, over a real catalog and settings: the
/// page's JSON in, and the request the model's server is posted out.
#[tokio::test]
async fn a_title_request_reaches_the_server_with_its_cap_shaped_for_the_model_served() {
    let (_dir, state) = state().await;
    let mut model = NewModel::new(
        "qwen".to_owned(),
        "/models/qwen.gguf".into(),
        Some(7.0),
        chrono::Utc::now(),
    );
    model.inference_defaults = Some(InferenceConfig {
        top_k: Some(33),
        max_tokens: Some(4096),
        ..InferenceConfig::default()
    });
    model.defaults_origin = Some(DefaultsOrigin::User);
    let model_id = state.core.models().add(model).await.expect("a model").id;
    let mut settings = state.core.settings().get().await.expect("settings");
    settings.inference_defaults = Some(InferenceConfig {
        top_p: Some(0.5),
        ..InferenceConfig::default()
    });
    state.core.settings().save(&settings).await.expect("saved");
    let (port, seen) = llama_server(StatusCode::OK, &reply("Cat breeds")).await;
    // Named otherwise than its catalog row: the row is found by its id.
    let server = ServerInfo {
        model_id,
        model_name: "served under another name".to_owned(),
        pid: None,
        port,
        started_at: 0,
        runtime: gglib_core::domain::RuntimeKind::Llama,
        launch: None,
    };
    let request: ChatTitleRequest = serde_json::from_value(json!({
        "port": port,
        "messages": [{ "role": "user", "content": "What is a tabby?" }],
        "temperature": 0.7,
        "max_tokens": 20,
    }))
    .expect("a title request");

    let title = title_from(&state, &server, request).await.expect("a title");

    assert_eq!(title, "Cat breeds");
    let sent = seen.lock().expect("the lock").clone().expect("a request");
    assert_eq!(sent["max_tokens"], 20, "{sent}");
    assert_eq!(sent["reasoning_budget_tokens"], 0, "{sent}");
    assert_eq!(sent["top_k"], 33, "the model's own default: {sent}");
    assert_eq!(sent["top_p"], 0.5, "the settings' default: {sent}");
    assert_eq!(sent["stream"], false, "{sent}");
}

/// The pinned llama-server's answer to a title request with the budget alone,
/// recorded 2026-11-16 from b12110 serving Qwen3-4B `Q4_K_M` on a Windows 11 x64
/// VM's CPU, launched with `--jinja --reasoning-format deepseek`; the
/// request was this module's shape with `"max_tokens":20`,
/// `"temperature":0.7`, `"seed":1` and `"reasoning_budget_tokens":0`. The
/// recordings are described in `gglib-runtime`'s
/// `tests/thinking_off_recordings.rs`.
const TITLE_BUDGET_ONLY: &str =
    include_str!("../../tests/fixtures/llama_b12110_qwen3_title_budget0.json");

/// The same request with `"chat_template_kwargs":{"enable_thinking":false}`
/// after the budget.
const TITLE_WITH_THE_KWARG: &str = include_str!(
    "../../tests/fixtures/llama_b12110_qwen3_title_budget0_enable_thinking_false.json"
);

/// A title is asked for with thinking off, and the template is told so
/// beside the budget: with the budget alone a hybrid model's title is the
/// start of its reasoning.
#[tokio::test]
async fn a_title_is_asked_for_with_the_templates_thinking_switched_off() {
    let (_dir, state) = state().await;
    let model = NewModel::new(
        "qwen3".to_owned(),
        "/models/qwen3.gguf".into(),
        Some(4.0),
        chrono::Utc::now(),
    );
    let model_id = state.core.models().add(model).await.expect("a model").id;
    let (port, seen) = llama_server(StatusCode::OK, TITLE_WITH_THE_KWARG).await;
    let server = ServerInfo {
        model_id,
        model_name: "qwen3".to_owned(),
        pid: None,
        port,
        started_at: 0,
        runtime: gglib_core::domain::RuntimeKind::Llama,
        launch: None,
    };
    let request: ChatTitleRequest = serde_json::from_value(json!({
        "port": port,
        "messages": [
            { "role": "user", "content": "What is the capital of France?" },
            { "role": "assistant", "content": "The capital of France is Paris." },
            { "role": "user", "content": "Write a short title (at most six words) for this conversation. Reply with the title only." },
        ],
        "temperature": 0.7,
        "max_tokens": 20,
    }))
    .expect("a title request");

    let title = title_from(&state, &server, request).await.expect("a title");

    assert_eq!(title, "Paris is the capital of France.");
    let sent = seen.lock().expect("the lock").clone().expect("a request");
    assert_eq!(sent["reasoning_budget_tokens"], 0, "{sent}");
    assert_eq!(
        sent["chat_template_kwargs"],
        json!({ "enable_thinking": false }),
        "{sent}"
    );
}

/// What the page is given is the model's text as it came: with the budget
/// alone, the reasoning cut off at the cap. The page's cleanup removes paired
/// tags only, so this was the chat's title; gglib does not parse it.
#[tokio::test]
async fn the_recorded_title_replies_are_read_as_the_model_gave_them() {
    let (port, _) = llama_server(StatusCode::OK, TITLE_BUDGET_ONLY).await;
    let leaked = ask(port, &json!({})).await.expect("a reply");
    assert_eq!(
        leaked,
        "Okay, the user asked for a short title for the conversation about the capital of France"
    );

    let (port, _) = llama_server(StatusCode::OK, TITLE_WITH_THE_KWARG).await;
    let title = ask(port, &json!({})).await.expect("a reply");
    assert_eq!(title, "Paris is the capital of France.");
}

#[tokio::test]
async fn the_server_on_the_port_is_posted_the_request_and_its_reply_text_is_the_answer() {
    let (port, seen) = llama_server(StatusCode::OK, &reply("Cat breeds")).await;
    let body = json!({
        "messages": [{ "role": "user", "content": "What is a tabby?" }],
        "max_tokens": 20,
        "stream": false,
    });

    let text = ask(port, &body).await.expect("a reply");

    assert_eq!(text, "Cat breeds");
    assert_eq!(seen.lock().expect("the lock").as_ref(), Some(&body));
}

#[tokio::test]
async fn a_reply_with_no_text_is_an_empty_answer() {
    for reply in [
        json!({ "choices": [{ "message": { "role": "assistant", "content": null } }] }),
        json!({ "choices": [] }),
    ] {
        let (port, _) = llama_server(StatusCode::OK, &reply.to_string()).await;

        assert_eq!(ask(port, &json!({})).await.expect("a reply"), "");
    }
}

#[tokio::test]
async fn a_refusal_from_the_model_server_is_a_500_that_quotes_it() {
    let (port, _) = llama_server(StatusCode::BAD_REQUEST, "the template failed").await;

    let refused = ask(port, &json!({})).await.expect_err("a refusal");

    assert!(
        matches!(
            &refused,
            HttpError::Internal(why)
                if why == "llama-server returned 400 Bad Request: the template failed"
        ),
        "{refused:?}"
    );
}

#[tokio::test]
async fn a_reply_that_is_not_json_is_a_500() {
    let (port, _) = llama_server(StatusCode::OK, "not json").await;

    let refused = ask(port, &json!({}))
        .await
        .expect_err("an unreadable reply");

    assert!(
        matches!(&refused, HttpError::Internal(why) if why.starts_with("Failed to parse")),
        "{refused:?}"
    );
}

#[tokio::test]
async fn a_port_nothing_listens_on_is_a_503() {
    // Bound, read and closed: a port that was free a moment ago.
    let port = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .expect("a loopback port")
        .local_addr()
        .expect("its address")
        .port();

    let refused = ask(port, &json!({})).await.expect_err("nothing there");

    assert!(
        matches!(&refused, HttpError::ServiceUnavailable(why) if why.contains(&port.to_string())),
        "{refused:?}"
    );
}
