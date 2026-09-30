//! Each forward against a fake far proxy on a loopback port: the path it
//! reaches, the key it carries, the body it sends, and what comes back.

use std::sync::{Arc, Mutex};
use std::time::Duration;

use axum::Router;
use axum::body::{Body, Bytes, to_bytes};
use axum::extract::{Request, State};
use axum::http::{StatusCode, header};
use axum::response::Response;
use gglib_app_services::FarChats;
use http_body_util::BodyExt;
use tokio::sync::{Notify, mpsc};
use tokio_stream::wrappers::ReceiverStream;

use super::{
    RemoteTurnBody, add_turn_via, cancel_run_via, list_chats_via, list_runs_via, open_chat_via,
    run_events_via,
};

const KEY: &str = "sk-far-key-for-this-device";

/// One request the far proxy saw.
#[derive(Debug, Clone)]
struct Seen {
    method: String,
    uri: String,
    bearer: Option<String>,
    body: String,
}

/// What the far proxy answers, and what it saw.
struct Fake {
    seen: Mutex<Vec<Seen>>,
    status: Mutex<u16>,
    body: Mutex<String>,
    retry_after: Mutex<Option<&'static str>>,
    /// Holds a run's stream after its first frame until notified.
    release: Notify,
}

async fn answer(State(fake): State<Arc<Fake>>, request: Request) -> Response {
    let (parts, body) = request.into_parts();
    let body = to_bytes(body, usize::MAX).await.unwrap();
    fake.seen.lock().unwrap().push(Seen {
        method: parts.method.to_string(),
        uri: parts.uri.to_string(),
        bearer: parts
            .headers
            .get(header::AUTHORIZATION)
            .map(|v| v.to_str().unwrap().to_owned()),
        body: String::from_utf8_lossy(&body).into_owned(),
    });
    if parts.uri.path().ends_with("/events") {
        let (tx, rx) = mpsc::channel::<Result<Bytes, std::io::Error>>(4);
        let fake = Arc::clone(&fake);
        tokio::spawn(async move {
            let frame = |text: &str| Ok(Bytes::from(text.to_owned()));
            tx.send(frame("id: 1\ndata: {\"n\":1}\n\n")).await.unwrap();
            fake.release.notified().await;
            tx.send(frame("id: 2\ndata: {\"n\":2}\n\n")).await.unwrap();
            tx.send(frame("event: run\ndata: {\"status\":\"completed\"}\n\n"))
                .await
                .unwrap();
        });
        return Response::builder()
            .header(header::CONTENT_TYPE, "text/event-stream")
            .body(Body::from_stream(ReceiverStream::new(rx)))
            .unwrap();
    }
    let mut response = Response::builder()
        .status(*fake.status.lock().unwrap())
        .header(header::CONTENT_TYPE, "application/json");
    if let Some(value) = *fake.retry_after.lock().unwrap() {
        response = response.header(header::RETRY_AFTER, value);
    }
    response
        .body(Body::from(fake.body.lock().unwrap().clone()))
        .unwrap()
}

/// A fake far proxy answering `status` with `body`, and a `FarChats` at it.
async fn far(status: u16, body: &str) -> (Arc<Fake>, FarChats) {
    let fake = Arc::new(Fake {
        seen: Mutex::new(Vec::new()),
        status: Mutex::new(status),
        body: Mutex::new(body.to_owned()),
        retry_after: Mutex::new(None),
        release: Notify::new(),
    });
    let app = Router::new().fallback(answer).with_state(Arc::clone(&fake));
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = listener.local_addr().unwrap().port();
    tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
    let client = FarChats::new(&format!("http://127.0.0.1:{port}/v1"), KEY).unwrap();
    (fake, client)
}

fn only(fake: &Fake) -> Seen {
    let seen = fake.seen.lock().unwrap();
    assert_eq!(seen.len(), 1, "{seen:?}");
    seen[0].clone()
}

async fn read(response: Response) -> (StatusCode, String) {
    let status = response.status();
    let bytes = to_bytes(response.into_body(), usize::MAX).await.unwrap();
    (status, String::from_utf8_lossy(&bytes).into_owned())
}

fn json(text: &str) -> serde_json::Value {
    serde_json::from_str(text).unwrap()
}

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
    assert_eq!(
        seen.bearer.as_deref(),
        Some(format!("Bearer {KEY}").as_str())
    );
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
    assert_eq!(
        seen.bearer.as_deref(),
        Some(format!("Bearer {KEY}").as_str())
    );
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
    assert_eq!(
        seen.bearer.as_deref(),
        Some(format!("Bearer {KEY}").as_str())
    );
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

#[tokio::test]
async fn a_far_refusal_keeps_its_status_and_code_in_the_daemons_shape() {
    for (status, code) in [
        (404, "not_found"),
        (403, "device_not_paired"),
        (409, "conflict"),
        (409, "no_model"),
        (503, "unavailable"),
    ] {
        let far_body = format!(
            r#"{{"error":{{"message":"refused as {code}","type":"invalid_request_error","code":"{code}"}}}}"#
        );
        let (_, far) = far(status, &far_body).await;

        let (shown, body) = read(open_chat_via(&far, 12).await.unwrap()).await;

        assert_eq!(shown.as_u16(), status);
        assert_eq!(
            json(&body),
            serde_json::json!({ "error": format!("refused as {code}"), "status": status, "type": code })
        );
    }
}

#[tokio::test]
async fn a_busy_far_machine_says_when_to_come_back() {
    let (fake, far) = far(429, r#"{"error":{"message":"busy","code":"agent_busy"}}"#).await;
    *fake.retry_after.lock().unwrap() = Some("3");

    let response = add_turn_via(
        &far,
        12,
        "chat-1",
        RemoteTurnBody {
            content: "x".into(),
        },
    )
    .await
    .unwrap();

    assert_eq!(response.status(), StatusCode::TOO_MANY_REQUESTS);
    assert_eq!(response.headers().get(header::RETRY_AFTER).unwrap(), "3");
}

/// A `401` from here would have the page ask for this daemon's key.
#[tokio::test]
async fn a_refused_key_is_a_conflict_that_says_to_pair_again() {
    let (_, far) = far(401, r#"{"error":{"message":"Invalid API key"}}"#).await;

    let (status, body) = read(list_chats_via(&far).await.unwrap()).await;

    assert_eq!(status, StatusCode::CONFLICT);
    let body = json(&body);
    assert_eq!(body["type"], "key_refused");
    assert!(
        body["error"]
            .as_str()
            .unwrap()
            .contains("gglib remote invite")
    );
}

/// Frame one arrives while the far run is still writing: the stream is
/// passed on as it comes, in order, never gathered first.
#[tokio::test]
async fn a_runs_events_stream_through_in_order_as_they_come() {
    let (fake, far) = far(200, "").await;

    let response = run_events_via(&far, "chat-1", 3).await.unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(
        response.headers().get(header::CONTENT_TYPE).unwrap(),
        "text/event-stream"
    );
    let seen = only(&fake);
    assert_eq!(seen.uri, "/v1/runs/chat-1/events?after=3");
    assert_eq!(
        seen.bearer.as_deref(),
        Some(format!("Bearer {KEY}").as_str())
    );

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
