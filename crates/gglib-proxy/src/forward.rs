//! Request forwarding to llama-server with parse → normalize → re-encode
//! pipeline for streaming responses.
//!
//! ## Request pipeline
//!
//! The request-shaping transforms live in [`gglib_core::request_pipeline`],
//! which owns their order and its rationale, and the proxy runs the whole of
//! it with a single [`apply`](gglib_core::request_pipeline::apply) call — the
//! same call the in-process agent path makes, so the two cannot drift.  What
//! is proxy-specific, and therefore still here, is exactly two things:
//! the `Bytes` ⇄ `Value` conversion at the HTTP boundary
//! ([`shape_request_body`]), and mapping the pipeline's one failure mode onto
//! this surface's wire contract — HTTP 400 / `context_length_exceeded`.
//!
//! The proxy differs from the agent path in one input: its truncation budget
//! comes from the **live** serving context of the running llama-server, scaled
//! by a per-model chars-per-token ratio learned from observed usage frames
//! ([`crate::token_calibration`]), rather than from the model's nominal
//! context length.  Both numbers describe the same thing; the proxy simply has
//! a better one available.
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
use gglib_core::request_pipeline::{
    self, ModelContext, SamplingLayers, TruncationError, TruncationReport,
};
use gglib_core::sse::{DONE_SENTINEL, SseEncoder, SseStreamDecoder};

use crate::connections::ConnectionGuard;
use crate::metrics::{ContextMetricsStore, ContextSnapshot};
use crate::models::ErrorResponse;
use crate::token_calibration::TokenCalibration;
use crate::upstream_health::UpstreamHealth;
use gglib_core::cache_metrics::CacheMetricsStore;

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
    /// `true` if at least one *client-renderable* frame (content, tool call,
    /// recovered normalization text, or an error frame) was emitted.
    ///
    /// Reasoning deliberately does not count. A turn whose entire output landed
    /// in `reasoning_content` renders as an empty response in every client that
    /// treats reasoning as a collapsed side-channel (notably the VS Code LLM
    /// Gateway), so scoring it as output made a hard failure indistinguishable
    /// from success — and, via [`crate::upstream_health`], reset the strike
    /// counter that arms the recycle watchdog. See [`Self::saw_reasoning`].
    pub saw_visible_output: bool,
    /// `true` if at least one `ReasoningDelta` was emitted.
    ///
    /// Tracked separately from [`Self::saw_visible_output`] so a reasoning-only
    /// turn is distinguishable both from a healthy response and from a wholly
    /// empty one.
    pub saw_reasoning: bool,
    /// The `finish_reason` from the terminating `Done` event, if one arrived.
    pub finish_reason: Option<String>,
    /// `usage.prompt_tokens` reported by the upstream, if a Usage frame
    /// arrived. Feeds the per-model chars-per-token calibration.
    pub prompt_tokens: Option<u32>,
    /// How many of `prompt_tokens` the upstream served from its KV cache.
    /// `None` when no Usage frame arrived *or* when it omitted the field —
    /// see [`gglib_core::LlmStreamEvent::Usage`] on why absent and zero must
    /// stay distinct. Feeds [`gglib_core::cache_metrics::CacheMetricsStore`].
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

/// Prefix prepended to reasoning text that is promoted into the content
/// channel because the turn produced no visible output of its own.
///
/// Same rescue as [`NORMALIZATION_NOTICE_PREFIX`], one channel over: a model
/// that never closes its `<think>` block leaves a complete, often correct
/// answer stranded in `reasoning_content`, which clients that collapse
/// reasoning render as an empty response. Promoting it makes the turn usable;
/// the flag keeps the underlying degradation visible instead of silently
/// papering over it.
const REASONING_ONLY_NOTICE_PREFIX: &str = "\n\n⚠️ [proxy: reasoning-only response] ";

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

/// The wire contract for a conversation that cannot be trimmed to fit.
///
/// HTTP 400 with both `error.type` and `error.code` set to
/// `context_length_exceeded`.  Clients — the GitHub Copilot LLM Gateway
/// extension among them — branch on this, so the status, the two codes and the
/// message are a public interface of the proxy and not an implementation
/// detail of [`shape_request_body`].
fn context_length_exceeded_response() -> Response {
    (
        StatusCode::BAD_REQUEST,
        axum::Json(ErrorResponse::context_length_exceeded()),
    )
        .into_response()
}

/// Run the shared request-shaping pipeline over a body held as `Bytes`.
///
/// The pipeline operates on a `&mut serde_json::Value` — the seam that
/// preserves unknown client fields, which a typed request struct would silently
/// drop.  This is the whole of what the proxy adds on top: the conversion at
/// the HTTP boundary, with zero blast radius.
///
/// A body that is not JSON is forwarded byte-for-byte and reported as
/// unmeasured — the upstream can produce its own diagnostic for it.  A
/// re-serialization failure likewise forwards the original and logs; `Value`
/// serialization has no reachable failure mode, but forwarding beats dropping.
///
/// # Errors
///
/// [`TruncationError`] when the conversation cannot be made to fit
/// `budget_chars`.  The caller maps it to the wire contract.
fn shape_request_body(
    body: Bytes,
    ctx: &ModelContext,
    layers: &SamplingLayers,
    budget_chars: Option<usize>,
) -> Result<(Bytes, TruncationReport), TruncationError> {
    let Ok(mut value) = serde_json::from_slice::<serde_json::Value>(&body) else {
        return Ok((body, TruncationReport::default()));
    };

    let report = request_pipeline::apply(&mut value, ctx, layers, budget_chars)?;

    match serde_json::to_vec(&value) {
        Ok(v) => Ok((Bytes::from(v), report)),
        Err(e) => {
            warn!(error = %e, "failed to re-serialize request body after shaping; forwarding original");
            Ok((body, TruncationReport::default()))
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

    // ── Request shaping ────────────────────────────────────────────────────
    //
    // Canonicalization (dynamic IDE-injected lines stripped for system-prompt
    // BPE stability) already happened once in `chat_completions` before this
    // function was called, so the body arriving here doesn't need it again.
    //
    // Everything else is one call into the shared pipeline. Its budget is the
    // live serving context scaled by this model's calibrated chars-per-token
    // ratio, learned from prior usage frames and falling back to the static
    // approximation until the first observation lands. That is strictly better
    // information than `ModelContext::context_budget_chars()` — which is what
    // the agent path uses — so the proxy passes its own.
    let chars_per_token = calibration.chars_per_token(model_name);
    let budget_chars = Some((effective_ctx as f64 * chars_per_token) as usize);

    let (body, report) = match shape_request_body(body, &context, &sampling, budget_chars) {
        Ok(shaped) => shaped,
        Err(e) => {
            // Hard abort: the conversation cannot be trimmed to fit. Record a
            // clamped snapshot — the zeroed char counts are how the dashboard
            // tells a clamped request from a measured one — then reject with
            // the wire contract clients already handle.
            debug!(error = %e, "rejecting request that exceeds the context budget");
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
            return Ok(context_length_exceeded_response());
        }
    };

    // Deliberate noise control: history truncation is routine enough that
    // logging every no-op would drown the interesting case.
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

    debug!(
        body_bytes = body.len(),
        "sending request to upstream (post-transform)"
    );

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
    // Accumulates reasoning text for the promotion path below. Bounded in
    // practice by the request's `max_tokens`.
    let mut reasoning_buf = String::new();
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
                    outcome.saw_visible_output = true;
                    let recovered = LlmStreamEvent::TextDelta {
                        content: format!("{NORMALIZATION_NOTICE_PREFIX}{raw}"),
                    };
                    encoder.encode(&recovered).map(Bytes::from)
                }
                LlmStreamEvent::ReasoningDelta { content } => {
                    // Forwarded unchanged, but NOT counted as visible output —
                    // clients that collapse reasoning render this as empty.
                    // Buffered so it can be promoted if the turn ends without
                    // ever producing content of its own.
                    connection.mark_generating();
                    outcome.saw_reasoning = true;
                    reasoning_buf.push_str(content);
                    encoder.encode(&ev).map(Bytes::from)
                }
                LlmStreamEvent::TextDelta { .. }
                | LlmStreamEvent::ToolCallDelta { .. }
                | LlmStreamEvent::UpstreamError { .. } => {
                    connection.mark_generating();
                    outcome.saw_visible_output = true;
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
                outcome.saw_visible_output = true;
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

    // No visible output: either rescue the turn or explain it. Skipped when the
    // client already disconnected (nothing to send).
    if client_connected && !outcome.saw_visible_output {
        let reason = outcome.finish_reason.as_deref().unwrap_or("none");
        let notice = if outcome.saw_reasoning {
            // Reasoning-only: the answer is usually complete and correct, just
            // stranded in the wrong channel. Promote it rather than letting the
            // client see an empty turn and retry a prompt that will deterministically
            // strand it again.
            warn!(
                finish_reason = %reason,
                reasoning_bytes = reasoning_buf.len(),
                "proxy: response was reasoning-only; promoting reasoning to content"
            );
            format!("{REASONING_ONLY_NOTICE_PREFIX}{reasoning_buf}")
        } else {
            warn!(
                finish_reason = %reason,
                "proxy: upstream stream produced no visible output; emitting diagnostic"
            );
            format!("{EMPTY_STREAM_NOTICE} (finish_reason: {reason})")
        };
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
        let (out, report) = shape_request_body(
            body,
            &ModelContext::passthrough(),
            &SamplingLayers::default(),
            None,
        )
        .expect("no budget, so nothing to reject");

        let parsed: serde_json::Value = serde_json::from_slice(&out).expect("valid json");
        assert_eq!(parsed["cache_prompt"], true, "the pipeline ran");
        assert!(parsed["temperature"].is_number());
        assert_eq!(parsed["totally_made_up"], serde_json::json!({"a": 1}));
        assert_eq!(report, TruncationReport::default(), "unmeasured, no budget");
    }

    #[test]
    fn shaping_leaves_non_json_bodies_alone() {
        let body = Bytes::from_static(b"not json at all");
        let (out, report) = shape_request_body(
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
        let (out, report) = shape_request_body(
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
}
