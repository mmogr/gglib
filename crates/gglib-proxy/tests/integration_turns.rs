//! `PUT /v1/runs/{id}?kind=agent` on the real proxy: a paired device adds a
//! turn to one of the hub's chats, and nothing else may.
//!
//! The starter is a stand-in (`fixtures::turns`) that records what it is
//! handed, because what is under test is the door: who may, the body, and
//! the refusal's shape. The run itself is the daemon's, tested there.

use std::sync::Arc;

use gglib_core::domain::hub_chats::HubTurn;
use gglib_core::ports::TurnRefused;
use reqwest::{Client, RequestBuilder, StatusCode};
use serde_json::json;

mod fixtures;
use fixtures::remote::from_device;
use fixtures::runs::{FakeRuns, code, json};
use fixtures::tunnel::DEVICE;
use fixtures::turns::{FakeTurns, serve};

fn put(base: &str, body: &serde_json::Value) -> RequestBuilder {
    Client::new()
        .put(format!("{base}/v1/runs/d1?kind=agent"))
        .json(body)
}

fn body() -> serde_json::Value {
    json!({ "conversation_id": 7, "content": "carry on" })
}

#[tokio::test]
async fn a_named_devices_turn_reaches_the_starter_in_its_name() {
    let turns = Arc::new(FakeTurns::default());
    let runs = Arc::new(FakeRuns::default());
    let (base, cancel) = serve(Some(Arc::clone(&turns)), Arc::clone(&runs)).await;
    let (status, run) = json(from_device(put(&base, &body())).send().await.unwrap()).await;
    assert_eq!(status, StatusCode::CREATED, "{run}");
    assert_eq!(run["kind"], "agent");
    assert_eq!(run["device"], DEVICE);
    let started = turns.started.lock().unwrap().clone();
    let turn = HubTurn {
        conversation_id: 7,
        content: "carry on".to_owned(),
        images: Vec::new(),
        thinking: None,
        answer_saved: false,
        draw: false,
    };
    assert_eq!(started, vec![(DEVICE.to_owned(), "d1".to_owned(), turn)]);
    assert!(runs.scopes().is_empty(), "no chat run was made");
    cancel.cancel();
}

/// A turn's images reach the starter as their ids, in order, and a turn
/// may be its images alone. An image that is not an id is a body that is
/// not a turn.
#[tokio::test]
async fn a_turns_images_reach_the_starter_by_id() {
    use gglib_core::domain::AttachmentId;

    let turns = Arc::new(FakeTurns::default());
    let (base, cancel) = serve(Some(Arc::clone(&turns)), Arc::default()).await;
    let images = vec![AttachmentId::of(b"one"), AttachmentId::of(b"two")];
    let sent = json!({ "conversation_id": 7, "content": "", "images": images });

    let (status, run) = json(from_device(put(&base, &sent)).send().await.unwrap()).await;

    assert_eq!(status, StatusCode::CREATED, "{run}");
    let turn = HubTurn {
        conversation_id: 7,
        content: String::new(),
        images,
        thinking: None,
        answer_saved: false,
        draw: false,
    };
    let started = turns.started.lock().unwrap().clone();
    assert_eq!(started, vec![(DEVICE.to_owned(), "d1".to_owned(), turn)]);

    let named = json!({ "conversation_id": 7, "content": "x", "images": ["shot.png"] });
    let (status, answer) = json(from_device(put(&base, &named)).send().await.unwrap()).await;
    assert_eq!(status, StatusCode::BAD_REQUEST, "{answer}");
    assert_eq!(code(&answer), "invalid_request");
    cancel.cancel();
}

/// A turn's Thinking choice reaches the starter as it was said. A word that
/// is neither is a body that is not a turn, and the refusal names the key.
#[tokio::test]
async fn a_turns_thinking_choice_reaches_the_starter_and_an_unknown_word_is_400() {
    use gglib_core::domain::Thinking;

    let turns = Arc::new(FakeTurns::default());
    let (base, cancel) = serve(Some(Arc::clone(&turns)), Arc::default()).await;
    for (word, choice) in [("off", Thinking::Off), ("default", Thinking::Default)] {
        let sent = json!({ "conversation_id": 7, "content": "carry on", "thinking": word });
        let (status, run) = json(from_device(put(&base, &sent)).send().await.unwrap()).await;
        assert_eq!(status, StatusCode::CREATED, "{run}");
        let started = turns.started.lock().unwrap().clone();
        assert_eq!(started.last().unwrap().2.thinking, Some(choice));
    }

    for word in [json!("on"), json!(0)] {
        let bad = json!({ "conversation_id": 7, "content": "x", "thinking": word });
        let (status, answer) = json(from_device(put(&base, &bad)).send().await.unwrap()).await;
        assert_eq!(status, StatusCode::BAD_REQUEST, "{answer}");
        assert_eq!(code(&answer), "invalid_request");
        assert!(answer.to_string().contains("thinking"), "{answer}");
    }
    assert_eq!(turns.started.lock().unwrap().len(), 2);
    cancel.cancel();
}

/// This machine adds turns to its chats at `/api`; here only a named device
/// may.
#[tokio::test]
async fn a_turn_not_tunnelled_from_a_named_device_is_refused() {
    let turns = Arc::new(FakeTurns::default());
    let (base, cancel) = serve(Some(Arc::clone(&turns)), Arc::default()).await;
    let (status, answer) = json(put(&base, &body()).send().await.unwrap()).await;
    assert_eq!(status, StatusCode::FORBIDDEN, "{answer}");
    assert_eq!(code(&answer), "device_not_named");
    assert!(turns.started.lock().unwrap().is_empty());
    cancel.cancel();
}

#[tokio::test]
async fn a_body_that_is_not_a_turn_is_400_and_echoes_nothing() {
    let turns = Arc::new(FakeTurns::default());
    let (base, cancel) = serve(Some(Arc::clone(&turns)), Arc::default()).await;
    let secret = "zzq-private-words";
    for bad in [
        json!({ "content": secret }),
        json!({ "conversation_id": "seven", "content": secret }),
        json!({ "messages": [{ "role": "user", "content": secret }] }),
        json!({ "conversation_id": 7, "content": secret, "model": "m" }),
        json!({ "conversation_id": 7, "content": secret, "replace_from": 3 }),
    ] {
        let (status, answer) = json(from_device(put(&base, &bad)).send().await.unwrap()).await;
        assert_eq!(status, StatusCode::BAD_REQUEST, "{answer}");
        assert_eq!(code(&answer), "invalid_request");
        assert!(!answer.to_string().contains(secret), "{answer}");
    }
    assert!(turns.started.lock().unwrap().is_empty());
    cancel.cancel();
}

/// A refusal comes back with the starter's status, code and message: the
/// one-live-reply `conflict` among them.
#[tokio::test]
async fn the_starters_refusal_is_answered_as_it_was_given() {
    let turns = Arc::new(FakeTurns::default());
    *turns.fail.lock().unwrap() = Some(TurnRefused {
        status: 409,
        code: "conflict".to_owned(),
        message: "conversation 7 already has a live reply, run d0; stop it or wait".to_owned(),
    });
    let (base, cancel) = serve(Some(Arc::clone(&turns)), Arc::default()).await;
    let (status, answer) = json(from_device(put(&base, &body())).send().await.unwrap()).await;
    assert_eq!(status, StatusCode::CONFLICT, "{answer}");
    assert_eq!(code(&answer), "conflict");
    assert_eq!(
        answer["error"]["message"],
        "conversation 7 already has a live reply, run d0; stop it or wait"
    );
    cancel.cancel();
}

#[tokio::test]
async fn a_proxy_without_a_starter_answers_503() {
    let (base, cancel) = serve(None, Arc::default()).await;
    let (status, answer) = json(from_device(put(&base, &body())).send().await.unwrap()).await;
    assert_eq!(status, StatusCode::SERVICE_UNAVAILABLE, "{answer}");
    assert_eq!(code(&answer), "runs_unavailable");
    cancel.cancel();
}

#[tokio::test]
async fn an_unknown_kind_is_400() {
    let (base, cancel) = serve(None, Arc::default()).await;
    let request = Client::new()
        .put(format!("{base}/v1/runs/d1?kind=zzq"))
        .json(&body());
    let (status, answer) = json(from_device(request).send().await.unwrap()).await;
    assert_eq!(status, StatusCode::BAD_REQUEST, "{answer}");
    assert_eq!(code(&answer), "invalid_request");
    assert!(!answer.to_string().contains("zzq"), "{answer}");
    cancel.cancel();
}

fn put_chat(base: &str, query: &str, body: &serde_json::Value) -> RequestBuilder {
    Client::new()
        .put(format!("{base}/v1/runs/p1{query}"))
        .json(body)
}

fn chat_request() -> serde_json::Value {
    json!({ "model": "qwen", "messages": [{ "role": "user", "content": "a red fox" }] })
}

/// `?kind=chat&tools=builtin` hands the device's unchanged `OpenAI` request
/// to the daemon's starter, with `draw` as the query said it, and the run
/// it answers says its events are the agent loop's. No chat run is made.
#[tokio::test]
async fn a_chat_run_with_builtins_reaches_the_starter_with_its_body_and_draw() {
    let turns = Arc::new(FakeTurns::default());
    let runs = Arc::new(FakeRuns::default());
    let (base, cancel) = serve(Some(Arc::clone(&turns)), Arc::clone(&runs)).await;

    for (query, draw) in [
        ("?kind=chat&tools=builtin&draw=true", true),
        ("?kind=chat&tools=builtin", false),
        ("?tools=builtin&draw=false", false),
    ] {
        let sent = from_device(put_chat(&base, query, &chat_request()));
        let (status, run) = json(sent.send().await.unwrap()).await;
        assert_eq!(status, StatusCode::CREATED, "{run}");
        assert_eq!(
            (&run["kind"], &run["frames"]),
            (&json!("chat"), &json!("agent"))
        );
        assert_eq!(run["device"], DEVICE);
        let started = turns.chats.lock().unwrap().last().cloned().unwrap();
        assert_eq!(
            started,
            (DEVICE.to_owned(), "p1".to_owned(), chat_request(), draw)
        );
    }
    assert!(turns.started.lock().unwrap().is_empty(), "no hub turn");
    assert!(runs.scopes().is_empty(), "no chat run was made");
    cancel.cancel();
}

/// Without `tools` a chat run is what it always was: made by the runs, its
/// body untouched, the starter never asked, and its answer carries no
/// `frames` key.
#[tokio::test]
async fn a_chat_run_without_tools_is_a_plain_chat_run() {
    let turns = Arc::new(FakeTurns::default());
    let runs = Arc::new(FakeRuns::default());
    let (base, cancel) = serve(Some(Arc::clone(&turns)), Arc::clone(&runs)).await;

    for query in ["", "?kind=chat"] {
        let sent = from_device(put_chat(&base, query, &chat_request()));
        let (status, run) = json(sent.send().await.unwrap()).await;
        assert_eq!(status, StatusCode::CREATED, "{run}");
        assert_eq!(run["kind"], "chat");
        assert!(run.get("frames").is_none(), "{run}");
    }
    assert_eq!(runs.scopes().len(), 2);
    assert!(turns.chats.lock().unwrap().is_empty());
    cancel.cancel();
}

/// Only a named device may, `draw` means nothing without `tools=builtin`
/// and is refused beside `kind=agent`, `tools` says only `builtin`, and the
/// starter's refusal comes back as it was given.
#[tokio::test]
async fn a_chat_run_with_builtins_is_refused_as_a_turn_is() {
    let turns = Arc::new(FakeTurns::default());
    let runs = Arc::new(FakeRuns::default());
    let (base, cancel) = serve(Some(Arc::clone(&turns)), Arc::clone(&runs)).await;

    let local = put_chat(&base, "?tools=builtin", &chat_request());
    let (status, answer) = json(local.send().await.unwrap()).await;
    assert_eq!(status, StatusCode::FORBIDDEN, "{answer}");
    assert_eq!(code(&answer), "device_not_named");

    for query in [
        "?draw=true",
        "?kind=chat&draw=true",
        "?tools=mcp",
        "?tools=builtin&draw=yes",
    ] {
        let sent = from_device(put_chat(&base, query, &chat_request()));
        let (status, answer) = json(sent.send().await.unwrap()).await;
        assert_eq!(status, StatusCode::BAD_REQUEST, "{query}: {answer}");
        assert_eq!(code(&answer), "invalid_request");
    }
    assert!(turns.chats.lock().unwrap().is_empty());
    assert!(runs.scopes().is_empty());

    // A turn on a hub chat says `draw` in its body: the query flag is
    // refused there, not ignored, and no turn starts.
    let turn = json!({ "conversation_id": 7, "content": "a fox" });
    let sent = from_device(put_chat(&base, "?kind=agent&draw=true", &turn));
    let (status, answer) = json(sent.send().await.unwrap()).await;
    assert_eq!(status, StatusCode::BAD_REQUEST, "{answer}");
    assert_eq!(code(&answer), "invalid_request");
    let message = answer["error"]["message"].as_str().unwrap();
    assert!(message.contains("in its body"), "{message}");
    assert!(turns.started.lock().unwrap().is_empty());

    *turns.fail.lock().unwrap() = Some(TurnRefused {
        status: 400,
        code: "drawing_unavailable".to_owned(),
        message: "there is no image model on this machine".to_owned(),
    });
    let sent = from_device(put_chat(&base, "?tools=builtin&draw=true", &chat_request()));
    let (status, answer) = json(sent.send().await.unwrap()).await;
    assert_eq!(status, StatusCode::BAD_REQUEST, "{answer}");
    assert_eq!(code(&answer), "drawing_unavailable");
    cancel.cancel();
}
