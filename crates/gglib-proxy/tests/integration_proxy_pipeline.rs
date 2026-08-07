//! End-to-end round-trip tests for the proxy normalization pipeline.
//!
//! Each test:
//!
//! 1. Spins up a **mock upstream** HTTP server that streams a fixture's bytes
//!    in response to `POST /v1/chat/completions`.
//! 2. Spins up the **real `gglib-proxy`** with mock ports — the runtime
//!    points at the mock upstream and the catalog returns a `ModelSummary`
//!    whose `tags` select the dialect parser under test.
//! 3. Sends a streaming chat-completion request from a strict external
//!    client (plain `reqwest`).
//! 4. Collects the proxy's response bytes and parses every `data:` frame as
//!    JSON.
//! 5. Asserts the **post-normalization** wire format — the bytes external
//!    clients (OpenWebUI, OpenAI SDKs) would actually see.
//!
//! No `gglib_core::sse::*` types are used in the assertions; the tests speak
//! pure HTTP + JSON, exactly like an external consumer.

use futures_util::StreamExt as _;
use reqwest::Client;
use serde_json::{Value, json};
use tokio_util::sync::CancellationToken;

mod fixtures;
use fixtures::common::{
    assert_sse_canonical_envelope, parse_sse_frames, spawn_mock_upstream, spawn_proxy,
    spawn_proxy_with_dialect,
};
use fixtures::sse::{
    BASIC_TEXT, DERIVED_MARKER_TOOL_CALL, MALFORMED_JSON_RECOVERY, QWEN_FUNCTION_XML_TOOL_CALL,
    QWEN_XML_TOOL_CALL, RAW_MARKUP_SPLIT_ACROSS_FRAMES, REASONING_DEEPSEEK, REASONING_ONLY,
    STANDARD_OPENAI_TOOL_CALL, basic_text_split_chunks,
};

// ─── End-to-end driver ─────────────────────────────────────────────────────

/// Send a streaming chat-completion request to the proxy and collect the
/// raw response bytes.
async fn round_trip(
    upstream_chunks: Vec<&'static [u8]>,
    model_name: &str,
    tags: Vec<String>,
) -> String {
    let upstream_cancel = CancellationToken::new();
    let upstream_port = spawn_mock_upstream(upstream_chunks, upstream_cancel.clone()).await;
    let (proxy_url, proxy_cancel) = spawn_proxy(upstream_port, model_name, tags).await;

    let client = Client::new();
    let resp = client
        .post(format!("{proxy_url}/v1/chat/completions"))
        .json(&json!({
            "model": model_name,
            "stream": true,
            "messages": [{"role": "user", "content": "hi"}],
        }))
        .send()
        .await
        .expect("proxy request");

    assert_eq!(resp.status(), 200, "proxy returned non-200");
    assert_eq!(
        resp.headers()
            .get("content-type")
            .and_then(|v| v.to_str().ok()),
        Some("text/event-stream")
    );

    // Drain the streaming body into a single String — tests parse data:
    // frames out of it the same way an external client would.
    let mut body = Vec::new();
    let mut stream = resp.bytes_stream();
    while let Some(chunk) = stream.next().await {
        body.extend_from_slice(&chunk.expect("body chunk"));
    }

    proxy_cancel.cancel();
    upstream_cancel.cancel();

    String::from_utf8(body).expect("utf-8 body")
}

/// Like [`round_trip`], but also polls `GET /v1/proxy/status` afterwards
/// until `predicate` accepts the dashboard snapshot (or a timeout expires),
/// returning the body and the final snapshot. The drift alarm back-patches
/// its flag from the streaming task after the body finishes, hence the
/// bounded poll rather than a single read.
async fn round_trip_with_status(
    upstream_chunks: Vec<&'static [u8]>,
    model_name: &str,
    tags: Vec<String>,
    predicate: impl Fn(&Value) -> bool,
) -> (String, Value) {
    let upstream_cancel = CancellationToken::new();
    let upstream_port = spawn_mock_upstream(upstream_chunks, upstream_cancel.clone()).await;
    let (proxy_url, proxy_cancel) = spawn_proxy(upstream_port, model_name, tags).await;

    let client = Client::new();
    let resp = client
        .post(format!("{proxy_url}/v1/chat/completions"))
        .json(&json!({
            "model": model_name,
            "stream": true,
            "messages": [{"role": "user", "content": "hi"}],
        }))
        .send()
        .await
        .expect("proxy request");
    assert_eq!(resp.status(), 200);

    let mut body = Vec::new();
    let mut stream = resp.bytes_stream();
    while let Some(chunk) = stream.next().await {
        body.extend_from_slice(&chunk.expect("body chunk"));
    }

    let mut status = Value::Null;
    for _ in 0..50 {
        status = client
            .get(format!("{proxy_url}/v1/proxy/status"))
            .send()
            .await
            .expect("status request")
            .json()
            .await
            .expect("status json");
        if predicate(&status) {
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
    }

    proxy_cancel.cancel();
    upstream_cancel.cancel();
    (String::from_utf8(body).expect("utf-8 body"), status)
}

// ═════════════════════════════════════════════════════════════════════════════
// Tests
// ═════════════════════════════════════════════════════════════════════════════

/// Vanilla streaming text — the proxy must re-emit content deltas verbatim
/// and terminate with `data: [DONE]`.
#[tokio::test]
async fn basic_text_round_trip() {
    let body = round_trip(vec![BASIC_TEXT], "test-model", vec![]).await;
    let (frames, saw_done) = parse_sse_frames(&body);
    assert!(saw_done, "missing [DONE] terminator");
    assert_sse_canonical_envelope(&frames, "test-model");

    // Reconstruct the visible text from delta.content fields.
    let text: String = frames
        .iter()
        .filter_map(|f| f["choices"][0]["delta"]["content"].as_str())
        .collect();
    assert_eq!(text, "Hello, world");

    // The stop chunk has finish_reason="stop" and an empty delta.
    let stop = frames
        .iter()
        .find(|f| f["choices"][0]["finish_reason"] == "stop")
        .expect("missing stop chunk");
    assert!(
        stop["choices"][0]["delta"]
            .as_object()
            .is_some_and(|o| o.is_empty()),
        "stop chunk should have empty delta, got {stop}"
    );
}

/// DeepSeek/QwQ-style reasoning frames must surface as `reasoning_content`
/// deltas, with text content following.
#[tokio::test]
async fn reasoning_content_round_trip() {
    let body = round_trip(vec![REASONING_DEEPSEEK], "r1-test", vec![]).await;
    let (frames, saw_done) = parse_sse_frames(&body);
    assert!(saw_done);
    assert_sse_canonical_envelope(&frames, "r1-test");

    let reasoning: String = frames
        .iter()
        .filter_map(|f| f["choices"][0]["delta"]["reasoning_content"].as_str())
        .collect();
    let content: String = frames
        .iter()
        .filter_map(|f| f["choices"][0]["delta"]["content"].as_str())
        .collect();
    assert_eq!(reasoning, "Let me think.");
    assert_eq!(content, "42");
}

/// A turn that produces `reasoning_content` but never any `content` renders as
/// an empty response in clients that collapse reasoning. The proxy must promote
/// the stranded text into the content channel, flagged, so the turn is usable.
#[tokio::test]
async fn reasoning_only_response_is_promoted_to_content() {
    let body = round_trip(vec![REASONING_ONLY], "r1-test", vec![]).await;
    let (frames, saw_done) = parse_sse_frames(&body);
    assert!(saw_done);

    let reasoning: String = frames
        .iter()
        .filter_map(|f| f["choices"][0]["delta"]["reasoning_content"].as_str())
        .collect();
    let content: String = frames
        .iter()
        .filter_map(|f| f["choices"][0]["delta"]["content"].as_str())
        .collect();

    // Reasoning still reaches the client untouched on its own channel.
    assert_eq!(reasoning, "The answer is 42.");
    // ...and is also promoted into content, so the turn is not empty.
    assert!(
        content.contains("The answer is 42."),
        "stranded reasoning should be promoted into content, got {content:?}"
    );
    assert!(
        content.contains("reasoning-only response"),
        "promotion must be flagged so the degradation stays visible, got {content:?}"
    );
    // The wholly-empty diagnostic is a different path and must not fire here.
    assert!(
        !content.contains("produced no output"),
        "empty-stream notice must not fire for a reasoning-only turn, got {content:?}"
    );
}

/// Qwen XML tool calls must be rewritten into strict OpenAI `tool_calls`
/// deltas — the `<tool_call>…</tool_call>` markers must NOT appear in the
/// rebuilt content stream.
#[tokio::test]
async fn qwen_xml_tool_call_is_normalized() {
    let body = round_trip(
        vec![QWEN_XML_TOOL_CALL],
        "qwen3-coder",
        vec!["format:qwen-xml".to_owned()],
    )
    .await;
    let (frames, saw_done) = parse_sse_frames(&body);
    assert!(saw_done);
    assert_sse_canonical_envelope(&frames, "qwen3-coder");

    // The literal Qwen markup MUST NOT appear in the wire output.
    assert!(
        !body.contains("<tool_call>") && !body.contains("</tool_call>"),
        "Qwen XML markers leaked into wire output:\n{body}"
    );

    // Reconstruct the visible content — should only contain the leading prose.
    let content: String = frames
        .iter()
        .filter_map(|f| f["choices"][0]["delta"]["content"].as_str())
        .collect();
    assert_eq!(content, "Looking it up. ");

    // Find the tool_calls delta(s).
    let tc_frames: Vec<&Value> = frames
        .iter()
        .filter(|f| f["choices"][0]["delta"]["tool_calls"].is_array())
        .collect();
    assert!(
        !tc_frames.is_empty(),
        "expected at least one tool_calls delta"
    );

    // First tool_call delta must carry id + type:"function" + function.name.
    let first_tc = &tc_frames[0]["choices"][0]["delta"]["tool_calls"][0];
    assert_eq!(first_tc["index"], json!(0));
    assert!(
        first_tc["id"].is_string(),
        "first tool_call delta missing id"
    );
    assert_eq!(first_tc["type"], "function");
    assert_eq!(first_tc["function"]["name"], "get_weather");

    // The cumulative arguments JSON must reconstruct to the original args.
    let mut args = String::new();
    for f in &tc_frames {
        if let Some(s) = f["choices"][0]["delta"]["tool_calls"][0]["function"]["arguments"].as_str()
        {
            args.push_str(s);
        }
    }
    let parsed_args: Value =
        serde_json::from_str(&args).expect("tool_call arguments should be JSON");
    assert_eq!(parsed_args, json!({"city": "Paris"}));

    // Final chunk must announce finish_reason="tool_calls".
    let stop = frames
        .iter()
        .find(|f| f["choices"][0]["finish_reason"] == "tool_calls")
        .expect("missing tool_calls finish chunk");
    assert!(
        stop["choices"][0]["delta"]
            .as_object()
            .is_some_and(|o| o.is_empty())
    );
}

/// The second `format:qwen-xml` body shape — Qwen3 + `--jinja`'s
/// `<function=NAME><parameter=KEY>VALUE</parameter></function>` dialect,
/// rather than [`qwen_xml_tool_call_is_normalized`]'s JSON body — must be
/// rewritten into the same strict OpenAI `tool_calls` deltas, through the
/// full proxy pipeline rather than just the parser's own unit tests.
#[tokio::test]
async fn qwen_function_xml_tool_call_is_normalized() {
    let body = round_trip(
        vec![QWEN_FUNCTION_XML_TOOL_CALL],
        "qwen3.6",
        vec!["format:qwen-xml".to_owned()],
    )
    .await;
    let (frames, saw_done) = parse_sse_frames(&body);
    assert!(saw_done);
    assert_sse_canonical_envelope(&frames, "qwen3.6");

    // The literal Qwen markup MUST NOT appear in the wire output.
    assert!(
        !body.contains("<tool_call>")
            && !body.contains("</tool_call>")
            && !body.contains("<function=")
            && !body.contains("<parameter="),
        "Qwen inner-XML markers leaked into wire output:\n{body}"
    );

    let content: String = frames
        .iter()
        .filter_map(|f| f["choices"][0]["delta"]["content"].as_str())
        .collect();
    assert_eq!(content, "Looking it up. ");

    let tc_frames: Vec<&Value> = frames
        .iter()
        .filter(|f| f["choices"][0]["delta"]["tool_calls"].is_array())
        .collect();
    assert!(
        !tc_frames.is_empty(),
        "expected at least one tool_calls delta"
    );

    let first_tc = &tc_frames[0]["choices"][0]["delta"]["tool_calls"][0];
    assert_eq!(first_tc["index"], json!(0));
    assert!(
        first_tc["id"].is_string(),
        "first tool_call delta missing id"
    );
    assert_eq!(first_tc["type"], "function");
    assert_eq!(first_tc["function"]["name"], "get_weather");

    let mut args = String::new();
    for f in &tc_frames {
        if let Some(s) = f["choices"][0]["delta"]["tool_calls"][0]["function"]["arguments"].as_str()
        {
            args.push_str(s);
        }
    }
    let parsed_args: Value =
        serde_json::from_str(&args).expect("tool_call arguments should be JSON");
    assert_eq!(parsed_args, json!({"city": "Paris"}));

    let stop = frames
        .iter()
        .find(|f| f["choices"][0]["finish_reason"] == "tool_calls")
        .expect("missing tool_calls finish chunk");
    assert!(
        stop["choices"][0]["delta"]
            .as_object()
            .is_some_and(|o| o.is_empty())
    );
}

/// A NON-Qwen model tagged `format:hermes` — the tag detection has emitted
/// since it existed, previously wired to nothing — must now normalize its
/// `<tool_call>` markup exactly like a qwen-tagged model.  This pins the
/// gap fix: before the dialect-spec work this markup reached clients raw.
#[tokio::test]
async fn hermes_tagged_model_tool_call_is_normalized() {
    let body = round_trip(
        vec![QWEN_XML_TOOL_CALL],
        "hermes-2-pro",
        vec!["format:hermes".to_owned()],
    )
    .await;
    let (frames, saw_done) = parse_sse_frames(&body);
    assert!(saw_done);
    assert_sse_canonical_envelope(&frames, "hermes-2-pro");

    assert!(
        !body.contains("<tool_call>") && !body.contains("</tool_call>"),
        "hermes markup leaked into wire output:\n{body}"
    );

    let tc_frames: Vec<&Value> = frames
        .iter()
        .filter(|f| f["choices"][0]["delta"]["tool_calls"].is_array())
        .collect();
    assert!(!tc_frames.is_empty(), "expected a tool_calls delta");
    let first_tc = &tc_frames[0]["choices"][0]["delta"]["tool_calls"][0];
    assert_eq!(first_tc["type"], "function");
    assert_eq!(first_tc["function"]["name"], "get_weather");
    frames
        .iter()
        .find(|f| f["choices"][0]["finish_reason"] == "tool_calls")
        .expect("missing tool_calls finish chunk");
}

/// A model whose catalog row carries a template-derived spec — custom
/// multibyte markers, no `format:*` tag anywhere — must normalize through
/// the full pipeline, including a close marker split across SSE frames.
/// This is the zero-code-for-new-dialects property, end to end.
#[tokio::test]
async fn derived_spec_tool_call_is_normalized_across_frame_splits() {
    let spec = gglib_core::domain::DialectSpec {
        id: "derived".to_owned(),
        tool_open: "«TC»".to_owned(),
        tool_close: "«/TC»".to_owned(),
        body_codecs: vec![gglib_core::domain::BodyCodec::Json],
        emission: gglib_core::domain::EmissionProfile::default(),
        id_prefix: "call_dialect_".to_owned(),
    };

    let upstream_cancel = CancellationToken::new();
    let upstream_port =
        spawn_mock_upstream(vec![DERIVED_MARKER_TOOL_CALL], upstream_cancel.clone()).await;
    let (proxy_url, proxy_cancel) =
        spawn_proxy_with_dialect(upstream_port, "any-new-model", spec).await;

    let client = Client::new();
    let resp = client
        .post(format!("{proxy_url}/v1/chat/completions"))
        .json(&json!({
            "model": "any-new-model",
            "stream": true,
            "messages": [{"role": "user", "content": "hi"}],
        }))
        .send()
        .await
        .expect("proxy request");
    assert_eq!(resp.status(), 200);

    let mut body = Vec::new();
    let mut stream = resp.bytes_stream();
    while let Some(chunk) = stream.next().await {
        body.extend_from_slice(&chunk.expect("body chunk"));
    }
    proxy_cancel.cancel();
    upstream_cancel.cancel();
    let body = String::from_utf8(body).expect("utf-8 body");

    let (frames, saw_done) = parse_sse_frames(&body);
    assert!(saw_done);
    assert_sse_canonical_envelope(&frames, "any-new-model");

    assert!(
        !body.contains("«TC»") && !body.contains("«/TC»"),
        "derived markers leaked into wire output:\n{body}"
    );

    let content: String = frames
        .iter()
        .filter_map(|f| f["choices"][0]["delta"]["content"].as_str())
        .collect();
    assert_eq!(content, "Sure.  done.");

    let tc_frames: Vec<&Value> = frames
        .iter()
        .filter(|f| f["choices"][0]["delta"]["tool_calls"].is_array())
        .collect();
    assert!(!tc_frames.is_empty(), "expected a tool_calls delta");
    let first_tc = &tc_frames[0]["choices"][0]["delta"]["tool_calls"][0];
    assert_eq!(first_tc["type"], "function");
    assert_eq!(first_tc["function"]["name"], "get_weather");
    assert!(
        first_tc["id"]
            .as_str()
            .is_some_and(|id| id.starts_with("call_dialect_")),
        "derived specs mint call_dialect_ IDs, got {first_tc}"
    );

    let mut args = String::new();
    for f in &tc_frames {
        if let Some(a) = f["choices"][0]["delta"]["tool_calls"][0]["function"]["arguments"].as_str()
        {
            args.push_str(a);
        }
    }
    let parsed_args: Value = serde_json::from_str(&args).expect("arguments should be JSON");
    assert_eq!(parsed_args, json!({"city": "Paris"}));
}

/// The drift alarm end to end: an untagged, spec-less model leaks raw
/// `<tool_call>` markup — split across SSE frames — to the client
/// verbatim (unchanged behaviour), and the dashboard reports it: the
/// eviction-safe total increments and the recent-request entry is flagged.
#[tokio::test]
async fn dialect_residue_is_flagged_on_the_dashboard() {
    let (body, status) = round_trip_with_status(
        vec![RAW_MARKUP_SPLIT_ACROSS_FRAMES],
        "mystery-model",
        vec![],
        |s| s["dialect_residue_total"].as_u64() == Some(1),
    )
    .await;

    // Unchanged behaviour: the alarm observes, the client still gets the
    // raw markup.
    assert!(
        body.contains("<tool") && body.contains("_call>"),
        "passthrough must not alter the stream:\n{body}"
    );

    assert_eq!(
        status["dialect_residue_total"].as_u64(),
        Some(1),
        "drift alarm total must count the leak, got {status}"
    );
    let flagged = status["recent_requests"]
        .as_array()
        .expect("recent_requests array")
        .iter()
        .any(|r| r["dialect_residue"] == json!(true));
    assert!(
        flagged,
        "the recent-request entry must carry the flag, got {}",
        status["recent_requests"]
    );
}

/// A dialect model whose stream parses cleanly must NOT trip the alarm —
/// the markup was consumed by the parser, nothing leaked.
#[tokio::test]
async fn clean_dialect_stream_does_not_trip_the_alarm() {
    let (body, status) = round_trip_with_status(
        vec![QWEN_XML_TOOL_CALL],
        "qwen3-clean",
        vec!["format:qwen-xml".to_owned()],
        // Wait until our request shows up in the recent list; the flag, if
        // it were ever coming, is set before the body finishes streaming.
        |s| {
            s["recent_requests"]
                .as_array()
                .is_some_and(|r| !r.is_empty())
        },
    )
    .await;

    assert!(!body.contains("<tool_call>"), "markup must be consumed");
    assert_eq!(status["dialect_residue_total"].as_u64(), Some(0));
    assert!(
        status["recent_requests"]
            .as_array()
            .unwrap()
            .iter()
            .all(|r| r["dialect_residue"] == json!(false)),
        "no entry may be flagged: {}",
        status["recent_requests"]
    );
}

/// A standard OpenAI tool-call stream (no dialect) must round-trip through
/// the identity parser preserving id, type, name, and arguments.
#[tokio::test]
async fn standard_openai_tool_call_passthrough() {
    let body = round_trip(vec![STANDARD_OPENAI_TOOL_CALL], "strict-openai", vec![]).await;
    let (frames, saw_done) = parse_sse_frames(&body);
    assert!(saw_done);
    assert_sse_canonical_envelope(&frames, "strict-openai");

    let tc_frames: Vec<&Value> = frames
        .iter()
        .filter(|f| f["choices"][0]["delta"]["tool_calls"].is_array())
        .collect();
    assert!(!tc_frames.is_empty(), "expected tool_calls deltas");

    let first_tc = &tc_frames[0]["choices"][0]["delta"]["tool_calls"][0];
    assert_eq!(first_tc["index"], json!(0));
    assert_eq!(first_tc["id"], "call_abc");
    assert_eq!(first_tc["type"], "function");
    assert_eq!(first_tc["function"]["name"], "get_weather");

    let mut args = String::new();
    for f in &tc_frames {
        if let Some(s) = f["choices"][0]["delta"]["tool_calls"][0]["function"]["arguments"].as_str()
        {
            args.push_str(s);
        }
    }
    let parsed_args: Value = serde_json::from_str(&args).expect("arguments should be JSON");
    assert_eq!(parsed_args, json!({"city": "Paris"}));
}

/// SSE frames split across arbitrary byte boundaries must reassemble inside
/// `SseStreamDecoder` and produce the same content as if they had arrived
/// whole.
#[tokio::test]
async fn split_frame_round_trip() {
    let body = round_trip(basic_text_split_chunks(), "split-model", vec![]).await;
    let (frames, saw_done) = parse_sse_frames(&body);
    assert!(saw_done, "missing [DONE] terminator after split chunks");
    assert_sse_canonical_envelope(&frames, "split-model");

    let text: String = frames
        .iter()
        .filter_map(|f| f["choices"][0]["delta"]["content"].as_str())
        .collect();
    assert_eq!(text, "split-frame-1-and-2");
}

/// A malformed `data:` payload in the upstream stream is unrecoverable at
/// the SSE-frame layer (we cannot know where the next frame begins), so the
/// proxy must:
/// 1. Emit any content frames that arrived **before** the malformed frame.
/// 2. Surface a structured `error` data frame so the client sees a terminal
///    signal instead of a hang.
/// 3. Always finish with `data: [DONE]`.
#[tokio::test]
async fn malformed_json_terminates_cleanly() {
    let body = round_trip(vec![MALFORMED_JSON_RECOVERY], "noisy-model", vec![]).await;
    let (frames, saw_done) = parse_sse_frames(&body);
    assert!(saw_done, "missing [DONE] after malformed-json frame");

    // Pre-error content must have made it to the wire.
    let pre_error_text: String = frames
        .iter()
        .filter_map(|f| f["choices"][0]["delta"]["content"].as_str())
        .collect();
    assert!(
        pre_error_text.contains("before"),
        "content before malformed frame must be delivered, got {pre_error_text:?}"
    );

    // The terminator must be a structured error frame, not a half-emitted
    // chunk-completion.  This proves the client gets a clean signal instead
    // of a silent hang.
    let error_frame = frames
        .iter()
        .find(|f| f.get("error").is_some())
        .expect("expected a structured error frame on malformed JSON");
    assert!(
        error_frame["error"]["message"]
            .as_str()
            .is_some_and(|m| !m.is_empty()),
        "error frame missing message"
    );
}
