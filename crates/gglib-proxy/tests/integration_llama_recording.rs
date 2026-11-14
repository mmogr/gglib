//! A streamed tool call recorded from the pinned llama-server (b12110), sent
//! through the real proxy.
//!
//! The recording is llama-server's reply to one `get_weather` request: the
//! role frame, 51 reasoning frames, a call in 10 frames, the finish
//! (`tool_calls`), the usage frame with `timings`, and `[DONE]`. Every
//! frame's `model`, which named a GGUF path on the recording machine, reads
//! `/models/Qwen3.8-27B-UD-Q8_K_XL.gguf`. Frames are compared as parsed JSON,
//! since a frame the proxy edits is written again with its keys in order.

use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

use async_trait::async_trait;
use axum::{Router, body::Body, http::Response, routing::post};
use futures_util::StreamExt as _;
use gglib_core::domain::ModelCapabilities;
use gglib_core::ports::{CatalogError, ModelCatalogPort, ModelLaunchSpec, ModelSummary};
use serde_json::{Value, json};
use tokio::net::TcpListener;
use tokio_util::sync::CancellationToken;

mod fixtures;
use fixtures::common::{FixedUpstream, parse_sse_frames, spawn_proxy_with_catalog};

/// llama-server's bytes, as recorded.
const RECORDING: &[u8] = include_bytes!("fixtures/llama_b12110_tool_call.sse");

/// The name gglib serves the model as.
const MODEL: &str = "Qwen3.8-27B";

/// A catalog of the one model, which calls tools, so the request keeps its
/// tools and repair can judge the call.
#[derive(Debug)]
struct ToolModel;

impl ToolModel {
    fn summary() -> ModelSummary {
        ModelSummary {
            capabilities: ModelCapabilities::SUPPORTS_TOOL_CALLS,
            ..ModelSummary::bare(1, MODEL)
        }
    }
}

#[async_trait]
impl ModelCatalogPort for ToolModel {
    async fn list_models(&self) -> Result<Vec<ModelSummary>, CatalogError> {
        Ok(vec![Self::summary()])
    }

    async fn resolve_model(&self, name: &str) -> Result<Option<ModelSummary>, CatalogError> {
        Ok((name == MODEL).then(Self::summary))
    }

    async fn resolve_for_launch(
        &self,
        _name: &str,
    ) -> Result<Option<ModelLaunchSpec>, CatalogError> {
        Ok(None)
    }
}

/// The recording's JSON frames, in order, `[DONE]` left out.
fn upstream_frames() -> Vec<Value> {
    let (frames, done) = parse_sse_frames(std::str::from_utf8(RECORDING).expect("utf-8"));
    assert!(done, "the recording ends in [DONE]");
    assert_eq!(frames.len(), 64, "the recording's frames");
    frames
}

/// `frame` as the client should get it: named as the served model.
fn served(mut frame: Value) -> Value {
    frame["model"] = json!(MODEL);
    frame
}

/// Whether `frame` carries a tool call.
fn carries_a_call(frame: &Value) -> bool {
    frame
        .pointer("/choices/0/delta/tool_calls")
        .is_some_and(|calls| calls.as_array().is_some_and(|c| !c.is_empty()))
}

/// The arguments of the call at index 0, joined across `frames`.
fn arguments(frames: &[Value]) -> String {
    frames
        .iter()
        .filter_map(|f| {
            f.pointer("/choices/0/delta/tool_calls/0/function/arguments")?
                .as_str()
        })
        .collect()
}

/// The recorded request, with `unit` allowed to be only `units`.
fn request(units: &[&str]) -> Value {
    json!({
        "model": MODEL,
        "stream": true,
        "stream_options": {"include_usage": true},
        "messages": [
            {"role": "system", "content": "You are a helpful assistant. Use the tools when they help."},
            {"role": "user", "content": "What's the weather in Melbourne right now? Use the tool."}
        ],
        "tools": [{"type": "function", "function": {
            "name": "get_weather",
            "description": "Current weather for a city.",
            "parameters": {"type": "object", "properties": {
                "city": {"type": "string", "description": "City name"},
                "unit": {"type": "string", "enum": units}
            }, "required": ["city"]}
        }}],
        "tool_choice": "auto",
    })
}

/// An upstream that answers the first request with the recording and any
/// later one, a repair's re-issue, with `second`. Returns its port and how
/// many requests it had.
async fn spawn_upstream(second: Value, cancel: CancellationToken) -> (u16, Arc<AtomicUsize>) {
    let asked = Arc::new(AtomicUsize::new(0));
    let count = Arc::clone(&asked);
    let app = Router::new().route(
        "/v1/chat/completions",
        post(move || {
            let n = count.fetch_add(1, Ordering::SeqCst);
            let second = second.to_string();
            async move {
                let (kind, body) = if n == 0 {
                    ("text/event-stream", Body::from(RECORDING))
                } else {
                    ("application/json", Body::from(second))
                };
                Response::builder()
                    .header("content-type", kind)
                    .body(body)
                    .unwrap()
            }
        }),
    );
    let listener = TcpListener::bind("127.0.0.1:0").await.expect("bind");
    let port = listener.local_addr().unwrap().port();
    tokio::spawn(async move {
        axum::serve(listener, app)
            .with_graceful_shutdown(cancel.cancelled_owned())
            .await
            .ok();
    });
    (port, asked)
}

/// One streamed turn of `request` through the proxy: every byte the client
/// was sent, and how many requests reached the upstream.
async fn turn(request: Value, second: Value) -> (String, usize) {
    let cancel = CancellationToken::new();
    let (port, asked) = spawn_upstream(second, cancel.clone()).await;
    let runtime = FixedUpstream {
        port,
        model_name: MODEL.into(),
        slot_restore_supported: true,
        pinned: false,
    };
    let (proxy_url, proxy_cancel) =
        spawn_proxy_with_catalog(Arc::new(runtime), Arc::new(ToolModel)).await;
    let response = reqwest::Client::new()
        .post(format!("{proxy_url}/v1/chat/completions"))
        .json(&request)
        .send()
        .await
        .expect("proxy request");
    assert_eq!(response.status(), 200);
    let mut wire = Vec::new();
    let mut chunks = response.bytes_stream();
    while let Some(chunk) = chunks.next().await {
        wire.extend_from_slice(&chunk.expect("body chunk"));
    }
    proxy_cancel.cancel();
    cancel.cancel();
    (
        String::from_utf8(wire).expect("utf-8"),
        asked.load(Ordering::SeqCst),
    )
}

/// Every recorded frame reaches the client, in order, with only `model`
/// changed: the role frame, the reasoning, the call (held while repair
/// judged it, then released as it came), the `tool_calls` finish and the
/// usage frame with its `timings`. One `[DONE]` ends it.
#[tokio::test]
async fn the_recorded_tool_call_stream_is_forwarded_frame_for_frame() {
    let (wire, asked) = turn(request(&["celsius", "fahrenheit"]), Value::Null).await;

    assert_eq!(asked, 1, "a call that validates is not asked for again");
    let (frames, _) = parse_sse_frames(&wire);
    let want: Vec<Value> = upstream_frames().into_iter().map(served).collect();
    assert_eq!(frames, want);
    assert_eq!(wire.matches("data: [DONE]").count(), 1, "{wire}");
    assert!(wire.ends_with("data: [DONE]\n\n"));

    let call = &frames.iter().find(|f| carries_a_call(f)).expect("a call")["choices"][0]["delta"]["tool_calls"]
        [0];
    assert_eq!(call["function"]["name"], json!("get_weather"));
    assert_eq!(
        arguments(&frames),
        r#"{"city":"Melbourne","unit":"celsius"}"#
    );
    let finish = &frames[frames.len() - 2];
    assert_eq!(finish["choices"][0]["finish_reason"], json!("tool_calls"));
    let usage = frames.last().expect("the usage frame");
    assert_eq!(usage["usage"]["completion_tokens"], json!(94));
    assert_eq!(usage["timings"]["draft_n_accepted"], json!(64));
}

/// A call that breaks the schema (`celsius` is not allowed here) is held,
/// judged at the finish and asked for again. The client gets every other
/// frame as recorded, and the repair's call in place of the recorded one,
/// ahead of the finish.
#[tokio::test]
async fn a_recorded_call_a_repair_replaces_goes_out_in_its_place() {
    let fixed = json!({"choices": [{"index": 0, "finish_reason": "tool_calls", "message": {
        "role": "assistant",
        "tool_calls": [{"id": "call_fixed", "type": "function", "function": {
            "name": "get_weather",
            "arguments": r#"{"city":"Melbourne","unit":"fahrenheit"}"#
        }}]
    }}]});
    let (wire, asked) = turn(request(&["fahrenheit"]), fixed).await;

    assert_eq!(asked, 2, "the call was asked for again");
    let (frames, _) = parse_sse_frames(&wire);
    let upstream = upstream_frames();
    let (calls, rest): (Vec<Value>, Vec<Value>) = frames.into_iter().partition(carries_a_call);
    let want: Vec<Value> = upstream
        .iter()
        .filter(|f| !carries_a_call(f))
        .cloned()
        .map(served)
        .collect();
    assert_eq!(rest, want, "every other frame as recorded");
    assert_eq!(
        arguments(&calls),
        r#"{"city":"Melbourne","unit":"fahrenheit"}"#
    );
    let call = &calls[0];
    assert_eq!(
        call["choices"][0]["delta"]["tool_calls"][0]["id"],
        json!("call_fixed")
    );
    assert_eq!(call["id"], upstream[0]["id"], "the reply's own id");
    assert_eq!(call["model"], json!(MODEL));
    assert!(
        calls.iter().all(|f| !f.to_string().contains("elsius")),
        "the recorded call never went out: {calls:?}"
    );
    let (_, after) = wire.split_once("call_fixed").expect("the repair's call");
    assert!(
        after.contains(r#""finish_reason":"tool_calls""#),
        "the finish comes after the call"
    );
    assert_eq!(wire.matches("data: [DONE]").count(), 1);
}
