//! Request forwarding to llama-server with parse → normalize → re-encode
//! pipeline for streaming responses.
//!
//! ## Request pipeline
//!
//! Before the upstream call the proxy applies three stateless transforms to the
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
//! 3. [`truncate_history`] — defends against client-side context compaction
//!    failures.  Any unprotected `role: "tool"` or `role: "assistant"` message
//!    whose string `content` exceeds **2,000 characters** is replaced with a
//!    short placeholder.  If the total payload still exceeds **240,000
//!    characters** (≈ 60,000 tokens) after this pass, the request is rejected
//!    with HTTP 400 / `context_length_exceeded` rather than forwarding a
//!    prompt that would cause the model to fail.  The last four messages and
//!    all `role: "system"` messages are always preserved.
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
use tracing::{debug, error, info, warn};

use gglib_core::LlmStreamEvent;
use gglib_core::ModelCapabilities;
use gglib_core::domain::{ChatMessage, InferenceConfig, transform_messages_for_capabilities};
use gglib_core::normalize::{NormalizingStream, get_parser};
use gglib_core::ports::ModelCatalogPort;
use gglib_core::sse::{SseEncoder, SseStreamDecoder};

use crate::metrics::{ContextMetricsStore, ContextSnapshot};
use crate::models::ErrorResponse;
use crate::truncation::truncate_history;

/// Signals that the upstream llama-server was unreachable (connection refused
/// or timed out).  Returned by [`forward_chat_completion`] so the caller can
/// invalidate stale model state and surface a retriable 503 to the client
/// instead of a terminal 502.
#[derive(Debug)]
pub(crate) enum ForwardError {
    /// The upstream llama-server could not be reached (ECONNREFUSED or timeout).
    UpstreamDead,
}

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
    /// Per-model inference defaults to merge into each request.
    inference_defaults: Option<InferenceConfig>,
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
            inference_defaults: summary.inference_defaults,
        },
        Ok(None) => {
            debug!(model = %model_name, "model not found in catalog; using pass-through context");
            ModelContext {
                capabilities: ModelCapabilities::empty(),
                tags: Vec::new(),
                inference_defaults: None,
            }
        }
        Err(e) => {
            warn!(model = %model_name, error = %e, "failed to resolve model context; using pass-through context");
            ModelContext {
                capabilities: ModelCapabilities::empty(),
                tags: Vec::new(),
                inference_defaults: None,
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

    // Log size of non-message top-level fields to identify what's inflating the body.
    for (key, val) in value.as_object().into_iter().flatten() {
        if key != "messages" {
            let approx_bytes = serde_json::to_vec(val).map(|v| v.len()).unwrap_or(0);
            debug!(key, approx_bytes, "coalesce: top-level field size");
        }
    }

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
    for (i, m) in messages.iter().enumerate() {
        let content_bytes = m
            .content
            .as_ref()
            .map(|c| {
                c.as_str()
                    .map(|s| s.len())
                    .unwrap_or_else(|| format!("{:?}", c).len())
            })
            .unwrap_or(0);
        debug!(i, role = %m.role, content_bytes, "coalesce: message sizes");
    }

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
/// * `metrics` - Metrics store for recording per-request context snapshots
/// * `global_inference_defaults` - Global inference defaults from settings
///
/// # Returns
///
/// The response from llama-server, with the streaming SSE body re-emitted
/// through the universal normalization pipeline when `is_streaming` is true.
#[allow(clippy::too_many_arguments)]
pub(crate) async fn forward_chat_completion(
    client: &Client,
    upstream_url: &str,
    headers: &HeaderMap,
    body: Bytes,
    is_streaming: bool,
    model_name: &str,
    catalog: Arc<dyn ModelCatalogPort>,
    metrics: Arc<ContextMetricsStore>,
    global_inference_defaults: Option<InferenceConfig>,
) -> Result<Response, ForwardError> {
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

    // 3. Truncate stale tool/large-assistant history to prevent local model
    //    context-window overflow caused by broken client-side compaction.
    let body = match truncate_history(body) {
        Ok((b, report)) => {
            if report.messages_truncated > 0 {
                info!(
                    messages_truncated = report.messages_truncated,
                    payload_chars_before = report.payload_chars_before,
                    payload_chars_after = report.payload_chars_after,
                    "history truncated: reduced payload before upstream forwarding"
                );
            }
            metrics.record(ContextSnapshot {
                model_name: model_name.to_owned(),
                payload_chars_before: report.payload_chars_before,
                payload_chars_after: report.payload_chars_after,
                messages_truncated: report.messages_truncated,
                was_clamped: false,
                recorded_at_secs: std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap_or_default()
                    .as_secs(),
            });
            b
        }
        Err(response) => {
            // Hard abort: payload still exceeds budget after truncation.
            // Record a clamped snapshot before returning the error response.
            metrics.record(ContextSnapshot {
                model_name: model_name.to_owned(),
                payload_chars_before: 0,
                payload_chars_after: 0,
                messages_truncated: 0,
                was_clamped: true,
                recorded_at_secs: std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap_or_default()
                    .as_secs(),
            });
            return Ok(*response);
        }
    };

    debug!(
        body_bytes = body.len(),
        "sending request to upstream (post-transform)"
    );

    // 4. Inject resolved inference defaults.
    //
    // Extract any params the client already sent, then merge model-level and
    // global defaults behind them via the 4-level hierarchy.  The resolved
    // values are aggressively inserted (not `or_insert`) so that every
    // param in the final config is forwarded to llama-server.
    let body = if let Ok(mut body_value) = serde_json::from_slice::<serde_json::Value>(&body) {
        let client_params = InferenceConfig::from_openai_json(&body_value);
        let resolved = client_params.resolve_with_defaults(
            context.inference_defaults.as_ref(),
            global_inference_defaults.as_ref(),
        );
        if let Some(body_obj) = body_value.as_object_mut() {
            for (k, v) in resolved.to_openai_json_patch() {
                body_obj.insert(k, v);
            }
        }
        serde_json::to_vec(&body_value)
            .map(Bytes::from)
            .unwrap_or(body)
    } else {
        body
    };

    // Build the request builder with all forwarded headers.
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

    if is_streaming {
        // ── Streaming path: keepalive background task ─────────────────────
        //
        // llama.cpp queues requests internally when all N slots are busy and
        // does not send HTTP response headers until a slot is assigned.  For
        // large-context prompts this wait can exceed 6 minutes, causing the
        // VS Code LLM Gateway extension to abort with "This operation was
        // aborted" before the response begins.
        //
        // Strategy:
        // 1. Quick TCP probe — distinguishes a dead server (ECONNREFUSED)
        //    from a live-but-busy one (TCP ACCEPT succeeds).  Dead → return
        //    UpstreamDead so the caller triggers the transparent restart loop.
        // 2. Return 200 + text/event-stream headers immediately so the client
        //    considers the connection live.
        // 3. Background task races the real send() against a 15-second timer,
        //    emitting SSE comment frames (":" ) while waiting for llama.cpp
        //    to assign a slot.  Once headers arrive the task streams the real
        //    response through the normalization pipeline via the same channel.

        // Phase 1 — TCP probe (1 s timeout).
        let probe_addr = host_port_from_url(upstream_url);
        let probe_result = tokio::time::timeout(
            std::time::Duration::from_secs(1),
            tokio::net::TcpStream::connect(probe_addr.as_str()),
        )
        .await;
        let server_alive = match probe_result {
            Ok(Ok(_conn)) => true,
            Ok(Err(e)) => {
                error!(addr = %probe_addr, "upstream llama-server TCP probe failed: {e}");
                false
            }
            Err(_) => {
                warn!(addr = %probe_addr, "upstream llama-server TCP probe timed out");
                false
            }
        };
        if !server_alive {
            return Err(ForwardError::UpstreamDead);
        }

        // Phase 2 — channel-backed response + keepalive background task.
        let (tx, rx) = tokio::sync::mpsc::channel::<Result<Bytes, std::io::Error>>(32);
        let model_name_owned = model_name.to_owned();
        let tags = context.tags;

        tokio::spawn(async move {
            let mut keepalive_interval =
                tokio::time::interval(std::time::Duration::from_secs(15));
            keepalive_interval.tick().await; // skip first immediate tick

            // Race: llama.cpp response headers vs 15-second keepalive timer.
            let send_future = req_builder.body(body).send();
            tokio::pin!(send_future);

            let upstream_response = loop {
                tokio::select! {
                    biased;
                    result = &mut send_future => break result,
                    _ = keepalive_interval.tick() => {
                        debug!("slot-queue wait: sending SSE keepalive to client");
                        if tx.send(Ok(Bytes::from_static(b":\n\n"))).await.is_err() {
                            return; // client disconnected
                        }
                    }
                }
            };

            match upstream_response {
                Ok(resp) if resp.status().is_success() => {
                    debug!(
                        status = resp.status().as_u16(),
                        "upstream accepted streaming request after slot-queue wait"
                    );
                    stream_response_to_channel(resp, model_name_owned, tags, tx).await;
                }
                Ok(resp) => {
                    let status = resp.status();
                    let error_bytes = resp.bytes().await.unwrap_or_default();
                    let body_str = String::from_utf8_lossy(&error_bytes);
                    warn!(
                        status = status.as_u16(),
                        body = %body_str,
                        "upstream returned error during slot-queue wait"
                    );
                    let payload = serde_json::json!({
                        "error": {
                            "message": format!("upstream returned {}: {}", status, body_str),
                            "type": "server_error",
                            "code": "upstream_error",
                        }
                    });
                    let frame = format!("data: {payload}\n\ndata: [DONE]\n\n");
                    let _ = tx.send(Ok(Bytes::from(frame))).await;
                }
                Err(e) => {
                    error!("upstream llama-server unreachable during slot-queue wait: {e}");
                    let payload = serde_json::json!({
                        "error": {
                            "message": format!("upstream llama-server unavailable: {e}"),
                            "type": "server_error",
                            "code": "upstream_error",
                        }
                    });
                    let frame = format!("data: {payload}\n\ndata: [DONE]\n\n");
                    let _ = tx.send(Ok(Bytes::from(frame))).await;
                }
            }
        });

        // Return 200 immediately — the client sees a live SSE stream right
        // away, keeps the connection open, and receives keepalive comments
        // while llama.cpp assigns a slot.
        let body = Body::from_stream(async_stream::stream! {
            let mut rx = rx;
            while let Some(item) = rx.recv().await {
                yield item;
            }
        });
        return Ok(Response::builder()
            .status(StatusCode::OK)
            .header("content-type", "text/event-stream")
            .header("cache-control", "no-cache")
            .header("x-accel-buffering", "no")
            .body(body)
            .unwrap_or_else(|_| StatusCode::INTERNAL_SERVER_ERROR.into_response()));
    }

    // ── Non-streaming path (unchanged) ────────────────────────────────────
    let response = match req_builder.body(body).send().await {
        Ok(resp) => resp,
        Err(e) if e.is_connect() || e.is_timeout() => {
            // Connection refused or timed out — the llama-server process is dead
            // or hung.  Signal the caller so it can clear stale state and return
            // a retriable 503 rather than a terminal 502.
            error!("Upstream llama-server unreachable (connect/timeout): {e}");
            return Err(ForwardError::UpstreamDead);
        }
        Err(e) => {
            error!("Failed to send request to llama-server: {e}");
            return Ok((
                StatusCode::BAD_GATEWAY,
                axum::Json(ErrorResponse::upstream_error(&e.to_string())),
            )
                .into_response());
        }
    };

    let status = response.status();

    // For errors, return the error body directly
    if !status.is_success() {
        let error_bytes = response.bytes().await.unwrap_or_default();
        let error_body = String::from_utf8_lossy(&error_bytes);
        warn!(
            status = status.as_u16(),
            body = %error_body,
            "upstream llama-server returned error"
        );
        return Ok(Response::builder()
            .status(StatusCode::from_u16(status.as_u16()).unwrap_or(StatusCode::BAD_GATEWAY))
            .header("content-type", "application/json")
            .body(Body::from(error_bytes))
            .unwrap_or_else(|_| StatusCode::INTERNAL_SERVER_ERROR.into_response()));
    }

    debug!(
        status = status.as_u16(),
        "upstream llama-server accepted request"
    );

    // Non-streaming: read full response. Dialect normalization for
    // non-streaming responses is intentionally deferred — the wire
    // formats we currently rewrite (Qwen XML tool calls, bare <think>
    // tags) only manifest in streaming clients today.
    Ok(forward_non_streaming_response(response).await)
}

/// Extract the `host:port` authority from an HTTP/HTTPS URL string.
///
/// Returns `"127.0.0.1:0"` on any parse failure, which causes the TCP probe
/// to fail immediately (treated as `UpstreamDead`) — the safe fallback that
/// triggers the transparent restart loop in the caller.
fn host_port_from_url(url: &str) -> String {
    // Strip the scheme prefix ("http://" or "https://"), then take everything
    // up to the first path separator as the authority ("host:port").
    url.find("://")
        .and_then(|i| url[i + 3..].split('/').next())
        .filter(|s| !s.is_empty())
        .map(str::to_owned)
        .unwrap_or_else(|| {
            warn!(url, "could not parse host:port from upstream URL; TCP probe will fail safely");
            "127.0.0.1:0".to_owned()
        })
}

/// Feed a streaming response through the normalization pipeline and send each
/// encoded frame to `tx`.
///
/// Used by the keepalive streaming path in [`forward_chat_completion`] where
/// the `Response` has already been returned to the client before llama.cpp
/// assigns a slot.
async fn stream_response_to_channel(
    response: reqwest::Response,
    model_name: String,
    tags: Vec<String>,
    tx: tokio::sync::mpsc::Sender<Result<Bytes, std::io::Error>>,
) {
    let id = format!("chatcmpl-{}", uuid::Uuid::new_v4().simple());
    let created = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    let encoder = SseEncoder::new(id, model_name, created);

    let byte_stream = response.bytes_stream();
    let event_stream = async_stream::stream! {
        let mut decoder = SseStreamDecoder::default();
        let mut byte_stream = std::pin::pin!(byte_stream);

        'outer: while let Some(chunk_result) = byte_stream.next().await {
            let chunk = match chunk_result {
                Ok(c) => c,
                Err(e) => {
                    warn!("upstream SSE byte-stream error: {e}");
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

    let parser = get_parser(&tags);
    let normalized = NormalizingStream::new(Box::pin(event_stream), parser);

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

    let mut wire_stream = Box::pin(wire_stream);
    while let Some(item) = wire_stream.next().await {
        if tx.send(item).await.is_err() {
            // Client disconnected; stop draining the upstream.
            break;
        }
    }
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
