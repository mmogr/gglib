//! What a turn whose tool call a repair replaced, or tried to, is counted as
//! having generated.

use serde_json::json;

use super::forward_generation_fixtures::{
    Client, call_continues, call_opens, done, drain, reasoning, usage,
};
use super::*;

/// A `read_file` request as forwarded upstream, on an `auto` turn.
fn read_file_request() -> Bytes {
    let body = json!({
        "model": "m",
        "stream": true,
        "tool_choice": "auto",
        "messages": [{"role": "user", "content": "read it"}],
        "tools": [{"type": "function", "function": {"name": "read_file", "parameters": {
            "type": "object",
            "properties": {"path": {"type": "string"}, "max_lines": {"type": "integer"}},
            "required": ["path"]
        }}}]
    });
    Bytes::from(serde_json::to_vec(&body).unwrap())
}

/// A turn of one `read_file` call that lacks its required `path`, after four
/// characters of reasoning.
fn bad_call_turn() -> Vec<(u64, LlmStreamEvent)> {
    vec![
        (0, reasoning("hmm.")),
        (0, call_opens(0, "read_file")),
        (0, call_continues(0, "{\"max_lines\":42}")),
        (0, done("tool_calls")),
        (0, usage(37)),
    ]
}

/// A re-issue's answer: two conformant calls, with reasoning and a usage of
/// its own, neither of which is the first generation's.
fn two_good_calls() -> serde_json::Value {
    let call = |id: &str, path: &str| {
        json!({"id": id, "type": "function", "function": {
            "name": "read_file", "arguments": json!({"path": path}).to_string()
        }})
    };
    json!({
        "choices": [{"index": 0, "finish_reason": "stop", "message": {
            "role": "assistant",
            "reasoning_content": "a second, longer thought",
            "tool_calls": [call("call_a", "a.rs"), call("call_b", "b.rs")]
        }}],
        "usage": {"prompt_tokens": 11, "completion_tokens": 99, "total_tokens": 110}
    })
}

/// Drain [`bad_call_turn`] with repair on, the re-issue answered with
/// `status` and `answer`.
async fn drain_reissued(status: u16, answer: serde_json::Value) -> StreamOutcome {
    use axum::routing::post;

    let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .expect("bind");
    let port = listener.local_addr().unwrap().port();
    let app = axum::Router::new().route(
        "/v1/chat/completions",
        post(move || {
            let body = answer.to_string();
            async move {
                axum::response::Response::builder()
                    .status(status)
                    .header("content-type", "application/json")
                    .body(axum::body::Body::from(body))
                    .unwrap()
            }
        }),
    );
    let server = tokio::spawn(async move {
        let _ = axum::serve(listener, app).await;
    });
    let repair = RepairContext {
        req_builder: crate::loopback::client()
            .post(format!("http://127.0.0.1:{port}/v1/chat/completions")),
        request_body: read_file_request(),
        enabled: true,
    };
    let outcome = drain(bad_call_turn(), Some(repair), Client::Stays).await;
    server.abort();
    outcome
}

/// A repaired turn counts the calls that replaced the original, and keeps
/// the first generation's tokens, reasoning and finish reason.
#[tokio::test]
async fn a_repaired_turn_counts_the_calls_that_replaced_the_original() {
    let outcome = drain_reissued(200, two_good_calls()).await;

    assert!(outcome.repair_succeeded, "the re-issue replaced the call");
    assert_eq!(outcome.tool_calls, 2, "the two sent, not the one held");
    assert_eq!(outcome.completion_tokens, Some(37), "not the re-issue's 99");
    assert_eq!(outcome.reasoning_chars, 4, "not the re-issue's reasoning");
    assert_eq!(outcome.finish_reason.as_deref(), Some("tool_calls"));
}

/// A re-issue that fails leaves the original call to go out, and it is the
/// one counted.
#[tokio::test]
async fn a_failed_reissue_counts_the_original_call() {
    let outcome = drain_reissued(500, two_good_calls()).await;

    assert!(outcome.repair_attempted && !outcome.repair_succeeded);
    assert_eq!(outcome.tool_calls, 1);
    assert_eq!(outcome.completion_tokens, Some(37));
}
