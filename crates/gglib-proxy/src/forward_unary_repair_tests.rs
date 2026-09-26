//! Non-streaming turns repaired end to end: the answer is judged, drawn
//! again when its call does not validate, and the client is answered with
//! the draw that validates.
//!
//! The second request's answer is read the way the first was. Under
//! `tool_choice: "none"` llama-server parses no tool calls, so a second draw
//! under gglib's grammar arrives as dialect markup and is read as a call
//! before it is judged.

use std::sync::Arc;

use reqwest::Client;
use serde_json::json;

use super::*;
use crate::metrics::ContextSnapshot;

/// A call missing its required `path`, as a Qwen model writes it.
const BAD_MARKUP: &str = "<tool_call>\n<function=read_file>\n<parameter=max_lines>\n4242\n</parameter>\n</function>\n</tool_call>";

/// A conformant call, as the same model writes it.
const FIXED_MARKUP: &str = "<tool_call>\n<function=read_file>\n<parameter=path>\nfixed.rs\n</parameter>\n</function>\n</tool_call>";

/// `max_lines` carrying a string, the way Llama 3.2 was measured wrong.
const BAD_ARGUMENTS: &str = "{\"path\":\"a\",\"max_lines\":\"42\"}";

/// The same call, conformant.
const FIXED_ARGUMENTS: &str = "{\"path\":\"a\",\"max_lines\":42}";

/// A structured answer carrying one `read_file` call.
fn structured(id: &str, arguments: &str) -> serde_json::Value {
    json!({"choices": [{"index": 0, "finish_reason": "tool_calls", "message": {
        "role": "assistant",
        "tool_calls": [{"id": id, "type": "function", "function": {"name": "read_file", "arguments": arguments}}]
    }}]})
}

/// An answer whose call is still text, as a dialect model returns it under
/// `tool_choice: "none"`.
fn markup(content: &str) -> serde_json::Value {
    json!({"choices": [{"index": 0, "finish_reason": "stop", "message": {"role": "assistant", "content": content}}]})
}

/// A llama-server stand-in that answers in order: the first request gets
/// `answers[0]`, the next `answers[1]`, and the last answer repeats; a `null`
/// answer is a 500 with an empty body. Every request is recorded.
///
/// It dispatches on the ordinal because nothing in a body tells the two
/// apart on this path: the first request is already `stream: false`, and a
/// second draw under gglib's grammar keeps the first request's tool choice.
async fn spawn_ordinal_mock(
    seen: Arc<std::sync::Mutex<Vec<serde_json::Value>>>,
    answers: Vec<serde_json::Value>,
) -> (u16, tokio::task::JoinHandle<()>) {
    use axum::routing::post;

    let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .expect("bind");
    let port = listener.local_addr().unwrap().port();
    let answers = Arc::new(answers);
    let app = axum::Router::new().route(
        "/v1/chat/completions",
        post(move |body: String| {
            let seen = Arc::clone(&seen);
            let answers = Arc::clone(&answers);
            async move {
                let parsed: serde_json::Value =
                    serde_json::from_str(&body).unwrap_or(serde_json::Value::Null);
                let ordinal = {
                    let mut seen = seen.lock().unwrap();
                    seen.push(parsed);
                    seen.len() - 1
                };
                let answer = &answers[ordinal.min(answers.len() - 1)];
                let (status, body) = match answer {
                    serde_json::Value::Null => (500, String::new()),
                    answer => (200, answer.to_string()),
                };
                axum::response::Response::builder()
                    .status(status)
                    .header("content-type", "application/json")
                    .body(axum::body::Body::from(body))
                    .unwrap()
            }
        }),
    );
    let handle = tokio::spawn(async move {
        let _ = axum::serve(listener, app).await;
    });
    (port, handle)
}

/// A `read_file` request as forwarded upstream, non-streaming, with `extra`
/// merged in.
#[allow(
    clippy::needless_pass_by_value,
    reason = "grandfathered at lint inheritance, #1157"
)]
fn forwarded_body(extra: serde_json::Value) -> Bytes {
    let mut body = json!({
        "model": "m",
        "stream": false,
        "messages": [{"role": "user", "content": "read it"}],
        "tools": [{"type": "function", "function": {"name": "read_file", "parameters": {
            "type": "object",
            "properties": {"path": {"type": "string"}, "max_lines": {"type": "integer"}},
            "required": ["path"]
        }}}]
    });
    for (key, value) in extra.as_object().unwrap() {
        body[key] = value.clone();
    }
    Bytes::from(serde_json::to_vec(&body).unwrap())
}

fn snapshot() -> ContextSnapshot {
    ContextSnapshot {
        model_name: "m".to_owned(),
        payload_chars_before: 0,
        payload_chars_after: 0,
        messages_truncated: 0,
        was_clamped: false,
        grammar_enforced: false,
        dialect_residue: false,
        tool_repaired: false,
        loop_guard_trip: None,
        seq: 0,
        recorded_at_secs: 0,
    }
}

/// One non-streaming turn: the requests upstream saw, the body the client
/// was answered with, and the metrics store that recorded it.
async fn run_turn(
    body: Bytes,
    answers: Vec<serde_json::Value>,
    dialect: Option<DialectSpec>,
    turn: RepairTurn,
) -> (
    Vec<serde_json::Value>,
    serde_json::Value,
    ContextMetricsStore,
) {
    let seen = Arc::new(std::sync::Mutex::new(Vec::new()));
    let (port, server) = spawn_ordinal_mock(Arc::clone(&seen), answers).await;
    let url = format!("http://127.0.0.1:{port}/v1/chat/completions");
    let metrics = ContextMetricsStore::new();
    let seq = metrics.record(snapshot());
    let response = forward_unary(
        Client::new().post(&url),
        body,
        dialect.as_ref(),
        &CacheMetricsStore::new(),
        &metrics,
        seq,
        turn,
    )
    .await
    .expect("mock upstream reachable");
    assert_eq!(response.status(), StatusCode::OK);
    let bytes = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    server.abort();
    let requests = seen.lock().unwrap().clone();
    let answer = serde_json::from_slice(&bytes).expect("the client is answered with JSON");
    (requests, answer, metrics)
}

/// The arguments of the first call in an answered body.
fn arguments(answer: &serde_json::Value) -> &str {
    answer["choices"][0]["message"]["tool_calls"][0]["function"]["arguments"]
        .as_str()
        .unwrap_or("")
}

/// A malformed call on an `auto` turn is drawn again with `tool_choice:
/// "required"`, and the client is answered with the draw that validates.
#[tokio::test]
async fn a_bad_unary_tool_call_is_reissued_and_the_client_gets_the_valid_one() {
    let answers = vec![
        structured("call_bad", BAD_ARGUMENTS),
        structured("call_fixed", FIXED_ARGUMENTS),
    ];
    let body = forwarded_body(json!({"tool_choice": "auto"}));
    let (requests, answer, metrics) = run_turn(body, answers, None, RepairTurn::ON).await;

    assert_eq!(requests.len(), 2, "one original, one repair");
    assert_eq!(requests[1]["tool_choice"], "required");
    assert_eq!(requests[1]["stream"], false, "the re-issue does not stream");
    assert_eq!(
        arguments(&answer),
        FIXED_ARGUMENTS,
        "the fixed call reaches the client: {answer}"
    );
    assert_eq!(
        answer["choices"][0]["message"]["tool_calls"][0]["id"],
        "call_fixed"
    );
    assert_eq!(
        (
            metrics.tool_repairs_attempted(),
            metrics.tool_repairs_succeeded()
        ),
        (1, 1),
        "the dashboard counts this path"
    );
}

/// Stage 6 installed gglib's grammar: the call is judged, drawn again under
/// the same grammar, and the second draw's markup is read as a call.
#[tokio::test]
async fn a_unary_turn_gglibs_grammar_constrained_is_answered_in_markup_and_read_as_a_call() {
    let body = forwarded_body(json!({
        "tool_choice": "none",
        "grammar": "root ::= \"<tool_call>\" [^<]* \"</tool_call>\""
    }));
    let turn = RepairTurn {
        enabled: true,
        gglib_grammar: true,
    };
    let answers = vec![markup(BAD_MARKUP), markup(FIXED_MARKUP)];
    let (requests, answer, metrics) =
        run_turn(body, answers, Some(DialectSpec::qwen_xml()), turn).await;

    assert_eq!(requests.len(), 2, "one original, one second draw");
    assert_eq!(
        requests[1]["tool_choice"], "none",
        "the second draw keeps the tool choice gglib's grammar needs"
    );
    assert!(requests[1]["grammar"].is_string(), "and the grammar");
    assert!(
        arguments(&answer).contains("fixed.rs"),
        "the fixed call reaches the client as a call: {answer}"
    );
    assert!(
        !answer.to_string().contains("4242"),
        "the bad call does not: {answer}"
    );
    assert_eq!(
        (
            metrics.tool_repairs_attempted(),
            metrics.tool_repairs_succeeded()
        ),
        (1, 1)
    );
}

/// A conformant call is answered as it came: one request, the same choices.
#[tokio::test]
async fn a_valid_unary_tool_call_is_not_reissued() {
    let good = structured("call_good", FIXED_ARGUMENTS);
    let body = forwarded_body(json!({"tool_choice": "auto"}));
    let (requests, answer, metrics) =
        run_turn(body, vec![good.clone()], None, RepairTurn::ON).await;

    assert_eq!(requests.len(), 1, "nothing to repair, nothing re-issued");
    assert_eq!(answer["choices"], good["choices"]);
    assert_eq!(metrics.tool_repairs_attempted(), 0);
}

/// A re-issue upstream rejects falls open to the first answer, and the
/// attempt is counted as one that did not succeed.
#[tokio::test]
async fn a_rejected_re_issue_falls_open_to_the_first_answer() {
    let bad = structured("call_bad", BAD_ARGUMENTS);
    let body = forwarded_body(json!({"tool_choice": "auto"}));
    let (requests, answer, metrics) =
        run_turn(body, vec![bad.clone(), json!(null)], None, RepairTurn::ON).await;

    assert_eq!(requests.len(), 2, "the re-issue was sent");
    assert_eq!(answer, bad, "and refused, so the first answer stands");
    assert_eq!(
        (
            metrics.tool_repairs_attempted(),
            metrics.tool_repairs_succeeded()
        ),
        (1, 0)
    );
}

/// With repair off, the malformed call reaches the client as it came.
#[tokio::test]
async fn repair_off_leaves_a_unary_body_as_it_came() {
    let bad = structured("call_bad", BAD_ARGUMENTS);
    let answers = vec![bad.clone(), structured("call_fixed", FIXED_ARGUMENTS)];
    let body = forwarded_body(json!({"tool_choice": "auto"}));
    let (requests, answer, metrics) = run_turn(body, answers, None, RepairTurn::OFF).await;

    assert_eq!(requests.len(), 1, "nothing is asked twice with repair off");
    assert_eq!(
        answer, bad,
        "the body is answered as it came, bad call and all"
    );
    assert_eq!(metrics.tool_repairs_attempted(), 0);
}
