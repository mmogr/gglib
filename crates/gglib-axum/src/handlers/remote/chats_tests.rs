//! Each forward against a fake far proxy on a loopback port: the path it
//! reaches, the key it carries, the body it sends, and what comes back.

use std::time::Duration;

use axum::http::{StatusCode, header};
use http_body_util::BodyExt;

use super::super::fake_far::{KEY, carries_key, far, json, only, read};
use super::{
    RemoteTurnBody, add_turn_via, cancel_run_via, list_chats_via, list_runs_via, open_chat_via,
    run_events_via,
};

#[tokio::test]
async fn a_listing_reaches_the_far_chats_with_the_key_and_comes_back_as_it_was() {
    let listed = r#"{"chats":[{"id":12,"title":"Why","updated_at":"2026-09-30 09:13:07"}]}"#;
    let (fake, far) = far(200, listed).await;

    let (status, body) = read(list_chats_via(&far).await.unwrap()).await;

    assert_eq!((status, body.as_str()), (StatusCode::OK, listed));
    let seen = only(&fake);
    assert_eq!(
        (seen.method.as_str(), seen.uri.as_str()),
        ("GET", "/v1/chats")
    );
    assert!(carries_key(&seen), "{seen:?}");
}

#[tokio::test]
async fn opening_a_chat_reaches_it_by_id() {
    let (fake, far) = far(200, r#"{"conversation":{},"messages":[]}"#).await;

    let (status, _) = read(open_chat_via(&far, 12).await.unwrap()).await;

    assert_eq!(status, StatusCode::OK);
    let seen = only(&fake);
    assert_eq!(
        (seen.method.as_str(), seen.uri.as_str()),
        ("GET", "/v1/chats/12")
    );
    assert!(carries_key(&seen), "{seen:?}");
}

#[tokio::test]
async fn a_turn_is_the_chat_and_the_message_only_and_its_201_comes_back() {
    let started = r#"{"id":"chat-1","kind":"agent","status":"queued"}"#;
    let (fake, far) = far(201, started).await;
    let body = RemoteTurnBody {
        content: "And how do I fix it?".to_owned(),
    };

    let (status, answer) = read(add_turn_via(&far, 12, "chat-1", body).await.unwrap()).await;

    assert_eq!((status, answer.as_str()), (StatusCode::CREATED, started));
    let seen = only(&fake);
    assert_eq!(
        (seen.method.as_str(), seen.uri.as_str()),
        ("PUT", "/v1/runs/chat-1?kind=agent")
    );
    assert!(carries_key(&seen), "{seen:?}");
    assert_eq!(
        json(&seen.body),
        serde_json::json!({ "conversation_id": 12, "content": "And how do I fix it?" })
    );
}

#[tokio::test]
async fn the_runs_and_a_cancel_reach_the_far_runs() {
    let (fake, far) = far(200, r#"{"runs":[]}"#).await;

    read(list_runs_via(&far).await.unwrap()).await;
    read(cancel_run_via(&far, "chat-1").await.unwrap()).await;

    let seen = fake.seen.lock().unwrap().clone();
    let asked: Vec<_> = seen
        .iter()
        .map(|s| (s.method.as_str(), s.uri.as_str()))
        .collect();
    assert_eq!(
        asked,
        [("GET", "/v1/runs"), ("POST", "/v1/runs/chat-1/cancel")]
    );
    let key = format!("Bearer {KEY}");
    assert!(
        seen.iter()
            .all(|s| s.bearer.as_deref() == Some(key.as_str()))
    );
}

/// Frame one arrives while the far run is still writing: the stream is
/// passed on as it comes, in order, never gathered first.
#[tokio::test]
async fn a_runs_events_stream_through_in_order_as_they_come() {
    let (fake, far) = far(200, "").await;

    // Bounded: a forward that gathered the stream first would wait here for
    // an end the far run only reaches after the first frame is read.
    let response = tokio::time::timeout(Duration::from_secs(5), run_events_via(&far, "chat-1", 3))
        .await
        .expect("the events were answered before the run ended")
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(
        response.headers().get(header::CONTENT_TYPE).unwrap(),
        "text/event-stream"
    );
    let seen = only(&fake);
    assert_eq!(seen.uri, "/v1/runs/chat-1/events?after=3");
    assert!(carries_key(&seen), "{seen:?}");

    let mut body = response.into_body();
    let first = tokio::time::timeout(Duration::from_secs(5), body.frame())
        .await
        .expect("the first frame came before the run went on")
        .unwrap()
        .unwrap()
        .into_data()
        .unwrap();
    assert_eq!(first, "id: 1\ndata: {\"n\":1}\n\n");

    fake.release.notify_one();
    let rest = body.collect().await.unwrap().to_bytes();
    assert_eq!(
        rest,
        "id: 2\ndata: {\"n\":2}\n\nevent: run\ndata: {\"status\":\"completed\"}\n\n"
    );
}

#[path = "chats_refusal_tests.rs"]
mod refusals;
