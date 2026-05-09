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
/// Thin adapter over [`gglib_core::normalize::history::strip_thinking_debt`]
/// — that module is the single source of truth for the scrub rules.  This
/// function exists only to handle the bytes ⇄ JSON conversion at the proxy
/// boundary:
///
/// * On parse failure, the original `Bytes` are returned unchanged (zero
///   blast radius for non-JSON or unexpected request shapes).
/// * When the shared scrubber reports zero changes, the original `Bytes`
///   are returned to avoid a needless re-serialization.
fn strip_prior_reasoning(body: Bytes) -> Bytes {
    let Ok(mut value) = serde_json::from_slice::<serde_json::Value>(&body) else {
        return body;
    };

    let Some(messages) = value.get_mut("messages").and_then(|v| v.as_array_mut()) else {
        return body;
    };

    let touched = gglib_core::normalize::strip_thinking_debt(messages);
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

    // The full scrub-rule matrix lives in `gglib_core::normalize::history`
    // tests.  These tests cover only the bytes ⇄ JSON adapter behaviour
    // that is unique to the proxy boundary.

    #[test]
    fn strip_delegates_and_reserializes_when_changes_made() {
        let body = Bytes::from(
            r#"{"model":"m","messages":[
                {"role":"assistant","content":"hello","reasoning_content":"long ramble..."}
            ]}"#,
        );
        let out = parse(&strip_prior_reasoning(body));
        assert!(out["messages"][0].get("reasoning_content").is_none());
        assert_eq!(out["messages"][0]["content"], "hello");
    }

    #[test]
    fn strip_returns_original_bytes_on_invalid_json() {
        let body = Bytes::from_static(b"not json");
        let out = strip_prior_reasoning(body.clone());
        assert_eq!(out, body);
    }

    #[test]
    fn strip_returns_original_bytes_when_messages_missing() {
        let body = Bytes::from(r#"{"model":"m"}"#);
        let out = strip_prior_reasoning(body.clone());
        assert_eq!(out, body);
    }

    #[test]
    fn strip_returns_original_bytes_when_no_changes_needed() {
        // Pointer/length identity: when nothing changes we must avoid
        // re-serializing.
        let body = Bytes::from(r#"{"model":"m","messages":[{"role":"user","content":"hi"}]}"#);
        let out = strip_prior_reasoning(body.clone());
        assert_eq!(out, body);
    }
}
