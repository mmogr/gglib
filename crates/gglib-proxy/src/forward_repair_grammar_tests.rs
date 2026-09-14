//! Streamed turns on a dialect model, repaired end to end: the bad call
//! arrives as dialect markup, is held back, asked for again, and replaced
//! before the client sees it.
//!
//! The second request's answer is markup too. On a turn gglib's own grammar
//! constrained it keeps `tool_choice: "none"`, and llama-server parses no tool
//! calls under `none`, so the answer is read the way a non-streaming response
//! is before repair judges it.

use super::*;
use serde_json::json;

/// A call missing its required `path`, as a Qwen model writes it, split
/// across two frames.
const BAD_CALL: [&str; 2] = [
    "<tool_call>\n<function=read_file>\n<parameter=max_lines>\n4242\n",
    "</parameter>\n</function>\n</tool_call>",
];

/// A conformant call, as the same model writes it.
const FIXED_CALL: &str = "<tool_call>\n<function=read_file>\n<parameter=path>\nfixed.rs\n</parameter>\n</function>\n</tool_call>";

fn sse_frame(delta: &serde_json::Value, finish_reason: Option<&str>) -> String {
    let frame = json!({"choices": [{"index": 0, "delta": delta, "finish_reason": finish_reason}]});
    format!("data: {frame}\n\n")
}

/// A llama-server stand-in that parses no tool calls itself: the stream
/// carries the bad call as text, and the non-streaming second request is
/// answered with the fixed call, also as text. Every request is recorded.
///
/// It dispatches on `stream`, not `tool_choice`, since what the second
/// request carries is what the tests assert.
async fn spawn_markup_mock(
    seen: Arc<std::sync::Mutex<Vec<serde_json::Value>>>,
) -> (u16, tokio::task::JoinHandle<()>) {
    use axum::routing::post;

    let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .expect("bind");
    let port = listener.local_addr().unwrap().port();
    let app = axum::Router::new().route(
        "/v1/chat/completions",
        post(move |body: String| {
            let seen = Arc::clone(&seen);
            async move {
                let parsed: serde_json::Value =
                    serde_json::from_str(&body).unwrap_or(serde_json::Value::Null);
                let second = parsed.get("stream") == Some(&json!(false));
                seen.lock().unwrap().push(parsed);
                let (content_type, body) = if second {
                    let answer = json!({"choices": [{
                        "index": 0,
                        "message": {"role": "assistant", "content": FIXED_CALL},
                        "finish_reason": "stop"
                    }]});
                    ("application/json", answer.to_string())
                } else {
                    let frames = [
                        sse_frame(&json!({"content": BAD_CALL[0]}), None),
                        sse_frame(&json!({"content": BAD_CALL[1]}), None),
                        sse_frame(&json!({}), Some("stop")),
                    ];
                    ("text/event-stream", frames.concat() + "data: [DONE]\n\n")
                };
                axum::response::Response::builder()
                    .header("content-type", content_type)
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

/// A `read_file` request as forwarded upstream, with `extra` merged in.
fn forwarded_body(extra: serde_json::Value) -> Bytes {
    let mut body = json!({
        "model": "m",
        "stream": true,
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

/// One streamed turn on a Qwen model: its outcome, the requests upstream
/// saw, and everything the client was sent.
async fn run_turn(
    body: Bytes,
    turn: RepairTurn,
) -> (StreamOutcome, Vec<serde_json::Value>, String) {
    let seen = Arc::new(std::sync::Mutex::new(Vec::new()));
    let (port, server) = spawn_markup_mock(Arc::clone(&seen)).await;
    let url = format!("http://127.0.0.1:{port}/v1/chat/completions");
    let client = Client::new();
    let resp = client
        .post(&url)
        .body(body.clone())
        .send()
        .await
        .expect("mock upstream reachable");
    let registry = Arc::new(crate::connections::ActiveConnectionsRegistry::new());
    let connection = registry.register("m", true, None);
    let (tx, mut rx) = tokio::sync::mpsc::channel(64);

    let outcome = stream_response_to_channel(
        resp,
        "m".to_owned(),
        Some(DialectSpec::qwen_xml()),
        tx,
        &connection,
        Some(RepairContext {
            req_builder: client.post(&url),
            request_body: body,
            turn,
        }),
        false,
    )
    .await;

    let mut wire = String::new();
    while let Ok(Some(Ok(chunk))) =
        tokio::time::timeout(std::time::Duration::from_secs(5), rx.recv()).await
    {
        wire.push_str(&String::from_utf8_lossy(&chunk));
    }
    server.abort();
    let requests = seen.lock().unwrap().clone();
    (outcome, requests, wire)
}

/// Stage 6 installed gglib's grammar: the call is judged, drawn again under
/// the same grammar, and the second draw's markup is read as a call.
#[tokio::test]
async fn a_bad_call_under_gglibs_grammar_is_drawn_again_before_the_client_sees_it() {
    let body = forwarded_body(json!({
        "tool_choice": "none",
        "grammar": "root ::= \"<tool_call>\" [^<]* \"</tool_call>\""
    }));
    let turn = RepairTurn {
        enabled: true,
        gglib_grammar: true,
    };
    let (outcome, requests, wire) = run_turn(body, turn).await;

    assert!(outcome.repair_attempted, "the bad call was judged");
    assert!(
        outcome.repair_succeeded,
        "and the second draw replaced it: {wire}"
    );
    assert_eq!(requests.len(), 2, "one original, one second draw");
    assert_eq!(
        requests[1]["tool_choice"], "none",
        "the second draw keeps the tool choice gglib's grammar needs"
    );
    assert!(requests[1]["grammar"].is_string(), "and the grammar");
    assert_eq!(requests[1]["stream"], false);
    assert!(
        wire.contains("fixed.rs"),
        "the fixed call reaches the client: {wire}"
    );
    assert!(!wire.contains("4242"), "the bad call does not: {wire}");
}

/// An `auto` turn's re-issue demands a call; when the answer is still
/// markup, it is read as a call all the same.
#[tokio::test]
async fn a_re_issue_answered_in_markup_is_read_as_a_call() {
    let body = forwarded_body(json!({"tool_choice": "auto"}));
    let (outcome, requests, wire) = run_turn(body, RepairTurn::ON).await;

    assert!(
        outcome.repair_succeeded,
        "the re-issue replaced the call: {wire}"
    );
    assert_eq!(requests[1]["tool_choice"], "required");
    assert!(
        wire.contains("fixed.rs"),
        "the fixed call reaches the client: {wire}"
    );
    assert!(!wire.contains("4242"), "the bad call does not: {wire}");
}
