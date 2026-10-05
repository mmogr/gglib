//! The far proxy's refusals, as the page is handed them: status, code and
//! `Retry-After` kept, a refused key turned into a conflict, a refused stream
//! answered as a refusal, and a turn that carries more than its text refused
//! before anything is sent.

use axum::http::{StatusCode, header};

use super::super::super::fake_far::{far, json, read};
use super::super::{RemoteTurnBody, add_turn_via, list_chats_via, open_chat_via, run_events_via};

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
            images: Vec::new(),
            thinking: None,
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

/// A far refusal of a run's events comes back as any other does, not as a
/// stream.
#[tokio::test]
async fn a_refused_run_stream_comes_back_as_a_refusal() {
    let (_, far) = far(
        404,
        r#"{"error":{"message":"no run has id chat-1","code":"run_not_found"}}"#,
    )
    .await;

    let (status, body) = read(run_events_via(&far, "chat-1", 0).await.unwrap()).await;

    assert_eq!(status, StatusCode::NOT_FOUND);
    assert_eq!(
        json(&body),
        serde_json::json!({ "error": "no run has id chat-1", "status": 404, "type": "run_not_found" })
    );
}

/// A turn is its text and nothing else: history sent along is refused.
#[test]
fn a_turn_body_that_carries_more_than_its_text_is_refused() {
    let more = r#"{"content":"hi","messages":[{"role":"user","content":"old"}]}"#;
    assert!(serde_json::from_str::<RemoteTurnBody>(more).is_err());
    let only = serde_json::from_str::<RemoteTurnBody>(r#"{"content":"hi"}"#).unwrap();
    assert_eq!(only.content, "hi");
}
