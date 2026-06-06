//! Request forwarding to llama-server with parse → normalize → re-encode
//! pipeline for streaming responses.
//!
//! ## Request pipeline
//!
//! Before the upstream call the proxy applies two stateless transforms to the
//! request body, in order:
//!
//! 1. [`strip_prior_reasoning`] — scrubs `<think>` / `reasoning_content`
//!    artefacts from prior assistant turns so reasoning models don't
//!    pattern-match their own past traces.
//! 2. [`coalesce_for_capabilities`] — when a model's stored
//!    [`ModelCapabilities`] includes [`REQUIRES_STRICT_TURNS`], merges
//!    consecutive same-role user/assistant messages before they reach the
//!    Jinja template.  Mistral-family models raise a hard 500 exception
//!    without this.
//!
//! Capabilities are resolved with a **single** catalog lookup per request
//! (via [`resolve_model_context`]) that yields both the `ModelCapabilities`
//! bitfield (used for request preprocessing) and the `format:*` tags (used
//! for response-stream parser selection).  No second lookup is made.
//!
//! ## Response pipeline
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
//! `NormalizationError` events surfaced by the parsers are logged via
//! `tracing::warn` and never forwarded to the wire.
//!
//! Non-streaming responses are forwarded verbatim for now — the dialects
//! we currently rewrite (Qwen XML tool calls, bare `<think>` tags) only
//! manifest in streaming clients today.
//!
//! [`REQUIRES_STRICT_TURNS`]: gglib_core::ModelCapabilities

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
use gglib_core::ModelCapabilities;
use gglib_core::domain::{ChatMessage, transform_messages_for_capabilities};
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

/// Resolved per-request model context: capabilities for request preprocessing
/// and tags for response-stream parser selection.
///
/// Both values come from a **single** catalog lookup, eliminating the
/// previous split-brain where `resolve_tags` and capability lookups were
/// separate concerns served by different code paths.
struct ModelContext {
    /// Stored capability bitfield — drives request-side transforms.
    capabilities: ModelCapabilities,
    /// `format:*` tags — drives response-stream parser selection.
    tags: Vec<String>,
}

/// Resolve the [`ModelContext`] for a model in a single catalog round-trip.
///
/// On any failure (catalog unavailable, model unknown) returns a zeroed
/// context: empty capabilities → all transforms are no-ops, empty tags →
/// identity-passthrough parser.  This is the safe, conservative fallback.
async fn resolve_model_context(catalog: &dyn ModelCatalogPort, model_name: &str) -> ModelContext {
    match catalog.resolve_model(model_name).await {
        Ok(Some(summary)) => ModelContext {
            capabilities: summary.capabilities,
            tags: summary.tags,
        },
        Ok(None) => {
            debug!(model = %model_name, "model not found in catalog; using pass-through context");
            ModelContext {
                capabilities: ModelCapabilities::empty(),
                tags: Vec::new(),
            }
        }
        Err(e) => {
            warn!(model = %model_name, error = %e, "failed to resolve model context; using pass-through context");
            ModelContext {
                capabilities: ModelCapabilities::empty(),
                tags: Vec::new(),
            }
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

/// Coalesce consecutive same-role messages when the model requires strict
/// turn alternation.
///
/// Mistral-family models (and any architecture with
/// [`ModelCapabilities::REQUIRES_STRICT_TURNS`]) enforce user/assistant
/// alternation inside their Jinja chat templates and raise a hard 500
/// exception when consecutive same-role messages are present.  IDEs and
/// gateway extensions (e.g. the VSCode LLM Gateway) routinely send
/// multi-turn context that violates this — coalescing here is the correct
/// fix rather than constraining callers.
///
/// Uses [`transform_messages_for_capabilities`] from `gglib-core` as the
/// single source of truth for the merging rules, exactly as the internal
/// `chat_api` path does.  When capabilities are empty (unknown model) this
/// function is a zero-cost no-op.
///
/// On parse failure the original bytes are returned unchanged.
fn coalesce_for_capabilities(body: Bytes, capabilities: ModelCapabilities) -> Bytes {
    // Fast path: no preprocessing needed for this model.
    if !capabilities.requires_strict_turns() && capabilities.supports_system_role() {
        return body;
    }
    // Also fast path when capabilities are completely unknown.
    if capabilities.is_empty() {
        return body;
    }

    debug!(
        requires_strict_turns = capabilities.requires_strict_turns(),
        supports_system_role = capabilities.supports_system_role(),
        "coalesce: entering message transformation"
    );

    let Ok(mut value) = serde_json::from_slice::<serde_json::Value>(&body) else {
        warn!("coalesce: failed to parse request body as JSON; forwarding original");
        return body;
    };

    let Some(messages_raw) = value.get("messages").and_then(|v| v.as_array()) else {
        debug!("coalesce: no messages array found in request body");
        return body;
    };

    let before_count = messages_raw.len();

    // Deserialise only the fields `transform_messages_for_capabilities` needs.
    // `ChatMessage.content` accepts both a plain JSON string and a JSON array of
    // content-part objects (e.g. VSCode LLM Gateway sends array-form content per
    // the OpenAI spec).  Using `MessageContent` ensures we can always round-trip.
    let messages: Vec<ChatMessage> =
        match serde_json::from_value(serde_json::Value::Array(messages_raw.clone())) {
            Ok(m) => m,
            Err(e) => {
                warn!(
                    error = %e,
                    before = before_count,
                    "coalesce: failed to deserialise messages as Vec<ChatMessage>; \
                     forwarding original body unchanged. \
                     This usually means a message field has an unexpected type."
                );
                return body;
            }
        };

    debug!(
        before = before_count,
        roles = ?messages.iter().map(|m| m.role.as_str()).collect::<Vec<_>>(),
        "coalesce: parsed messages for transformation"
    );

    let transformed = transform_messages_for_capabilities(messages, capabilities);
    let after_count = transformed.len();

    debug!(
        before = before_count,
        after = after_count,
        merged = before_count.saturating_sub(after_count),
        "coalesce: transformation complete"
    );

    match serde_json::to_value(&transformed) {
        Ok(new_messages) => {
            value["messages"] = new_messages;
            match serde_json::to_vec(&value) {
                Ok(v) => Bytes::from(v),
                Err(e) => {
                    warn!(error = %e, "coalesce: failed to re-serialize; forwarding original");
                    body
                }
            }
        }
        Err(e) => {
            warn!(error = %e, "coalesce: failed to serialise transformed messages; forwarding original");
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
/// * `catalog` - Catalog port used to resolve capabilities and `format:*` tags
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

    // Single catalog lookup — yields both capabilities (request preprocessing)
    // and tags (response-stream parser).  Failures return a zero context so
    // all transforms become no-ops rather than blocking the request.
    let context = resolve_model_context(catalog.as_ref(), model_name).await;

    // ── Request transforms (applied in order) ──────────────────────────────
    //
    // 1. Strip reasoning artefacts from prior assistant turns so reasoning
    //    models don't pattern-match their own past <think> traces.
    let body = strip_prior_reasoning(body);

    // 2. Coalesce consecutive same-role messages for strict-turn models
    //    (e.g. Mistral/Devstral).  No-op when capabilities are empty/unknown.
    let body = coalesce_for_capabilities(body, context.capabilities);

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
        // Tags resolved above — no second catalog lookup needed.
        forward_streaming_response(response, model_name.to_owned(), context.tags).await
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
                            "type": "server_error",
                            "code": "upstream_error",
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
        // When no reasoning content is present the body should pass through
        // byte-for-byte unchanged.
        let body = Bytes::from(r#"{"model":"m","messages":[{"role":"user","content":"hi"}]}"#);
        let out = strip_prior_reasoning(body.clone());
        assert_eq!(out, body);
    }
}
