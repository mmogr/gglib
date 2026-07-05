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
use gglib_core::sse::{DONE_SENTINEL, SseEncoder, SseStreamDecoder};

use crate::connections::ConnectionGuard;
use crate::metrics::{ContextMetricsStore, ContextSnapshot};
use crate::models::ErrorResponse;
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
const EMPTY_STREAM_NOTICE: &str =
    "⚠️ [proxy] The model produced no output for this request. The upstream \
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
const FIRST_BYTE_DEADLINE_SECS: u64 = 300;

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
/// * `effective_ctx` - Live context size (tokens) the target llama-server
///   was launched with. Converted to a character budget
///   (`× CHARS_PER_TOKEN_APPROX`) for the history-truncation hard-abort;
///   floored at the historical default inside [`truncate_history`].
/// * `catalog` - Catalog port used to resolve capabilities and `format:*` tags
/// * `metrics` - Metrics store for recording per-request context snapshots
/// * `global_inference_defaults` - Global inference defaults from settings
/// * `connection` - RAII dashboard-registry guard for this request. Moved
///   into the spawned streaming task for the streaming path (so it lives
///   exactly as long as that task); held for the duration of this function
///   for the non-streaming path. Dropping it (by any path — completion,
///   early return, or panic) unregisters the connection from the dashboard.
/// * `upstream_health` - Consecutive-failure watchdog. The streaming task
///   records each terminal outcome (empty stream or first-byte timeout is a
///   strike; any visible output resets it) so the handler can recycle a
///   degraded-but-`/health`-green upstream before the next request.
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
    global_inference_defaults: Option<InferenceConfig>,
    connection: ConnectionGuard,
    upstream_health: Arc<UpstreamHealth>,
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
    //    The budget scales with the live serving context so clients that
    //    plan against the advertised context window are never rejected by a
    //    smaller hidden ceiling (truncate_history floors it at the default).
    let limit_chars =
        (effective_ctx as usize).saturating_mul(crate::truncation::CHARS_PER_TOKEN_APPROX);
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

        // Inject `stream_options.include_usage` and top-level
        // `return_progress` overrides — see `inject_streaming_body_overrides`
        // doc comment for why each is needed.
        let body = inject_streaming_body_overrides(body);

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

        // `connection` is moved into this task so it lives exactly as long
        // as the streaming task does — dropped (unregistering from the
        // dashboard) whether the task finishes normally, the client
        // disconnects (the task is a detached `tokio::spawn`, but `tx` being
        // dropped ends the response body stream, and the task itself exits
        // once `stream_response_to_channel` observes the closed channel), or
        // panics.
        tokio::spawn(async move {
            let connection = connection;
            let mut keepalive_interval = tokio::time::interval(std::time::Duration::from_secs(15));
            keepalive_interval.tick().await; // skip first immediate tick

            // Race: llama.cpp response headers vs 15-second keepalive timer.
            let send_future = req_builder.body(body).send();
            tokio::pin!(send_future);

            // Overall first-byte deadline: bounds pathological slot-queue
            // waits so a wedged upstream cannot hang the client indefinitely
            // on keepalive comments.
            let deadline = tokio::time::sleep(std::time::Duration::from_secs(
                FIRST_BYTE_DEADLINE_SECS,
            ));
            tokio::pin!(deadline);

            let upstream_response = loop {
                tokio::select! {
                    biased;
                    result = &mut send_future => break result,
                    () = &mut deadline => {
                        warn!(
                            deadline_secs = FIRST_BYTE_DEADLINE_SECS,
                            "slot-queue wait exceeded first-byte deadline; treating upstream as degraded"
                        );
                        upstream_health.record_timeout();
                        let visible = visible_content_frame(
                            &model_name_owned,
                            &format!(
                                "⚠️ [proxy] upstream model server did not begin responding within {FIRST_BYTE_DEADLINE_SECS}s — it may be overloaded or wedged. Retry; if it persists the model will be recycled."
                            ),
                        );
                        let payload = serde_json::json!({
                            "error": {
                                "message": format!(
                                    "upstream did not respond within {FIRST_BYTE_DEADLINE_SECS}s"
                                ),
                                "type": "server_error",
                                "code": "upstream_timeout",
                            }
                        });
                        let frame = format!("{visible}data: {payload}\n\ndata: [DONE]\n\n");
                        let _ = tx.send(Ok(Bytes::from(frame))).await;
                        return;
                    }
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
                    let outcome =
                        stream_response_to_channel(resp, model_name_owned, tags, tx, &connection)
                            .await;
                    // Feed the terminal outcome to the watchdog: an empty
                    // response is a strike, visible output resets the streak.
                    upstream_health.record_stream_outcome(outcome.saw_output);
                }
                Ok(resp) => {
                    let status = resp.status();
                    let error_bytes = resp.bytes().await.unwrap_or_default();
                    warn!(
                        status = status.as_u16(),
                        bytes = error_bytes.len(),
                        "upstream returned error during slot-queue wait"
                    );
                    // Preserve the upstream error's `type` and `code` so the
                    // LLM Gateway extension (and VS Code) can identify errors
                    // like `context_length_exceeded` rather than seeing an
                    // opaque `server_error` wrapper.  Falls back to the
                    // generic envelope only when the body is not valid JSON.
                    // No panic paths: every operation is Option/Result-safe.
                    let payload = match serde_json::from_slice::<serde_json::Value>(&error_bytes) {
                        Ok(upstream) => {
                            let msg = upstream
                                .pointer("/error/message")
                                .and_then(serde_json::Value::as_str)
                                .unwrap_or("upstream returned an error");
                            let typ = upstream
                                .pointer("/error/type")
                                .and_then(serde_json::Value::as_str)
                                .unwrap_or("server_error");
                            let code = upstream
                                .pointer("/error/code")
                                .and_then(serde_json::Value::as_str)
                                .unwrap_or("upstream_error");
                            serde_json::json!({
                                "error": { "message": msg, "type": typ, "code": code }
                            })
                        }
                        Err(_) => {
                            // Non-JSON body — fall back to a generic wrapper
                            // that includes the raw bytes as context.
                            let body_str = String::from_utf8_lossy(&error_bytes);
                            serde_json::json!({
                                "error": {
                                    "message": format!(
                                        "upstream returned {}: {}",
                                        status, body_str
                                    ),
                                    "type": "server_error",
                                    "code": "upstream_error",
                                }
                            })
                        }
                    };
                    let frame = format!("data: {payload}\n\ndata: [DONE]\n\n");
                    let human = payload
                        .pointer("/error/message")
                        .and_then(serde_json::Value::as_str)
                        .unwrap_or("upstream returned an error");
                    let visible = visible_content_frame(
                        &model_name_owned,
                        &format!("⚠️ [proxy] upstream model server error ({status}): {human}"),
                    );
                    let frame = format!("{visible}{frame}");
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
                    let visible = visible_content_frame(
                        &model_name_owned,
                        &format!("⚠️ [proxy] upstream llama-server unavailable: {e}"),
                    );
                    let frame = format!("{visible}{frame}");
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
fn visible_content_frame(model: &str, content: &str) -> String {
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
async fn stream_response_to_channel(
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
                _ => {
                    connection.mark_generating();
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
