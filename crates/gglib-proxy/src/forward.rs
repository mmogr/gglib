//! Request forwarding to llama-server with parse → normalize → re-encode
//! pipeline for streaming responses.
//!
//! For streaming requests this module owns the universal-consistency moat:
//!
//! ```text
//!  upstream bytes
//!        │
//!        ▼
//!  SseStreamDecoder          (→ typed LlmStreamEvent)
//!        │
//!        ▼
//!  NormalizingStream         (Qwen XML → ToolCallDelta, <think> → ReasoningDelta)
//!        │
//!        ▼
//!  SseEncoder                (→ pristine OpenAI `data:` frames)
//!        │
//!        ▼
//!  client
//! ```
//!
//! Tags consulted by `get_parser` are looked up via [`resolve_tags`] from
//! the [`ModelCatalogPort`] before the upstream call begins.  An empty tag
//! list selects the identity-passthrough parser, so models that already
//! emit strict OpenAI events are unaffected by the wrap.
//!
//! `NormalizationError` events surfaced by the parsers are logged via
//! `tracing::warn` and never forwarded to the wire.
//!
//! Non-streaming responses are forwarded verbatim for now — the dialects
//! we currently rewrite (Qwen XML tool calls, bare `<think>` tags) only
//! manifest in streaming clients today.

use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};

use axum::{
    body::Body,
    http::{HeaderMap, HeaderValue, StatusCode},
    response::{IntoResponse, Response},
};
use bytes::Bytes;
use futures_util::StreamExt as _;
use reqwest::Client;
use tracing::{debug, error, warn};

use gglib_core::LlmStreamEvent;
use gglib_core::normalize::{NormalizingStream, get_parser};
use gglib_core::ports::ModelCatalogPort;
use gglib_core::sse::{SseEncoder, SseStreamDecoder};

use crate::models::ErrorResponse;

/// Headers that should NOT be forwarded (hop-by-hop headers).
const HOP_BY_HOP_HEADERS: &[&str] = &[
    "connection",
    "keep-alive",
    "proxy-authenticate",
    "proxy-authorization",
    "te",
    "trailers",
    "transfer-encoding",
    "upgrade",
    // Also strip these for security/correctness
    "host",
    "content-length",
    "authorization", // Don't forward auth to llama-server
];

/// Check if a header should be forwarded.
fn should_forward_header(name: &str) -> bool {
    let lower = name.to_lowercase();
    !HOP_BY_HOP_HEADERS.contains(&lower.as_str())
}

/// Look up the `format:*` tags for a model so the proxy can pick a
/// dialect-specific parser via [`gglib_core::normalize::get_parser`].
///
/// Returns an empty `Vec` when the catalog cannot resolve the model or the
/// model has no tags — both cases select the identity-passthrough parser,
/// which is the right fallback for any model that already speaks strict
/// `OpenAI` tool-calling.
pub async fn resolve_tags(catalog: &dyn ModelCatalogPort, model_name: &str) -> Vec<String> {
    match catalog.resolve_model(model_name).await {
        Ok(Some(summary)) => summary.tags,
        Ok(None) => Vec::new(),
        Err(e) => {
            warn!(model = %model_name, error = %e, "failed to resolve model tags; using identity parser");
            Vec::new()
        }
    }
}

/// Strip prior reasoning artifacts from assistant messages in the request body.
///
/// Mirrors OpenAI's reference behavior: `reasoning_content` and inline
/// `<think>...</think>` blocks from previous turns are dropped before the
/// request reaches the upstream model. This prevents small reasoning models
/// from pattern-matching their own past `<think>` traces in the chat history
/// and looping endlessly inside an unclosed thought block on subsequent
/// turns.
///
/// The transform is unconditional and defensive:
///
/// * Any parse failure returns the original bytes unchanged (zero blast
///   radius for non-JSON or unexpected request shapes).
/// * Only `messages[*]` entries with `role == "assistant"` are touched.
/// * `reasoning_content` is removed outright when present.
/// * String `content` has every `<think>...</think>` block (including the
///   delimiters) excised; non-string content is left alone.
/// * User, system, tool, and developer messages pass through untouched.
fn strip_prior_reasoning(body: Bytes) -> Bytes {
    let Ok(mut value) = serde_json::from_slice::<serde_json::Value>(&body) else {
        return body;
    };

    let Some(messages) = value.get_mut("messages").and_then(|v| v.as_array_mut()) else {
        return body;
    };

    let mut touched = 0usize;
    for msg in messages.iter_mut() {
        let Some(obj) = msg.as_object_mut() else {
            continue;
        };
        let is_assistant = obj
            .get("role")
            .and_then(|r| r.as_str())
            .map(|r| r == "assistant")
            .unwrap_or(false);
        if !is_assistant {
            continue;
        }

        let mut changed = false;
        if obj.remove("reasoning_content").is_some() {
            changed = true;
        }

        if let Some(serde_json::Value::String(s)) = obj.get_mut("content")
            && let Some(stripped) = strip_think_blocks(s)
        {
            *s = stripped;
            changed = true;
        }

        if changed {
            touched += 1;
        }
    }

    if touched == 0 {
        return body;
    }

    match serde_json::to_vec(&value) {
        Ok(v) => {
            debug!(touched, "stripped prior reasoning from assistant messages");
            Bytes::from(v)
        }
        Err(e) => {
            warn!(error = %e, "failed to re-serialize request after reasoning strip; forwarding original");
            body
        }
    }
}

/// Remove every `<think>...</think>` block from `s`.
///
/// Returns `Some(new_string)` when at least one block was removed, otherwise
/// `None` so the caller can avoid a needless allocation. Matching is
/// case-sensitive and non-greedy: each `<think>` is paired with the next
/// `</think>` that follows it. An unclosed `<think>` is left intact (the
/// upstream model is responsible for closing it).
fn strip_think_blocks(s: &str) -> Option<String> {
    const OPEN: &str = "<think>";
    const CLOSE: &str = "</think>";

    if !s.contains(OPEN) {
        return None;
    }

    let mut out = String::with_capacity(s.len());
    let mut rest = s;
    let mut changed = false;
    while let Some(open_idx) = rest.find(OPEN) {
        let after_open = &rest[open_idx + OPEN.len()..];
        let Some(close_off) = after_open.find(CLOSE) else {
            // Unclosed <think>: keep verbatim, stop scanning.
            break;
        };
        out.push_str(&rest[..open_idx]);
        rest = &after_open[close_off + CLOSE.len()..];
        changed = true;
    }
    if !changed {
        return None;
    }
    out.push_str(rest);
    Some(out.trim().to_string())
}

/// Forward a chat completion request to the upstream llama-server.
///
/// # Arguments
///
/// * `client` - HTTP client to use for the request
/// * `upstream_url` - Full URL to the llama-server endpoint
/// * `headers` - Original request headers
/// * `body` - Request body bytes
/// * `is_streaming` - Whether this is a streaming request (affects response handling)
/// * `model_name` - Model name to advertise to the client (used in SSE envelope)
/// * `catalog` - Catalog port used to resolve `format:*` tags for the model
///
/// # Returns
///
/// The response from llama-server, with the streaming SSE body re-emitted
/// through the universal normalization pipeline when `is_streaming` is true.
pub async fn forward_chat_completion(
    client: &Client,
    upstream_url: &str,
    headers: &HeaderMap,
    body: Bytes,
    is_streaming: bool,
    model_name: &str,
    catalog: Arc<dyn ModelCatalogPort>,
) -> Response {
    debug!("Forwarding to {upstream_url}, streaming={is_streaming}");

    // Drop reasoning artifacts from prior assistant turns before the model
    // sees them. Mirrors OpenAI's native handling of reasoning tokens and
    // prevents small reasoning models from pattern-matching their own past
    // `<think>` traces into an unbounded thinking loop on follow-up turns.
    let body = strip_prior_reasoning(body);

    // Build the request to upstream
    let mut req_builder = client
        .post(upstream_url)
        .header("content-type", "application/json");

    // Forward allowed headers
    for (name, value) in headers.iter() {
        if should_forward_header(name.as_str())
            && let Ok(value_str) = value.to_str()
        {
            req_builder = req_builder.header(name.as_str(), value_str);
        }
    }

    // Send the request
    let response = match req_builder.body(body).send().await {
        Ok(resp) => resp,
        Err(e) => {
            error!("Failed to connect to llama-server: {e}");
            return (
                StatusCode::BAD_GATEWAY,
                axum::Json(ErrorResponse::upstream_error(&e.to_string())),
            )
                .into_response();
        }
    };

    let status = response.status();

    // For errors, return the error body directly
    if !status.is_success() {
        let error_bytes = response.bytes().await.unwrap_or_default();
        return Response::builder()
            .status(StatusCode::from_u16(status.as_u16()).unwrap_or(StatusCode::BAD_GATEWAY))
            .header("content-type", "application/json")
            .body(Body::from(error_bytes))
            .unwrap_or_else(|_| StatusCode::INTERNAL_SERVER_ERROR.into_response());
    }

    if is_streaming {
        let tags = resolve_tags(catalog.as_ref(), model_name).await;
        forward_streaming_response(response, model_name.to_owned(), tags).await
    } else {
        // Non-streaming: read full response. Dialect normalization for
        // non-streaming responses is intentionally deferred — the wire
        // formats we currently rewrite (Qwen XML tool calls, bare <think>
        // tags) only manifest in streaming clients today.
        forward_non_streaming_response(response).await
    }
}

/// Forward a streaming SSE response after running it through the universal
/// normalization pipeline (decode → normalize → re-encode).
async fn forward_streaming_response(
    response: reqwest::Response,
    model_name: String,
    tags: Vec<String>,
) -> Response {
    // Stable envelope metadata — same `id`/`created` for every chunk of
    // this response, matching the OpenAI streaming contract.
    let id = format!("chatcmpl-{}", uuid::Uuid::new_v4().simple());
    let created = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    let encoder = SseEncoder::new(id, model_name, created);

    // 1. Raw upstream bytes → typed LlmStreamEvent stream.
    let byte_stream = response.bytes_stream();
    let event_stream = async_stream::stream! {
        let mut decoder = SseStreamDecoder::default();
        let mut byte_stream = std::pin::pin!(byte_stream);

        'outer: while let Some(chunk_result) = byte_stream.next().await {
            let chunk = match chunk_result {
                Ok(c) => c,
                Err(e) => {
                    yield Err(anyhow::anyhow!("upstream SSE byte-stream error: {e}"));
                    return;
                }
            };

            let (events, stop) = decoder.feed_bytes(&chunk);
            for event in events {
                yield event;
            }
            if stop {
                break 'outer;
            }
        }

        if let Some(fallback) = decoder.finish() {
            yield Ok(fallback);
        }
    };

    // 2. Wrap with the universal normalization layer.
    let parser = get_parser(&tags);
    let normalized = NormalizingStream::new(Box::pin(event_stream), parser);

    // 3. Re-encode each typed event back into pristine OpenAI `data:` frames.
    //    NormalizationError events are logged but never reach the wire.
    let wire_stream = normalized.filter_map(move |event| {
        let encoder = encoder.clone();
        async move {
            match event {
                Ok(ev) => {
                    if let LlmStreamEvent::NormalizationError { kind, raw } = &ev {
                        warn!(?kind, raw = %raw, "proxy: suppressing normalization issue from wire");
                    }
                    encoder
                        .encode(&ev)
                        .map(|s| Ok::<Bytes, std::io::Error>(Bytes::from(s)))
                }
                Err(e) => {
                    error!("proxy stream error: {e}");
                    // Convert internal error into a structured SSE error frame
                    // so the client sees a terminal signal rather than a hang.
                    let payload = serde_json::json!({
                        "error": {
                            "message": e.to_string(),
                            "type": "upstream_error",
                        }
                    });
                    let frame = format!("data: {payload}\n\ndata: [DONE]\n\n");
                    Some(Ok::<Bytes, std::io::Error>(Bytes::from(frame)))
                }
            }
        }
    });

    let body = Body::from_stream(wire_stream);

    Response::builder()
        .status(StatusCode::OK)
        .header("content-type", "text/event-stream")
        .header("cache-control", "no-cache")
        .header("x-accel-buffering", "no") // Disable nginx buffering
        .body(body)
        .unwrap_or_else(|_| StatusCode::INTERNAL_SERVER_ERROR.into_response())
}

/// Forward a non-streaming JSON response from llama-server.
async fn forward_non_streaming_response(response: reqwest::Response) -> Response {
    // Collect upstream headers we want to preserve
    let content_type = response
        .headers()
        .get("content-type")
        .cloned()
        .unwrap_or_else(|| HeaderValue::from_static("application/json"));

    // Read the full body
    match response.bytes().await {
        Ok(body_bytes) => Response::builder()
            .status(StatusCode::OK)
            .header("content-type", content_type)
            .body(Body::from(body_bytes))
            .unwrap_or_else(|_| StatusCode::INTERNAL_SERVER_ERROR.into_response()),
        Err(e) => {
            error!("Failed to read upstream response: {e}");
            (
                StatusCode::BAD_GATEWAY,
                axum::Json(ErrorResponse::upstream_error(&e.to_string())),
            )
                .into_response()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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

    fn parse(b: &Bytes) -> serde_json::Value {
        serde_json::from_slice(b).expect("valid json")
    }

    #[test]
    fn strip_removes_reasoning_content_from_assistant_message() {
        let body = Bytes::from(
            r#"{"model":"m","messages":[
                {"role":"user","content":"hi"},
                {"role":"assistant","content":"hello","reasoning_content":"long ramble..."}
            ]}"#,
        );
        let out = parse(&strip_prior_reasoning(body));
        let msgs = out["messages"].as_array().unwrap();
        assert_eq!(msgs[1]["content"], "hello");
        assert!(msgs[1].get("reasoning_content").is_none());
        // User untouched.
        assert_eq!(msgs[0]["content"], "hi");
    }

    #[test]
    fn strip_removes_inline_think_blocks_from_assistant_content() {
        let body = Bytes::from(
            r#"{"model":"m","messages":[
                {"role":"assistant","content":"<think>secret\nplan</think>The answer is 42."}
            ]}"#,
        );
        let out = parse(&strip_prior_reasoning(body));
        assert_eq!(out["messages"][0]["content"], "The answer is 42.");
    }

    #[test]
    fn strip_handles_multiple_think_blocks() {
        let body = Bytes::from(
            r#"{"model":"m","messages":[
                {"role":"assistant","content":"<think>a</think>between<think>b</think>after"}
            ]}"#,
        );
        let out = parse(&strip_prior_reasoning(body));
        assert_eq!(out["messages"][0]["content"], "betweenafter");
    }

    #[test]
    fn strip_leaves_unclosed_think_intact() {
        let body = Bytes::from(
            r#"{"model":"m","messages":[
                {"role":"assistant","content":"<think>still going..."}
            ]}"#,
        );
        let out = parse(&strip_prior_reasoning(body));
        assert_eq!(
            out["messages"][0]["content"],
            "<think>still going..."
        );
    }

    #[test]
    fn strip_does_not_touch_user_or_system_or_tool_messages() {
        let body = Bytes::from(
            r#"{"model":"m","messages":[
                {"role":"system","content":"<think>policy</think>be helpful","reasoning_content":"x"},
                {"role":"user","content":"<think>ignore</think>question","reasoning_content":"y"},
                {"role":"tool","content":"<think>tool</think>result","tool_call_id":"c1","reasoning_content":"z"}
            ]}"#,
        );
        let original_value = parse(&body);
        let out = parse(&strip_prior_reasoning(body));
        assert_eq!(out, original_value);
    }

    #[test]
    fn strip_returns_original_on_invalid_json() {
        let body = Bytes::from_static(b"not json");
        let out = strip_prior_reasoning(body.clone());
        assert_eq!(out, body);
    }

    #[test]
    fn strip_returns_original_when_messages_missing() {
        let body = Bytes::from(r#"{"model":"m"}"#);
        let out = strip_prior_reasoning(body.clone());
        assert_eq!(out, body);
    }

    #[test]
    fn strip_handles_empty_messages_array() {
        let body = Bytes::from(r#"{"model":"m","messages":[]}"#);
        let out = strip_prior_reasoning(body.clone());
        assert_eq!(out, body);
    }

    #[test]
    fn strip_skips_when_nothing_to_remove() {
        // Should return the original Bytes (no re-serialization).
        let body = Bytes::from(
            r#"{"model":"m","messages":[{"role":"assistant","content":"plain answer"}]}"#,
        );
        let out = strip_prior_reasoning(body.clone());
        assert_eq!(out, body);
    }

    #[test]
    fn strip_preserves_non_string_content() {
        // Array-form content (OpenAI multi-part) is left alone; only
        // reasoning_content gets removed.
        let body = Bytes::from(
            r#"{"model":"m","messages":[
                {"role":"assistant","content":[{"type":"text","text":"<think>x</think>hi"}],"reasoning_content":"r"}
            ]}"#,
        );
        let out = parse(&strip_prior_reasoning(body));
        assert!(out["messages"][0].get("reasoning_content").is_none());
        // content array untouched
        assert_eq!(
            out["messages"][0]["content"][0]["text"],
            "<think>x</think>hi"
        );
    }
}
