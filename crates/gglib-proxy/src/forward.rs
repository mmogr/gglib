//! Request forwarding to llama-server with parse → normalize → re-encode
//! pipeline for streaming responses.
//!
//! ## Request pipeline
//!
//! The request-shaping transforms themselves live in
//! [`gglib_core::request_pipeline`], which owns their order and its rationale
//! — they are shared verbatim with the in-process agent path so the two cannot
//! drift.  What is proxy-specific, and therefore still here, is the `Bytes` ⇄
//! `Value` conversion at the HTTP boundary ([`edit_json_body`]) and one stage
//! that cannot live in `gglib-core` at all:
//!
//! 1. [`shape_messages`](gglib_core::request_pipeline::shape_messages) — the
//!    reasoning strip and capability coalescing.
//! 2. [`truncate_history`] — defends against client-side context compaction
//!    failures.  While the request fits within the model's live context
//!    budget it is forwarded **unchanged**; only when the payload exceeds the
//!    budget are the **oldest** unprotected `role: "tool"` / `role:
//!    "assistant"` messages whose string `content` exceeds **2,000
//!    characters** replaced with a short placeholder — just enough of them,
//!    oldest-first, to drop back under budget.  If the payload still exceeds
//!    the budget after every eligible message is trimmed, the request is
//!    rejected with HTTP 400 / `context_length_exceeded`.  The last eight
//!    messages and all `role: "system"` messages are always preserved.
//! 3. [`resolve_sampling`](gglib_core::request_pipeline::resolve_sampling) —
//!    the sampling hierarchy and the `cache_prompt` pin.
//!
//! Truncation is what keeps this a three-call sequence rather than a single
//! [`apply`](gglib_core::request_pipeline::apply): it gates on the payload's
//! size in **wire bytes** and can reject the request with an HTTP response, so
//! it can neither move into `gglib-core` nor be reordered around the sampling
//! stage without changing the number it measures.
//!
//! Capabilities are resolved with a **single** catalog lookup per request
//! (via [`gglib_core::request_pipeline::resolve`]) that yields both the
//! `ModelCapabilities` bitfield (used for request preprocessing) and the
//! `format:*` tags (used for response-stream parser selection).  No second
//! lookup is made.  That resolution is shared with every non-proxy surface,
//! so the proxy and the agent path cannot drift apart on what a model is.
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
use gglib_core::normalize::{NormalizingStream, get_parser};
use gglib_core::ports::ModelCatalogPort;
use gglib_core::request_pipeline::{self, SamplingLayers};
use gglib_core::sse::{DONE_SENTINEL, SseEncoder, SseStreamDecoder};

use crate::cache_metrics::CacheMetricsStore;
use crate::connections::ConnectionGuard;
use crate::metrics::{ContextMetricsStore, ContextSnapshot};
use crate::models::ErrorResponse;
use crate::token_calibration::TokenCalibration;
use crate::truncation::truncate_history;
use crate::upstream_health::UpstreamHealth;

/// Signals that the upstream llama-server was unreachable (connection refused
/// or timed out).  Returned by [`forward_chat_completion`] so the caller can
/// invalidate stale model state and surface a retriable 503 to the client
/// instead of a terminal 502.
#[derive(Debug)]
pub(crate) enum ForwardError {
    /// The upstream llama-server could not be reached (ECONNREFUSED or timeout).
    UpstreamDead,
}

/// Outcome of draining one upstream streaming response through the
/// normalization pipeline, returned by [`stream_response_to_channel`].
///
/// Used by the caller to distinguish a healthy response from a degenerate one
/// (no output at all) for upstream-health bookkeeping.
#[derive(Debug, Default, Clone)]
pub(crate) struct StreamOutcome {
    /// `true` if at least one visible frame (content, reasoning, tool call,
    /// recovered normalization text, or an error frame) was emitted to the
    /// client. `false` means the model produced a completely empty response.
    pub saw_output: bool,
    /// The `finish_reason` from the terminating `Done` event, if one arrived.
    pub finish_reason: Option<String>,
    /// `usage.prompt_tokens` reported by the upstream, if a Usage frame
    /// arrived. Feeds the per-model chars-per-token calibration.
    pub prompt_tokens: Option<u32>,
    /// How many of `prompt_tokens` the upstream served from its KV cache.
    /// `None` when no Usage frame arrived *or* when it omitted the field —
    /// see [`gglib_core::LlmStreamEvent::Usage`] on why absent and zero must
    /// stay distinct. Feeds [`crate::cache_metrics::CacheMetricsStore`].
    pub cached_tokens: Option<u32>,
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

/// Prefix prepended to malformed / unclosed tool-call markup when it is
/// surfaced to the client as visible assistant text instead of being silently
/// dropped.
///
/// The old behaviour logged a `warn!` and discarded the offending bytes, so a
/// turn whose entire output was an unparseable `<tool_call>` reached the client
/// as a zero-content stream — indistinguishable from "the model returned an
/// empty response". Surfacing the raw body (visually flagged) means the human
/// always sees *something* and can tell the model attempted a tool call the
/// proxy could not parse.
const NORMALIZATION_NOTICE_PREFIX: &str = "\n\n⚠️ [proxy: unparsed tool-call output] ";

/// Diagnostic text synthesized and sent to the client when an upstream
/// streaming response completes without emitting a single visible frame.
///
/// Without this, a degenerate generation (the model producing zero tokens —
/// e.g. a wedged or context-overflowed llama-server) reaches the client as a
/// silent empty stream that the LLM Gateway reports as "the model returned an
/// empty response" with no cause. Emitting a visible notice turns that silent
/// failure into a diagnosable one.
const EMPTY_STREAM_NOTICE: &str = "⚠️ [proxy] The model produced no output for this request. The upstream \
     server may be overloaded or degraded — retry, and if it persists restart \
     the model.";

/// Maximum time (seconds) the proxy waits for llama-server to return response
/// headers — i.e. assign a slot and begin the response — during the streaming
/// keepalive wait, before treating the upstream as wedged.
///
/// Large-context prefills on constrained hardware are legitimately slow (a
/// 60k-token prompt can take minutes), so this is generous. Its purpose is to
/// bound *pathological* waits — a degraded or deadlocked llama-server that
/// would otherwise keep the client hanging on keepalive comments indefinitely
/// — not to cap normal prefill latency.
///
/// This is a *per-cycle* bound, not an absolute cap: with `--parallel 1` a
/// second request legitimately queues behind an in-flight one, so when the
/// deadline fires and another connection is still active the wait is extended
/// for another cycle rather than failed (see the keepalive loop). Only an
/// expiry with no other active request counts as degradation.
pub(crate) const FIRST_BYTE_DEADLINE_SECS: u64 = 300;

/// Force-insert the streaming-only overrides every forwarded chat-completion
/// request needs, regardless of what the client sent.
///
/// - `stream_options.include_usage: true` — so llama.cpp emits a final usage
///   SSE chunk with real token counts.  The LLM Gateway extension (v1.1.0)
///   reads `e.usage` in `dispatchParsedChunk` and reports it to VS Code via
///   `LanguageModelDataPart("usage")`, which feeds the context window
///   indicator and enables automatic proactive compaction before the context
///   limit is ever reached.
/// - `return_progress: true` — so llama.cpp emits `prompt_progress` SSE
///   frames during the pre-fill phase (see `gglib_core::sse::parser`).
///   Without this, the proxy dashboard's progress bar has no data to show
///   during pre-fill and the connection appears to jump straight from 0%
///   to "generating" on the first token.
///
/// Both are force-inserted (not `or_insert`) so they take effect even if the
/// client explicitly requested them disabled — the proxy always needs this
/// data for its own bookkeeping.
///
/// Safety: if the body is not a JSON object the original bytes are forwarded
/// unchanged.  No panic paths — every operation returns an `Option`/`Result`
/// and is handled explicitly.
fn inject_streaming_body_overrides(body: Bytes) -> Bytes {
    match serde_json::from_slice::<serde_json::Value>(&body) {
        Ok(mut value) => {
            if let Some(obj) = value.as_object_mut() {
                let stream_opts = obj
                    .entry("stream_options")
                    .or_insert_with(|| serde_json::json!({}));
                if let serde_json::Value::Object(opts) = stream_opts {
                    opts.insert("include_usage".to_owned(), serde_json::Value::Bool(true));
                }
                obj.insert("return_progress".to_owned(), serde_json::Value::Bool(true));
            }
            serde_json::to_vec(&value).map(Bytes::from).unwrap_or(body)
        }
        Err(_) => body, // not JSON — forward as-is
    }
}

/// Run one `gglib-core` request-shaping stage over a body held as `Bytes`.
///
/// The shared stages operate on a `&mut serde_json::Value` — the seam that
/// preserves unknown client fields, which a typed request struct would silently
/// drop.  This is the whole of what the proxy adds on top: the conversion at
/// the HTTP boundary, with zero blast radius at each step.
///
/// * A body that is not JSON is forwarded byte-for-byte.
/// * `edit` reports whether it changed anything; when it did not, the original
///   `Bytes` are returned rather than a re-encoding of the same value.  That
///   matters beyond saving a serialization — [`truncate_history`] sizes its
///   budget from `body.len()`, so re-encoding an untouched body would have it
///   measure something the client never sent.
/// * A re-serialization failure forwards the original and logs.
fn edit_json_body(body: Bytes, edit: impl FnOnce(&mut serde_json::Value) -> bool) -> Bytes {
    let Ok(mut value) = serde_json::from_slice::<serde_json::Value>(&body) else {
        return body;
    };

    if !edit(&mut value) {
        return body;
    }

    match serde_json::to_vec(&value) {
        Ok(v) => Bytes::from(v),
        Err(e) => {
            warn!(error = %e, "failed to re-serialize request body after shaping; forwarding original");
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
/// * `effective_ctx` - Live context size (tokens) the target llama-server
///   was launched with. Converted to a character budget
///   (`× CHARS_PER_TOKEN_APPROX`) for the history-truncation hard-abort;
///   floored at the historical default inside [`truncate_history`].
/// * `catalog` - Catalog port used to resolve capabilities and `format:*` tags
/// * `metrics` - Metrics store for recording per-request context snapshots
/// * `sampling` - The profile and global sampling layers to resolve beneath
///   the client's own request parameters
/// * `connection` - RAII dashboard-registry guard for this request. Moved
///   into the spawned streaming task for the streaming path (so it lives
///   exactly as long as that task); held for the duration of this function
///   for the non-streaming path. Dropping it (by any path — completion,
///   early return, or panic) unregisters the connection from the dashboard.
/// * `upstream_health` - Consecutive-failure watchdog. The streaming task
///   records each terminal outcome (empty stream or first-byte timeout is a
///   strike; any visible output resets it) so the handler can recycle a
///   degraded-but-`/health`-green upstream before the next request.
/// * `permit` - KV cache semaphore permit (streaming path only), moved into
///   the spawned task and held for its entire lifetime. `None` when the KV
///   cache is disabled.
/// * `config` - KV cache lifecycle configuration (streaming path only).
///   `None` when the KV cache is disabled.
/// * `session_id` - Session identifier used to key the KV cache save
///   (streaming path only). `None` when the KV cache is disabled.
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
    effective_ctx: u64,
    catalog: Arc<dyn ModelCatalogPort>,
    metrics: Arc<ContextMetricsStore>,
    sampling: SamplingLayers,
    connection: ConnectionGuard,
    upstream_health: Arc<UpstreamHealth>,
    calibration: Arc<TokenCalibration>,
    cache_metrics: Arc<CacheMetricsStore>,
    permit: Option<tokio::sync::OwnedSemaphorePermit>,
    config: Option<crate::cache_lifecycle::StreamConfig>,
    session_id: Option<String>,
) -> Result<Response, ForwardError> {
    debug!("Forwarding to {upstream_url}, streaming={is_streaming}");

    // Single catalog lookup — yields both capabilities (request preprocessing)
    // and tags (response-stream parser).  Failures return a zero context so
    // all transforms become no-ops rather than blocking the request.
    let context = request_pipeline::resolve(catalog.as_ref(), Some(model_name)).await;

    // ── Request transforms (applied in order) ──────────────────────────────
    //
    // Canonicalization (dynamic IDE-injected lines stripped for system-prompt
    // BPE stability) already happened once in `chat_completions` before this
    // function was called, so the body arriving here doesn't need it again.
    //
    // 1. Message-level shaping: strip prior-turn reasoning artefacts, then
    //    coalesce consecutive same-role messages for strict-turn models.
    //    Shared with the agent path — see `gglib_core::request_pipeline`.
    let body = edit_json_body(body, |v| request_pipeline::shape_messages(v, &context));

    // 2. Truncate stale tool/large-assistant history to prevent local model
    //    context-window overflow caused by broken client-side compaction.
    //    The budget scales with the live serving context so clients that
    //    plan against the advertised context window are never rejected by a
    //    smaller hidden ceiling (truncate_history floors it at the default).
    //    The chars-per-token factor is the model's calibrated ratio (learned
    //    from prior usage frames), falling back to the static default until
    //    the first observation lands.
    let chars_per_token = calibration.chars_per_token(model_name);
    let limit_chars = (effective_ctx as f64 * chars_per_token) as usize;
    let body = match truncate_history(body, limit_chars) {
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

    // 3. Resolve the sampling hierarchy into the body and pin `cache_prompt`.
    //    Both are force-inserts, and both must stay that way — see
    //    `gglib_core::request_pipeline::resolve_sampling`.
    let body = edit_json_body(body, |v| {
        request_pipeline::resolve_sampling(v, &context, &sampling);
        true
    });

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

        // Inject `stream_options.include_usage` and top-level
        // `return_progress` overrides — see `inject_streaming_body_overrides`
        // doc comment for why each is needed.
        let body = inject_streaming_body_overrides(body);

        // Byte count of the payload actually forwarded upstream, paired with
        // the usage frame's prompt-token count after streaming to calibrate
        // this model's chars-per-token ratio.
        let forwarded_chars = body.len();

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

        // Phase 2 — channel-backed response + keepalive background task,
        // relocated to `sse_stream::spawn_and_return` (Step 4).
        let (tx, rx) = tokio::sync::mpsc::channel::<Result<Bytes, std::io::Error>>(32);
        let model_name_owned = model_name.to_owned();
        let tags = context.tags;

        return Ok(crate::sse_stream::spawn_and_return(
            req_builder,
            body,
            tx,
            rx,
            connection,
            model_name_owned,
            tags,
            upstream_health,
            calibration,
            cache_metrics,
            forwarded_chars,
            permit,
            config,
            session_id,
        ));
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
    Ok(forward_non_streaming_response(response, &cache_metrics).await)
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
            warn!(
                url,
                "could not parse host:port from upstream URL; TCP probe will fail safely"
            );
            "127.0.0.1:0".to_owned()
        })
}

/// Build a single SSE `chat.completion.chunk` frame carrying visible assistant
/// `content`.
///
/// Used to surface proxy/upstream failures as text the human can actually read
/// in the chat pane. Some clients (notably the VS Code LLM Gateway) do not
/// render bare inline `{"error": {...}}` frames inside an already-committed
/// 200 stream, so an error delivered only as a structured error frame looks
/// like an empty response. Pairing every such error with a visible content
/// frame guarantees the cause is shown.
pub(crate) fn visible_content_frame(model: &str, content: &str) -> String {
    let created = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    let id = format!("chatcmpl-{}", uuid::Uuid::new_v4().simple());
    let value = serde_json::json!({
        "id": id,
        "object": "chat.completion.chunk",
        "created": created,
        "model": model,
        "choices": [{
            "index": 0,
            "delta": { "content": content },
            "finish_reason": serde_json::Value::Null,
        }],
    });
    format!("data: {value}\n\n")
}

/// Feed a streaming response through the normalization pipeline and send each
/// encoded frame to `tx`.
///
/// Used by the keepalive streaming path in [`forward_chat_completion`] where
/// the `Response` has already been returned to the client before llama.cpp
/// assigns a slot.
///
/// Taps [`LlmStreamEvent::PromptProgress`] frames as they pass through and
/// records them on `connection` (the dashboard registry entry for this
/// request) as a side effect — the frame is still encoded and forwarded to
/// the client unchanged; this never alters what the client receives.
pub(crate) async fn stream_response_to_channel(
    response: reqwest::Response,
    model_name: String,
    tags: Vec<String>,
    tx: tokio::sync::mpsc::Sender<Result<Bytes, std::io::Error>>,
    connection: &ConnectionGuard,
) -> StreamOutcome {
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
    let mut normalized = Box::pin(normalized);

    let mut outcome = StreamOutcome::default();
    let mut client_connected = true;
    while let Some(event) = normalized.next().await {
        let frame: Option<Bytes> = match event {
            Ok(ev) => match &ev {
                LlmStreamEvent::PromptProgress {
                    processed,
                    total,
                    cached,
                    time_ms,
                } => {
                    connection.update_progress(*processed, *total, *cached, *time_ms);
                    encoder.encode(&ev).map(Bytes::from)
                }
                LlmStreamEvent::NormalizationError { kind, raw } => {
                    // Surface the discarded body as visible assistant text
                    // rather than dropping it silently (which manifested as an
                    // empty response when the whole turn was one bad tool call).
                    warn!(?kind, raw = %raw, "proxy: surfacing normalization issue as visible content");
                    outcome.saw_output = true;
                    let recovered = LlmStreamEvent::TextDelta {
                        content: format!("{NORMALIZATION_NOTICE_PREFIX}{raw}"),
                    };
                    encoder.encode(&recovered).map(Bytes::from)
                }
                LlmStreamEvent::TextDelta { .. }
                | LlmStreamEvent::ReasoningDelta { .. }
                | LlmStreamEvent::ToolCallDelta { .. }
                | LlmStreamEvent::UpstreamError { .. } => {
                    connection.mark_generating();
                    outcome.saw_output = true;
                    encoder.encode(&ev).map(Bytes::from)
                }
                LlmStreamEvent::Done { finish_reason } => {
                    connection.mark_generating();
                    outcome.finish_reason = Some(finish_reason.clone());
                    encoder.encode(&ev).map(Bytes::from)
                }
                LlmStreamEvent::Usage {
                    prompt_tokens,
                    cached_tokens,
                    ..
                } => {
                    // Trailing usage frame — capture the real prompt-token
                    // count for chars-per-token calibration, and the cached
                    // count for prompt-cache telemetry. Not counted as
                    // visible output (it carries an empty `choices` array).
                    outcome.prompt_tokens = Some(*prompt_tokens);
                    outcome.cached_tokens = *cached_tokens;
                    encoder.encode(&ev).map(Bytes::from)
                }
            },
            Err(e) => {
                error!("proxy stream error: {e}");
                outcome.saw_output = true;
                let payload = serde_json::json!({
                    "error": {
                        "message": e.to_string(),
                        "type": "server_error",
                        "code": "upstream_error",
                    }
                });
                // No inline [DONE] here -- appended once, unconditionally,
                // after the wire stream is exhausted (see below).
                Some(Bytes::from(format!("data: {payload}\n\n")))
            }
        };

        if let Some(bytes) = frame
            && tx.send(Ok(bytes)).await.is_err()
        {
            // Client disconnected; stop draining the upstream.
            client_connected = false;
            break;
        }
    }

    // Empty-stream detector: if the upstream completed without emitting a
    // single visible frame, synthesize a diagnostic so the client never sees
    // an unexplained empty response. Skipped when the client already
    // disconnected (nothing to send) or when output was surfaced.
    if client_connected && !outcome.saw_output {
        let reason = outcome.finish_reason.as_deref().unwrap_or("none");
        warn!(
            finish_reason = %reason,
            "proxy: upstream stream produced no visible output; emitting diagnostic"
        );
        let notice = format!("{EMPTY_STREAM_NOTICE} (finish_reason: {reason})");
        if let Some(s) = encoder.encode(&LlmStreamEvent::TextDelta { content: notice }) {
            let _ = tx.send(Ok(Bytes::from(s))).await;
        }
    }
    // Exactly one [DONE] sentinel, sent once the wire stream is truly
    // exhausted -- never bundled into an individual event's encoding, since
    // a trailing Usage event can legitimately follow Done (see
    // `gglib_core::sse::DONE_SENTINEL` doc). Skipped if the client already
    // disconnected -- the channel is closed, nothing to send.
    if client_connected {
        let _ = tx
            .send(Ok(Bytes::from_static(DONE_SENTINEL.as_bytes())))
            .await;
    }

    outcome
}

/// Extract `(prompt_tokens, cached_tokens)` from a non-streaming response body.
///
/// The streaming path gets these from a typed `Usage` event; a non-streaming
/// response carries the same figures in its terminal JSON instead, so they are
/// read here rather than leaving this path silently absent from the telemetry.
///
/// Returns `None` when the body isn't JSON or carries no `usage.prompt_tokens`
/// — nothing is recorded in that case, rather than recording a zero that would
/// dilute the totals. The inner `cached_tokens` stays `Option` for the reason
/// given on [`gglib_core::LlmStreamEvent::Usage`]: absent and zero differ.
fn usage_from_response_body(body: &[u8]) -> Option<(u32, Option<u32>)> {
    let parsed: serde_json::Value = serde_json::from_slice(body).ok()?;
    let usage = parsed.get("usage")?;
    let prompt_tokens = u32::try_from(usage.get("prompt_tokens")?.as_u64()?).ok()?;
    let cached_tokens = usage
        .get("prompt_tokens_details")
        .and_then(|d| d.get("cached_tokens"))
        .and_then(serde_json::Value::as_u64)
        .map(|v| u32::try_from(v).unwrap_or(u32::MAX));
    Some((prompt_tokens, cached_tokens))
}

/// Forward a non-streaming JSON response from llama-server.
async fn forward_non_streaming_response(
    response: reqwest::Response,
    cache_metrics: &CacheMetricsStore,
) -> Response {
    // Collect upstream headers we want to preserve
    let content_type = response
        .headers()
        .get("content-type")
        .cloned()
        .unwrap_or_else(|| HeaderValue::from_static("application/json"));

    // Read the full body
    match response.bytes().await {
        Ok(body_bytes) => {
            // Body is already fully buffered, so this is a parse of bytes we
            // hold rather than extra I/O. Failure is silent by design: an
            // unparseable body still forwards verbatim, since telemetry must
            // never change what the client receives.
            if let Some((prompt_tokens, cached_tokens)) = usage_from_response_body(&body_bytes) {
                cache_metrics.record(prompt_tokens, cached_tokens);
            }
            Response::builder()
                .status(StatusCode::OK)
                .header("content-type", content_type)
                .body(Body::from(body_bytes))
                .unwrap_or_else(|_| StatusCode::INTERNAL_SERVER_ERROR.into_response())
        }
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
        let body = Bytes::from(
            r#"{"model":"foo","messages":[],"stream_options":{"include_usage":false}}"#,
        );
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
    // boundary — specifically, the conditions under which the client's
    // original payload must be forwarded rather than a re-encoding of it.

    #[test]
    fn edit_json_body_reserializes_when_the_stage_reports_a_change() {
        let body = Bytes::from(r#"{"model":"m","keep":1}"#);
        let out = edit_json_body(body, |v| {
            v["added"] = serde_json::Value::Bool(true);
            true
        });
        let parsed: serde_json::Value = serde_json::from_slice(&out).expect("valid json");
        assert_eq!(parsed["added"], true);
        assert_eq!(parsed["keep"], 1, "untouched fields survive");
    }

    /// A stage reporting no change must cost the body nothing — not even a
    /// re-encoding, which would reorder keys and change `body.len()` under
    /// `truncate_history`.
    #[test]
    fn edit_json_body_returns_the_original_bytes_when_nothing_changed() {
        // Deliberately not in the canonical form `serde_json` would emit:
        // whitespace and key order both differ from a round-trip.
        let body = Bytes::from(r#"{ "zeta": 1,  "alpha": 2 }"#);
        let out = edit_json_body(body.clone(), |_| false);
        assert_eq!(out, body, "must be the same bytes, not the same value");
    }

    #[test]
    fn edit_json_body_leaves_non_json_bodies_alone() {
        let body = Bytes::from_static(b"not json");
        let out = edit_json_body(body.clone(), |v| {
            *v = serde_json::json!({"should": "never happen"});
            true
        });
        assert_eq!(out, body);
    }
}
