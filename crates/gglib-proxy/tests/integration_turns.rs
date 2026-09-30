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
use fixtures::runs::{FakeRuns, code, json};
use fixtures::tunnel::DEVICE;
use fixtures::turns::{FakeTurns, serve};

fn put(base: &str, body: &serde_json::Value) -> RequestBuilder {
    Client::new()
        .put(format!("{base}/v1/runs/d1?kind=agent"))
        .json(body)
}

fn from_device(request: RequestBuilder) -> RequestBuilder {
    request
        .header("via", "1.1 modelpipe")
        .header("x-modelpipe-device", DEVICE)
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
    };
    assert_eq!(started, vec![(DEVICE.to_owned(), "d1".to_owned(), turn)]);
    assert!(runs.scopes().is_empty(), "no chat run was made");
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
