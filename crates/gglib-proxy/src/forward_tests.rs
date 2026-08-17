//! Tests for [`super`] — the proxy's request/response boundary.

use super::*;

/// A turn that died upstream indicts the server even though its error
/// frame was renderable — the conflation that kept the recycle watchdog
/// from ever firing against a server failing every request.
#[test]
fn a_turn_that_died_upstream_is_not_healthy_however_renderable_it_was() {
    let outcome = StreamOutcome {
        saw_visible_output: true,
        upstream_errored: true,
        ..Default::default()
    };
    assert_eq!(outcome.health_verdict(), StreamVerdict::UpstreamError);
}

/// Dying outranks leaving: if both happened, the server is still at fault.
#[test]
fn dying_upstream_outranks_a_client_that_also_left() {
    let outcome = StreamOutcome {
        upstream_errored: true,
        client_aborted: true,
        ..Default::default()
    };
    assert_eq!(outcome.health_verdict(), StreamVerdict::UpstreamError);
}

/// Positive evidence of production is not retracted by the client leaving
/// afterwards — otherwise every cancelled long generation would abstain.
#[test]
fn output_already_produced_outranks_a_later_client_hangup() {
    let outcome = StreamOutcome {
        saw_visible_output: true,
        client_aborted: true,
        ..Default::default()
    };
    assert_eq!(outcome.health_verdict(), StreamVerdict::Healthy);
}

/// The hang-up case that used to be scored as an empty response.
#[test]
fn a_client_leaving_before_any_output_abstains() {
    let outcome = StreamOutcome {
        client_aborted: true,
        ..Default::default()
    };
    assert_eq!(outcome.health_verdict(), StreamVerdict::ClientAborted);
}

#[test]
fn a_turn_nobody_abandoned_that_produced_nothing_is_empty() {
    assert_eq!(
        StreamOutcome::default().health_verdict(),
        StreamVerdict::Empty
    );
}

/// Reasoning is not renderable output, so a reasoning-only turn stays a
/// strike. Guards the distinction `saw_reasoning` was introduced for.
#[test]
fn a_reasoning_only_turn_is_still_empty() {
    let outcome = StreamOutcome {
        saw_reasoning: true,
        ..Default::default()
    };
    assert_eq!(outcome.health_verdict(), StreamVerdict::Empty);
}

#[test]
fn session_aware_budget_falls_back_to_live_ratio_without_a_session_id() {
    let cal = TokenCalibration::new();
    cal.record("m", 40_000, 10_000);
    let live = cal.chars_per_token("m");
    // Mirrors the exact fallback expression used in
    // forward_chat_completion's budget computation above.
    let via_none: f64 = None::<&str>.map_or_else(
        || cal.chars_per_token("m"),
        |sid| cal.session_chars_per_token("m", sid, std::time::Instant::now()),
    );
    assert_eq!(live, via_none);
}

#[test]
fn test_should_forward_header() {
    // Should forward
    assert!(should_forward_header("accept"));
    assert!(should_forward_header("content-type"));
    assert!(should_forward_header("x-custom-header"));

    // Should NOT forward
    assert!(!should_forward_header("connection"));
    assert!(!should_forward_header("host"));
    assert!(!should_forward_header("authorization"));
    assert!(!should_forward_header("transfer-encoding"));
}

#[test]
fn hop_by_hop_headers_are_case_insensitive() {
    assert!(!should_forward_header("Connection"));
    assert!(!should_forward_header("HOST"));
    assert!(!should_forward_header("Transfer-Encoding"));
    assert!(!should_forward_header("Keep-Alive"));
    assert!(!should_forward_header("PROXY-AUTHORIZATION"));
}

#[test]
fn all_hop_by_hop_headers_are_blocked() {
    for header in HOP_BY_HOP_HEADERS {
        assert!(
            !should_forward_header(header),
            "hop-by-hop header '{header}' should be blocked"
        );
    }
}

#[test]
fn common_request_headers_are_forwarded() {
    let forward_headers = [
        "accept",
        "accept-encoding",
        "accept-language",
        "user-agent",
        "content-type",
        "x-request-id",
        "x-forwarded-for",
        "cache-control",
    ];
    for header in forward_headers {
        assert!(
            should_forward_header(header),
            "request header '{header}' should be forwarded"
        );
    }
}

#[test]
fn inject_streaming_body_overrides_sets_include_usage_and_return_progress() {
    let body = Bytes::from(r#"{"model":"foo","messages":[]}"#);
    let out = inject_streaming_body_overrides(body);
    let value: serde_json::Value = serde_json::from_slice(&out).expect("valid json");
    assert_eq!(value["stream_options"]["include_usage"], true);
    assert_eq!(value["return_progress"], true);
}

#[test]
fn inject_streaming_body_overrides_forces_include_usage_even_if_client_disabled_it() {
    let body =
        Bytes::from(r#"{"model":"foo","messages":[],"stream_options":{"include_usage":false}}"#);
    let out = inject_streaming_body_overrides(body);
    let value: serde_json::Value = serde_json::from_slice(&out).expect("valid json");
    assert_eq!(
        value["stream_options"]["include_usage"], true,
        "proxy must force include_usage on regardless of client request"
    );
}

#[test]
fn inject_streaming_body_overrides_leaves_non_json_body_unchanged() {
    let body = Bytes::from_static(b"not json at all");
    let out = inject_streaming_body_overrides(body.clone());
    assert_eq!(out, body, "non-JSON bodies must pass through unchanged");
}

// The transforms themselves are tested in `gglib_core::request_pipeline`.
// What is left here is the bytes ⇄ JSON conversion unique to the proxy
// boundary, and the wire contract this surface puts on the pipeline's one
// failure mode.

fn oversized_body() -> Bytes {
    let mut messages = vec![serde_json::json!({
        "role": "tool", "tool_call_id": "c1", "content": "x".repeat(50_000)
    })];
    for _ in 0..8 {
        messages.push(serde_json::json!({"role": "user", "content": "ok"}));
    }
    Bytes::from(
        serde_json::to_vec(&serde_json::json!({"model": "m", "messages": messages})).unwrap(),
    )
}

#[test]
fn shaping_runs_the_pipeline_and_preserves_unknown_fields() {
    let body = Bytes::from(r#"{"model":"m","messages":[],"totally_made_up":{"a":1}}"#);
    let ShapedRequest {
        body: out,
        truncation: report,
        grammar_enforced,
        ..
    } = shape_request_body(
        body,
        &ModelContext::passthrough(),
        &SamplingLayers::default(),
        None,
    )
    .expect("no budget, so nothing to reject");
    assert!(!grammar_enforced, "passthrough context never constrains");

    let parsed: serde_json::Value = serde_json::from_slice(&out).expect("valid json");
    assert_eq!(parsed["cache_prompt"], true, "the pipeline ran");
    assert!(parsed["temperature"].is_number());
    assert_eq!(parsed["totally_made_up"], serde_json::json!({"a": 1}));
    assert_eq!(report, TruncationReport::default(), "unmeasured, no budget");
}

/// The end-to-end proxy view of the constrain stage: a demanded tool
/// call on a qwen-xml model reports `grammar_enforced` so the log line
/// and dashboard snapshot can say so.
#[test]
fn shaping_reports_grammar_enforcement_for_a_demanded_dialect_call() {
    let ctx = ModelContext {
        tags: vec![gglib_core::normalize::tags::FORMAT_QWEN_XML.to_owned()],
        dialect: Some(gglib_core::domain::DialectSpec::qwen_xml()),
        // Tool-capable, or stage 2b strips the tools before stage 6 sees
        // them — the correct interplay for a model that cannot call tools.
        capabilities: gglib_core::domain::ModelCapabilities::SUPPORTS_TOOL_CALLS,
        catalog_resolved: true,
        ..ModelContext::passthrough()
    };
    let body = Bytes::from(
        r#"{"model":"m","messages":[],"tools":[{"type":"function","function":{"name":"f"}}],"tool_choice":"required"}"#,
    );
    let ShapedRequest {
        body: out,
        grammar_enforced,
        ..
    } = shape_request_body(body, &ctx, &SamplingLayers::default(), None)
        .expect("nothing to reject");

    assert!(grammar_enforced);
    let parsed: serde_json::Value = serde_json::from_slice(&out).unwrap();
    assert!(parsed["grammar"].is_string());
    assert_eq!(parsed["tool_choice"], "none");
}

#[test]
fn shaping_leaves_non_json_bodies_alone() {
    let body = Bytes::from_static(b"not json at all");
    let ShapedRequest {
        body: out,
        truncation: report,
        ..
    } = shape_request_body(
        body.clone(),
        &ModelContext::passthrough(),
        &SamplingLayers::default(),
        Some(10),
    )
    .expect("a body we cannot read is forwarded, not rejected");

    assert_eq!(out, body, "must be the same bytes, not the same value");
    assert_eq!(report, TruncationReport::default());
}

#[test]
fn shaping_truncates_when_the_budget_binds() {
    let ShapedRequest {
        body: out,
        truncation: report,
        ..
    } = shape_request_body(
        oversized_body(),
        &ModelContext::passthrough(),
        &SamplingLayers::default(),
        Some(20_000),
    )
    .expect("trimming the one oversized tool result is enough");

    assert_eq!(report.messages_truncated, 1);
    assert!(report.payload_chars_after <= 20_000);
    assert!(out.len() <= 20_000);
}

#[test]
fn shaping_reports_the_error_when_the_budget_cannot_be_met() {
    let err = shape_request_body(
        oversized_body(),
        &ModelContext::passthrough(),
        &SamplingLayers::default(),
        Some(200),
    )
    .expect_err("nothing left to trim, still over");

    assert!(matches!(
        err,
        TruncationError::ExceedsBudgetAfterTruncation { .. }
    ));
}

/// The wire contract clients branch on. Asserted field by field because
/// this is a public interface of the proxy, not an internal detail: the
/// status, both codes and the message are all load-bearing.
#[tokio::test]
async fn the_context_length_contract_is_400_with_both_codes_set() {
    let response = context_length_exceeded_response();
    assert_eq!(response.status(), StatusCode::BAD_REQUEST);

    let bytes = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .expect("body reads");
    let parsed: serde_json::Value = serde_json::from_slice(&bytes).expect("valid json");

    assert_eq!(parsed["error"]["type"], "context_length_exceeded");
    assert_eq!(parsed["error"]["code"], "context_length_exceeded");
    assert_eq!(
        parsed["error"]["message"],
        "Context window limit reached. Please start a new conversation."
    );
}

use serde_json::json;

// ── End-to-end repair over a real HTTP upstream ──────────────────────

/// A mock llama-server that answers the two halves of a repair turn
/// differently, and records what it was asked.
///
/// Dispatching on `tool_choice` is the point: it asserts the repair
/// request really arrived carrying `"required"`, which is the property
/// stage-6 suppression exists to protect and the one a unit test over
/// bytes cannot observe.
async fn spawn_repair_mock(
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
                let is_repair = parsed.get("tool_choice") == Some(&json!("required"));
                seen.lock().unwrap().push(parsed);

                if is_repair {
                    // Non-streaming, schema-conformant: max_lines an integer.
                    axum::response::Response::builder()
                        .header("content-type", "application/json")
                        .body(axum::body::Body::from(
                            serde_json::to_vec(&json!({
                                "choices": [{"message": {"role": "assistant", "tool_calls": [{
                                    "id": "call_fixed",
                                    "type": "function",
                                    "function": {
                                        "name": "read_file",
                                        "arguments": "{\"path\":\"a\",\"max_lines\":42}"
                                    }
                                }]}}]
                            }))
                            .unwrap(),
                        ))
                        .unwrap()
                } else {
                    // Streaming, and wrong the way Llama 3.2 was measured
                    // wrong: max_lines carrying a string.
                    let sse = concat!(
                        "data: {\"choices\":[{\"index\":0,\"delta\":{\"tool_calls\":[{\"index\":0,\"id\":\"call_bad\",\"type\":\"function\",\"function\":{\"name\":\"read_file\",\"arguments\":\"{\\\"path\\\":\\\"a\\\",\"}}]},\"finish_reason\":null}]}\n\n",
                        "data: {\"choices\":[{\"index\":0,\"delta\":{\"tool_calls\":[{\"index\":0,\"function\":{\"arguments\":\"\\\"max_lines\\\":\\\"42\\\"}\"}}]},\"finish_reason\":null}]}\n\n",
                        "data: {\"choices\":[{\"index\":0,\"delta\":{},\"finish_reason\":\"tool_calls\"}]}\n\n",
                        "data: [DONE]\n\n",
                    );
                    axum::response::Response::builder()
                        .header("content-type", "text/event-stream")
                        .body(axum::body::Body::from(sse))
                        .unwrap()
                }
            }
        }),
    );

    let handle = tokio::spawn(async move {
        let _ = axum::serve(listener, app).await;
    });
    (port, handle)
}

fn repair_request_body() -> Bytes {
    Bytes::from(
        serde_json::to_vec(&json!({
            "model": "m",
            "stream": true,
            "tool_choice": "auto",
            "messages": [{"role": "user", "content": "read it"}],
            "tools": [{
                "type": "function",
                "function": {
                    "name": "read_file",
                    "parameters": {
                        "type": "object",
                        "properties": {
                            "path": {"type": "string"},
                            "max_lines": {"type": "integer"}
                        },
                        "required": ["path"]
                    }
                }
            }]
        }))
        .unwrap(),
    )
}

/// The whole loop over HTTP: a streamed non-conformant tool call is
/// withheld, re-issued with `tool_choice: "required"`, and the client
/// receives only the repaired call — in the right order, with `[DONE]`
/// last.
#[tokio::test]
async fn a_bad_streamed_tool_call_is_repaired_before_the_client_sees_it() {
    let seen = Arc::new(std::sync::Mutex::new(Vec::new()));
    let (port, server) = spawn_repair_mock(Arc::clone(&seen)).await;
    let url = format!("http://127.0.0.1:{port}/v1/chat/completions");
    let client = Client::new();
    let body = repair_request_body();

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
        None,
        tx,
        &connection,
        Some(RepairContext {
            req_builder: client.post(&url),
            request_body: body,
            enabled: true,
        }),
    )
    .await;

    let mut wire = String::new();
    while let Ok(Some(Ok(chunk))) =
        tokio::time::timeout(std::time::Duration::from_secs(5), rx.recv()).await
    {
        wire.push_str(&String::from_utf8_lossy(&chunk));
    }
    server.abort();

    assert!(outcome.repair_attempted, "repair should have fired");
    assert!(
        outcome.repair_succeeded,
        "repair should have produced a conformant call"
    );

    // The upstream saw exactly two requests, the second demanding a call.
    let requests = seen.lock().unwrap().clone();
    assert_eq!(requests.len(), 2, "one original, one repair");
    assert_eq!(requests[1]["tool_choice"], "required");
    assert_eq!(
        requests[1]["stream"], false,
        "the re-issue must not stream — see repair::repair_body"
    );

    // The client saw the fixed arguments and never the broken ones.
    assert!(
        wire.contains(r#"\"max_lines\":42"#) || wire.contains("max_lines\\\":42"),
        "repaired arguments should reach the client: {wire}"
    );
    assert!(
        !wire.contains(r#"\"42\""#),
        "the string-typed max_lines must never reach the client: {wire}"
    );

    // Ordering: the tool call precedes finish_reason, and [DONE] is last.
    let call_at = wire.find("call_fixed").expect("repaired call on the wire");
    // Matched on the *terminating* finish_reason, not the substring
    // "tool_calls" — that also occurs inside the delta frame carrying the
    // call itself, which made an earlier version of this assertion
    // compare a position against itself.
    let finish_at = wire
        .find(r#""finish_reason":"tool_calls""#)
        .expect("terminating finish_reason on the wire");
    assert!(
        call_at < finish_at,
        "tool call must be flushed before finish_reason: {wire}"
    );
    assert!(
        wire.trim_end().ends_with("data: [DONE]"),
        "[DONE] must be the final frame: {wire}"
    );
}

/// The bound exists so a hung upstream cannot turn a repair into an
/// unbounded silence for the client.
///
/// Asserted as a relationship rather than a literal: what matters is that
/// a re-issue is capped *well inside* the first-byte deadline, since a
/// constrained non-streaming call has no business taking as long as a
/// fresh full turn, and that the client hears something long before the
/// cap is reached.
#[test]
fn a_reissue_is_bounded_and_kept_warm_well_inside_the_first_byte_deadline() {
    assert!(
        REPAIR_REISSUE_TIMEOUT < std::time::Duration::from_secs(FIRST_BYTE_DEADLINE_SECS),
        "a constrained re-issue must not be allowed to outlast a full turn's deadline"
    );
    assert!(
        REPAIR_KEEPALIVE_INTERVAL * 2 < REPAIR_REISSUE_TIMEOUT,
        "the client must hear from us several times before the re-issue gives up"
    );
}

/// A mock that emits one complete, schema-conformant tool call and then
/// dies mid-stream with an inline error frame — llama.cpp's own tool-call
/// grammar rejecting the model's output, which is terminal and yields no
/// `Done`.
async fn spawn_dying_tool_call_mock() -> (u16, tokio::task::JoinHandle<()>) {
    use axum::routing::post;

    let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .expect("bind");
    let port = listener.local_addr().unwrap().port();

    let app = axum::Router::new().route(
        "/v1/chat/completions",
        post(|| async {
            let sse = concat!(
                "data: {\"choices\":[{\"index\":0,\"delta\":{\"tool_calls\":[{\"index\":0,\"id\":\"call_held\",\"type\":\"function\",\"function\":{\"name\":\"read_file\",\"arguments\":\"{\\\"path\\\":\\\"a\\\"}\"}}]},\"finish_reason\":null}]}\n\n",
                "data: {\"error\":{\"message\":\"upstream died mid-generation\"}}\n\n",
            );
            axum::response::Response::builder()
                .header("content-type", "text/event-stream")
                .body(axum::body::Body::from(sse))
                .unwrap()
        }),
    );

    let handle = tokio::spawn(async move {
        let _ = axum::serve(listener, app).await;
    });
    (port, handle)
}

/// The hold-back may delay a tool call or replace it, but it must never
/// delete one.
///
/// An inline error frame is terminal: the decoder marks the turn done, so
/// the `Done` arm — the only place that used to flush — never runs. Every
/// frame the hold-back was sitting on went out with it, and because
/// withholding engages whenever `repair.is_some()` (even with repair
/// *disabled*), a turn that repair would have left completely alone lost
/// content it would otherwise have shown.
#[tokio::test]
async fn a_stream_that_dies_mid_turn_still_releases_its_held_tool_call() {
    let (port, server) = spawn_dying_tool_call_mock().await;
    let url = format!("http://127.0.0.1:{port}/v1/chat/completions");
    let client = Client::new();
    let body = repair_request_body();

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
        None,
        tx,
        &connection,
        Some(RepairContext {
            req_builder: client.post(&url),
            request_body: body,
            enabled: true,
        }),
    )
    .await;

    let mut wire = String::new();
    while let Ok(Some(Ok(chunk))) =
        tokio::time::timeout(std::time::Duration::from_secs(5), rx.recv()).await
    {
        wire.push_str(&String::from_utf8_lossy(&chunk));
    }
    server.abort();

    assert!(
        outcome.upstream_errored,
        "an inline error frame is an upstream death"
    );
    let call_at = wire
        .find("call_held")
        .unwrap_or_else(|| panic!("the held tool call must reach the client: {wire}"));

    // Ordering is the other half of the fix: a client that sees the error
    // frame first may consider the turn over and never read the call.
    let error_at = wire
        .find("upstream died mid-generation")
        .expect("the error frame still reaches the client");
    assert!(
        call_at < error_at,
        "the held call must precede the error frame: {wire}"
    );
}

/// A conformant streamed call is passed through unchanged and costs no
/// second request — the hold-back must be invisible on the happy path.
#[tokio::test]
async fn a_conformant_streamed_tool_call_is_not_reissued() {
    let seen = Arc::new(std::sync::Mutex::new(Vec::new()));
    let (port, server) = spawn_repair_mock(Arc::clone(&seen)).await;
    let url = format!("http://127.0.0.1:{port}/v1/chat/completions");
    let client = Client::new();

    // A schema with no integer field: the streamed call conforms.
    let body = Bytes::from(
        serde_json::to_vec(&json!({
            "model": "m", "stream": true, "tool_choice": "auto",
            "messages": [{"role": "user", "content": "read it"}],
            "tools": [{"type": "function", "function": {
                "name": "read_file",
                "parameters": {"type": "object",
                    "properties": {"path": {"type": "string"}, "max_lines": {}},
                    "required": ["path"]}}}]
        }))
        .unwrap(),
    );

    let resp = client.post(&url).body(body.clone()).send().await.unwrap();
    let registry = Arc::new(crate::connections::ActiveConnectionsRegistry::new());
    let connection = registry.register("m", true, None);
    let (tx, mut rx) = tokio::sync::mpsc::channel(64);

    let outcome = stream_response_to_channel(
        resp,
        "m".to_owned(),
        None,
        tx,
        &connection,
        Some(RepairContext {
            req_builder: client.post(&url),
            request_body: body,
            enabled: true,
        }),
    )
    .await;

    let mut wire = String::new();
    while let Ok(Some(Ok(chunk))) =
        tokio::time::timeout(std::time::Duration::from_secs(5), rx.recv()).await
    {
        wire.push_str(&String::from_utf8_lossy(&chunk));
    }
    server.abort();

    assert!(!outcome.repair_attempted);
    assert_eq!(seen.lock().unwrap().len(), 1, "no second request");
    assert!(wire.contains("call_bad"), "original call forwarded: {wire}");
    assert!(wire.trim_end().ends_with("data: [DONE]"));
}
