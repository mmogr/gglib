//! Request forwarding to llama-server with parse → normalize → re-encode
//! pipeline for streaming responses.
//!
//! ## Request pipeline
//!
//! The request-shaping transforms live in [`gglib_core::request_pipeline`],
//! which owns their order and its rationale, and the proxy runs the whole of
//! it with a single [`apply`](gglib_core::request_pipeline::apply()) call — the
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
//! (via [`gglib_core::request_pipeline::resolve()`]) that yields both the
//! `ModelCapabilities` bitfield (used for request preprocessing) and the
//! `format:*` tags (used for response-stream parser selection).  No second
//! lookup is made.  That resolution is shared with every non-proxy surface,
//! so the proxy and the agent path cannot drift apart on what a model is.
//!
//! That lookup happens in `chat_completions`, *before* the model is ensured
//! running, and the resulting [`ModelContext`] arrives here on
//! [`ForwardRequest`].  It used to happen here, which was one round-trip
//! either way — but resolving before the swap is what lets the handler refuse
//! a request the loaded model could never serve without paying for a model
//! load first.
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
//! Non-streaming responses run through the same parser in one shot
//! ([`gglib_core::normalize::normalize_chat_completion_body`]), so a
//! `stream: false` client gets the same structured `tool_calls` a streaming
//! client would.

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
use gglib_core::domain::DialectSpec;
use gglib_core::normalize::{NormalizingStream, get_parser};
use gglib_core::request_pipeline::{
    self, ModelContext, SamplingDecision, SamplingLayers, SuppressedEffort, TruncationError,
    TruncationReport,
};
use gglib_core::sse::{DONE_SENTINEL, SseEncoder, SseStreamDecoder};

use crate::connections::ConnectionGuard;
use crate::metrics::{ContextMetricsStore, ContextSnapshot};
use crate::models::ErrorResponse;
use crate::sampling_audit::SamplingAuditStore;
use crate::token_calibration::TokenCalibration;
use crate::upstream_health::{StreamVerdict, UpstreamHealth};
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
    /// from success. See [`Self::saw_reasoning`].
    ///
    /// This answers "did the client see anything?", and *only* that — it is
    /// what decides whether the turn needs a diagnostic notice appended. It is
    /// deliberately not the upstream-health verdict: an error frame is
    /// renderable but is the opposite of healthy. [`Self::health_verdict`]
    /// draws that second distinction.
    pub saw_visible_output: bool,
    /// `true` if at least one `ReasoningDelta` was emitted.
    ///
    /// Tracked separately from [`Self::saw_visible_output`] so a reasoning-only
    /// turn is distinguishable both from a healthy response and from a wholly
    /// empty one.
    pub saw_reasoning: bool,
    /// The `finish_reason` from the terminating `Done` event, if one arrived.
    pub finish_reason: Option<String>,
    /// A tool call failed schema validation and a repair re-issue was made.
    ///
    /// Recorded whether or not it worked: an attempt is evidence that this
    /// model's `auto` path is unconstrained, which is the per-model
    /// grammar-presence signal ADR 0002 left with no runtime source.
    pub repair_attempted: bool,
    /// The repair produced a conformant call and replaced the original.
    pub repair_succeeded: bool,
    /// `usage.prompt_tokens` reported by the upstream, if a Usage frame
    /// arrived. Feeds the per-model chars-per-token calibration.
    pub prompt_tokens: Option<u32>,
    /// How many of `prompt_tokens` the upstream served from its KV cache.
    /// `None` when no Usage frame arrived *or* when it omitted the field —
    /// see [`gglib_core::LlmStreamEvent::Usage`] on why absent and zero must
    /// stay distinct. Feeds [`gglib_core::cache_metrics::CacheMetricsStore`].
    pub cached_tokens: Option<u32>,
    /// First dialect marker found in the client-visible text after
    /// normalization, if any (see `gglib_core::normalize::residue`). The
    /// spawning task back-patches the request's metrics snapshot with it.
    pub dialect_residue: Option<String>,
    /// The turn died on an *upstream* failure: the model server emitted an
    /// error event mid-generation, or the byte stream itself broke. The
    /// client's turn simply fails — the failure a person actually feels.
    ///
    /// Client disconnects never set this; hanging up is not a model defect.
    pub upstream_errored: bool,
    /// Normalization discarded a malformed dialect tool call and surfaced the
    /// raw body as visible text instead.
    ///
    /// A pure model defect, and one the existing repair never sees: repair
    /// fires on schema violations of *parsed* calls, so a call too malformed
    /// to parse falls outside it entirely.
    pub normalization_errored: bool,
    /// The turn's tool call could not be validated, so repair never had an
    /// opinion to act on. See [`crate::repair::Skipped::Unvalidatable`].
    pub unvalidatable_schema: bool,
    /// The client went away mid-turn, so the drain loop stopped forwarding.
    ///
    /// Kept because a turn the client abandoned is not evidence about the
    /// upstream — see [`StreamVerdict::ClientAborted`].
    pub client_aborted: bool,
}

impl StreamOutcome {
    /// Reduce the turn to the one claim it makes about the upstream's health.
    ///
    /// The precedence is the whole content of this function, so it is stated
    /// once here rather than at the call site:
    ///
    /// 1. **Died upstream** wins outright. A turn that broke mid-generation
    ///    indicts the server even if it had already emitted good text.
    /// 2. **Visible output** beats a client disconnect. If the upstream
    ///    demonstrably produced, that is positive evidence of health and the
    ///    client leaving afterwards does not retract it.
    /// 3. **Client abort** with nothing produced is genuinely unknowable — the
    ///    model may have been mid-prefill — so it abstains.
    /// 4. Otherwise the turn produced nothing and nobody left: an empty
    ///    response, the degradation this watchdog was built for.
    pub(crate) fn health_verdict(&self) -> StreamVerdict {
        if self.upstream_errored {
            StreamVerdict::UpstreamError
        } else if self.saw_visible_output {
            StreamVerdict::Healthy
        } else if self.client_aborted {
            StreamVerdict::ClientAborted
        } else {
            StreamVerdict::Empty
        }
    }
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
pub(crate) fn should_forward_header(name: &str) -> bool {
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
/// This is a *per-cycle* bound, not an absolute cap: another connection can
/// legitimately be occupying the upstream — a co-resident model serving from
/// the second slot, or a request the admission queue admitted just before this
/// one — so when the deadline fires and another connection is still active the
/// wait is extended for another cycle rather than failed (see the keepalive
/// loop). Only an expiry with no other active request counts as degradation.
///
/// Requests for the *same* model no longer stack up behind each other here:
/// admission caps them at what llama-server was launched to serve at once, so
/// that queue forms in the runtime rather than inside the upstream.
pub(crate) const FIRST_BYTE_DEADLINE_SECS: u64 = 300;

/// How long a repair re-issue may take before it is abandoned.
///
/// The proxy's own client is deliberately built with **no** request timeout —
/// a 36k-token prompt can need minutes of prefill before its first token, and
/// a global timeout would kill legitimate work. That is right for the client's
/// own turn and wrong here, because a re-issue happens *while the client is
/// receiving nothing*: the frames it would have shown are being withheld, the
/// keepalive that guarded the wait for a slot has already stopped, and the
/// drain loop has no timer of its own.
///
/// So this is the one bound that keeps an unresponsive upstream from turning a
/// repair into an unbounded silence. Comfortably under
/// [`FIRST_BYTE_DEADLINE_SECS`], because a re-issue is a *constrained* call —
/// `tool_choice: "required"` and non-streaming — not a fresh full turn. On
/// expiry the turn falls open to the original frames, exactly as every other
/// repair failure path already does.
const REPAIR_REISSUE_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(60);

/// How often to push an SSE comment while a re-issue is in flight.
///
/// Invisible to clients — `parse_sse_frames` and every conforming SSE reader
/// skip lines that are not `data:` — but enough to stop an idle proxy, load
/// balancer or editor from concluding the connection died. Without it the
/// client sees zero bytes for the whole re-issue, since tool-call frames are
/// withheld from the moment they arrive.
const REPAIR_KEEPALIVE_INTERVAL: std::time::Duration = std::time::Duration::from_secs(15);

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

/// What [`shape_request_body`] did, for the caller that reports it.
#[derive(Debug)]
struct ShapedRequest {
    /// The body to forward — shaped, or the original when shaping could not
    /// be applied.
    body: Bytes,
    /// Zeroed when nothing was measured.
    truncation: TruncationReport,
    /// Whether the pipeline originated a decode-time tool-call grammar.
    grammar_enforced: bool,
    /// What the sampling stage resolved, when it ran at all.
    ///
    /// `None` on a body that was never JSON: the pipeline did not execute, so
    /// there is no decision — which is a different fact from a decision that
    /// executed and did not reach the wire. That second case is a decision
    /// with `applied: false`, and [`crate::sampling_audit`] treats the two
    /// identically only because both mean "nothing to compare".
    sampling: Option<SamplingDecision>,
    /// The `reasoning_effort` stage 5b threw away, if it threw one away.
    ///
    /// Carried separately from [`Self::sampling`] because it is not recoverable
    /// from it: the gate clears `resolved.reasoning_effort` and overwrites the
    /// rung in `sources` with
    /// [`SuppressedByTemplate`](gglib_core::domain::ParamSource::SuppressedByTemplate),
    /// so by the time a decision reaches a consumer, *which* level was dropped
    /// and *who* asked for it are gone.
    ///
    /// This field is the fix for a real drop: both construction sites below
    /// discarded `PipelineReport::effort_suppressed`, so on every request
    /// against a model whose template ignores the variable, the pipeline
    /// computed the record ADR 0007 exists to preserve and the proxy binned it.
    /// Nothing downstream could notice — neither reasoning control is echoed by
    /// any readback (finding 7a), so there was no wire evidence to contradict.
    effort_suppressed: Option<SuppressedEffort>,
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
) -> Result<ShapedRequest, TruncationError> {
    let Ok(mut value) = serde_json::from_slice::<serde_json::Value>(&body) else {
        return Ok(ShapedRequest {
            body,
            truncation: TruncationReport::default(),
            grammar_enforced: false,
            sampling: None,
            // The pipeline never ran, so no stage could have suppressed
            // anything. Distinct from `Some`-less-because-nothing-was-dropped
            // only in that there was also no decision to drop it from.
            effort_suppressed: None,
        });
    };

    // The constrain stage never engages over a client-sent grammar, so
    // before-absent + after-present is exactly "the pipeline originated one".
    let grammar_before = value.get("grammar").is_some();
    let report = request_pipeline::apply(&mut value, ctx, layers, budget_chars)?;
    let grammar_enforced = !grammar_before && value.get("grammar").is_some();

    match serde_json::to_vec(&value) {
        Ok(v) => Ok(ShapedRequest {
            body: Bytes::from(v),
            truncation: report.truncation,
            grammar_enforced,
            sampling: Some(report.sampling),
            effort_suppressed: report.effort_suppressed,
        }),
        Err(e) => {
            warn!(error = %e, "failed to re-serialize request body after shaping; forwarding original");
            // The shaped body is being discarded, so the values the sampling
            // stage wrote into it never reach upstream. Recording the decision
            // as applied here would hand the readback an intent that was never
            // sent, and every field of it would read as a divergence.
            let mut sampling = report.sampling;
            sampling.applied = false;
            Ok(ShapedRequest {
                body,
                truncation: TruncationReport::default(),
                grammar_enforced: false,
                sampling: Some(sampling),
                // Reported even though the shaped body was discarded, and
                // unlike `applied` it is not cleared. The two say different
                // things: `applied: false` means the resolved values never
                // reached upstream, while a suppression is a fact about what
                // gglib *decided*, and the original body being forwarded does
                // not un-decide it. The level still did not reach the model —
                // the client's own body carried no `reasoning_effort` from the
                // ladder to begin with.
                effort_suppressed: report.effort_suppressed,
            })
        }
    }
}

/// Bundles the arguments to [`forward_chat_completion`] that stay constant
/// across the cache-branching in `chat_completions` — only the trailing
/// `(permit, config, session_id)` triple passed to [`Self::send`] varies
/// between the non-streaming/streaming/fail-open/cache-disabled branches, and
/// between a request's primary attempt and its post-`UpstreamDead` retry.
pub(crate) struct ForwardRequest<'a> {
    /// HTTP client to use for the request.
    pub client: &'a Client,
    /// Full URL to the llama-server endpoint.
    pub upstream_url: &'a str,
    /// Original request headers.
    pub headers: &'a HeaderMap,
    /// Request body bytes.
    pub body: Bytes,
    /// Whether this is a streaming request (affects response handling).
    pub is_streaming: bool,
    /// Model name to advertise to the client (used in the SSE envelope).
    pub model_name: &'a str,
    /// Live context size (tokens) the target llama-server was launched
    /// with. Converted to a character budget (`× CHARS_PER_TOKEN_APPROX`)
    /// for the history-truncation hard-abort; floored at the historical
    /// default inside
    /// [`truncate_history`](gglib_core::request_pipeline::truncate_history).
    pub effective_ctx: u64,
    /// This model's stored capabilities and tags, resolved once by
    /// `chat_completions` before the model was ensured running.
    ///
    /// Passed in rather than re-resolved here so the one catalog round-trip a
    /// request pays for is also the one that decided, ahead of any model swap,
    /// whether the request should have been forwarded at all.
    pub context: ModelContext,
    /// Metrics store for recording per-request context snapshots.
    pub metrics: Arc<ContextMetricsStore>,
    /// The profile and global sampling layers to resolve beneath the
    /// client's own request parameters.
    pub sampling: SamplingLayers,
    /// RAII dashboard-registry guard for this request. Moved into the
    /// spawned streaming task for the streaming path (so it lives exactly
    /// as long as that task); held for the duration of
    /// [`forward_chat_completion`] for the non-streaming path. Dropping it
    /// (by any path — completion, early return, or panic) unregisters the
    /// connection from the dashboard.
    pub connection: ConnectionGuard,
    /// Consecutive-failure watchdog. The streaming task records each
    /// terminal outcome (empty stream or first-byte timeout is a strike;
    /// any visible output resets it) so the handler can recycle a
    /// degraded-but-`/health`-green upstream before the next request.
    pub upstream_health: Arc<UpstreamHealth>,
    /// Per-model chars-per-token calibration store.
    pub calibration: Arc<TokenCalibration>,
    /// Session id used to look up/freeze this session's chars-per-token
    /// snapshot (see
    /// [`crate::token_calibration::TokenCalibration::session_chars_per_token`]);
    /// `None` when no session id was resolved, which falls back to the live
    /// per-model ratio exactly as before. Distinct from the `session_id`
    /// parameter of [`Self::send`]: that one is only populated when disk
    /// KV-slot caching is enabled, but this budget-stability fix must work
    /// even when it's off (e.g. for hybrid/sliding-window-attention models,
    /// where disk caching is disabled but the host-RAM prompt cache — the
    /// thing this fix protects — still applies).
    pub calibration_session_id: Option<&'a str>,
    /// Cache-hit telemetry sink, fed from both the streaming and
    /// non-streaming response paths.
    pub cache_metrics: Arc<CacheMetricsStore>,
    /// Whether a tool call failing schema validation is re-issued with
    /// `tool_choice: "required"`.
    ///
    /// From `Settings.tool_call_repair`, which is absent-means-on. Resolved
    /// where the settings snapshot already lives rather than read again here,
    /// so one request cannot see two different answers.
    pub repair_enabled: bool,
    /// Tier C sink for what the sampling stage resolved. See
    /// [`crate::sampling_audit`] — this half records the intent; the `/slots`
    /// poller supplies the observation to compare it against.
    pub sampling_audit: Arc<SamplingAuditStore>,
}

impl ForwardRequest<'_> {
    /// Forward this request to the upstream llama-server, participating in
    /// the disk KV cache according to `(permit, config, session_id)`.
    ///
    /// * `permit` - KV cache semaphore permit (streaming path only), moved
    ///   into the spawned task and held for its entire lifetime. `None`
    ///   when the KV cache is disabled.
    /// * `config` - KV cache lifecycle configuration (streaming path only).
    ///   `None` when the KV cache is disabled.
    /// * `session_id` - Session identifier used to key the KV cache save
    ///   (streaming path only). `None` when the KV cache is disabled.
    ///
    /// Returns the response from llama-server, with the streaming SSE body
    /// re-emitted through the universal normalization pipeline when
    /// `is_streaming` is true.
    pub(crate) async fn send(
        self,
        permit: Option<tokio::sync::OwnedSemaphorePermit>,
        config: Option<crate::cache_lifecycle::StreamConfig>,
        session_id: Option<String>,
    ) -> Result<Response, ForwardError> {
        forward_chat_completion(self, permit, config, session_id).await
    }
}

/// Forward a chat completion request to the upstream llama-server.
///
/// See [`ForwardRequest`] for what `req`'s fields mean, and
/// [`ForwardRequest::send`] (its sole caller) for the trailing cache triple.
pub(crate) async fn forward_chat_completion(
    req: ForwardRequest<'_>,
    permit: Option<tokio::sync::OwnedSemaphorePermit>,
    config: Option<crate::cache_lifecycle::StreamConfig>,
    session_id: Option<String>,
) -> Result<Response, ForwardError> {
    let ForwardRequest {
        client,
        upstream_url,
        headers,
        body,
        is_streaming,
        model_name,
        effective_ctx,
        context,
        metrics,
        sampling,
        connection,
        upstream_health,
        calibration,
        calibration_session_id,
        cache_metrics,
        repair_enabled,
        sampling_audit,
    } = req;

    debug!("Forwarding to {upstream_url}, streaming={is_streaming}");

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
    //
    // When a session id is available, the ratio is frozen per-session rather
    // than read live: the live EWMA updates after every request, so reading
    // it fresh on every turn let the budget wobble turn-to-turn purely from
    // calibration noise — which could flip whether the earliest eligible
    // message gets elided below, breaking llama.cpp's common-prefix cache
    // match for a conversation that didn't actually change. See
    // `TokenCalibration::session_chars_per_token`.
    let chars_per_token = calibration_session_id.map_or_else(
        || calibration.chars_per_token(model_name),
        |sid| calibration.session_chars_per_token(model_name, sid, std::time::Instant::now()),
    );
    let budget_chars = Some((effective_ctx as f64 * chars_per_token) as usize);

    let shaped = match shape_request_body(body, &context, &sampling, budget_chars) {
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
                grammar_enforced: false,
                dialect_residue: false,
                tool_repaired: false,
                loop_guard_tripped: false,
                seq: 0,
                recorded_at_secs: std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap_or_default()
                    .as_secs(),
            });
            return Ok(context_length_exceeded_response());
        }
    };
    let ShapedRequest {
        body,
        truncation: report,
        grammar_enforced,
        sampling: decision,
        effort_suppressed,
    } = shaped;

    // Hand the readback this request's intent, and count what the client's own
    // sampling cost it. Both are Tier C — see `sampling_audit`. The intent
    // rides the connection guard because that is the object whose lifetime is
    // already exactly "while this request is in flight", which is also exactly
    // when a slot can report `params` for it.
    //
    // The suppression goes to the audit and *not* onto the connection: the
    // connection's copy exists to be matched against a `/slots` observation,
    // and no observation of a suppressed effort will ever arrive (ADR 0007
    // finding 7a). Its home is the store, which is the one surface whose job is
    // to record what gglib did when nothing downstream can corroborate it.
    if let Some(decision) = decision {
        sampling_audit.record_intent(&decision, effort_suppressed.as_ref());
        connection.record_sampling(decision);
    }

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
    if grammar_enforced {
        info!(
            model = model_name,
            "tool-call grammar enforced for this request (decode-time constraint)"
        );
    }
    let snapshot_seq = metrics.record(ContextSnapshot {
        model_name: model_name.to_owned(),
        payload_chars_before: report.payload_chars_before,
        payload_chars_after: report.payload_chars_after,
        messages_truncated: report.messages_truncated,
        was_clamped: false,
        grammar_enforced,
        dialect_residue: false,
        tool_repaired: false,
        loop_guard_tripped: false,
        seq: 0,
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
        let dialect = context.dialect;

        return Ok(crate::sse_stream::spawn_and_return(
            req_builder,
            body,
            tx,
            rx,
            connection,
            model_name_owned,
            dialect,
            upstream_health,
            calibration,
            cache_metrics,
            Arc::clone(&metrics),
            snapshot_seq,
            forwarded_chars,
            permit,
            config,
            session_id,
            repair_enabled,
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

    // Non-streaming: read the full response and run it through the same
    // dialect normalization the streaming path applies.
    Ok(forward_non_streaming_response(
        response,
        &cache_metrics,
        context.dialect.as_ref(),
        Some((metrics.as_ref(), snapshot_seq)),
    )
    .await)
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
/// Everything a streaming turn needs to re-issue itself as a tool-call repair.
///
/// Carried into the stream because the decision cannot be made until the call
/// is complete, and by then only this function knows what was emitted. The
/// re-issue itself is non-streaming — see [`crate::repair`].
pub(crate) struct RepairContext {
    /// Cloneable builder for the same upstream endpoint and headers.
    pub req_builder: reqwest::RequestBuilder,
    /// The request body as forwarded upstream, which the repair derives from.
    pub request_body: Bytes,
    /// Whether repair is enabled at all.
    pub enabled: bool,
}

pub(crate) async fn stream_response_to_channel(
    response: reqwest::Response,
    model_name: String,
    dialect: Option<DialectSpec>,
    tx: tokio::sync::mpsc::Sender<Result<Bytes, std::io::Error>>,
    connection: &ConnectionGuard,
    repair: Option<RepairContext>,
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

    let parser = get_parser(dialect.as_ref());
    let normalized = NormalizingStream::new(Box::pin(event_stream), parser);
    let mut normalized = Box::pin(normalized);

    // Drift alarm: watch the post-normalization client-visible text for
    // dialect markup that survived. Observation only — never alters what
    // the client receives. Error-recovery text is deliberately excluded
    // (its ⚠-prefixed notice already flags itself), as is reasoning.
    let mut residue = gglib_core::normalize::ResidueScanner::new(dialect.as_ref());

    let mut outcome = StreamOutcome::default();
    let mut client_connected = true;
    // Accumulates reasoning text for the promotion path below. Bounded in
    // practice by the request's `max_tokens`.
    let mut reasoning_buf = String::new();

    // Tool-call hold-back. Frames are encoded as they arrive but withheld
    // until the call is complete, because a repair decision cannot be made
    // before then and bytes already sent cannot be recalled. Text and
    // reasoning stream normally throughout — a tool call is the only part of
    // a turn no client can consume incrementally, so it is the only part
    // where withholding costs nothing.
    let repairing = repair.is_some();
    let mut held_tool_frames: Vec<Bytes> = Vec::new();
    let mut tool_calls = crate::repair::ToolCallAccumulator::default();
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
                    outcome.normalization_errored = true;
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
                LlmStreamEvent::TextDelta { content } => {
                    connection.mark_generating();
                    outcome.saw_visible_output = true;
                    residue.feed(content);
                    encoder.encode(&ev).map(Bytes::from)
                }
                LlmStreamEvent::ToolCallDelta {
                    index,
                    id,
                    name,
                    arguments,
                } if repairing => {
                    connection.mark_generating();
                    outcome.saw_visible_output = true;
                    tool_calls.push(*index, id.as_deref(), name.as_deref(), arguments.as_deref());
                    if let Some(frame) = encoder.encode(&ev) {
                        held_tool_frames.push(Bytes::from(frame));
                    }
                    // Withheld, not dropped — flushed or discarded at `Done`.
                    None
                }
                LlmStreamEvent::ToolCallDelta { .. } => {
                    connection.mark_generating();
                    outcome.saw_visible_output = true;
                    encoder.encode(&ev).map(Bytes::from)
                }
                LlmStreamEvent::UpstreamError { .. } => {
                    connection.mark_generating();
                    // Renderable, so the turn does not also need the empty-turn
                    // notice — but the opposite of healthy, so the watchdog
                    // hears about it separately. Folding these two facts into
                    // one flag is what let a server dying on every request
                    // report itself as producing output.
                    outcome.saw_visible_output = true;
                    outcome.upstream_errored = true;
                    // This event is terminal — the decoder marks the turn done,
                    // so the `Done` arm below never runs and the hold-back's
                    // contents would go out with it. Release them first, ahead
                    // of an error frame a client may read as the end of the turn.
                    if !emit_held_frames(&tx, &mut held_tool_frames).await {
                        client_connected = false;
                        outcome.client_aborted = true;
                    }
                    encoder.encode(&ev).map(Bytes::from)
                }
                LlmStreamEvent::Done { finish_reason } => {
                    connection.mark_generating();
                    outcome.finish_reason = finish_reason.clone();

                    // The turn's tool calls are complete exactly here, so this
                    // is the only point at which a repair can be judged. The
                    // `Done` frame is emitted after whichever set of tool-call
                    // frames wins, never before — a client that sees
                    // `finish_reason` first considers the turn over.
                    if !held_tool_frames.is_empty() {
                        let mut flush = resolve_held_tool_calls(
                            repair.as_ref(),
                            &tool_calls,
                            std::mem::take(&mut held_tool_frames),
                            &encoder,
                            &mut outcome,
                            &tx,
                        )
                        .await;
                        if !emit_held_frames(&tx, &mut flush).await {
                            client_connected = false;
                            outcome.client_aborted = true;
                        }
                    }
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
                // The upstream byte stream broke mid-generation (a crashed
                // llama-server, a severed connection) — the same
                // turn-died-upstream fact as an explicit error event.
                outcome.upstream_errored = true;
                // The inner stream returns without a terminating `Done`, so the
                // flush in that arm never happens. Whatever the hold-back
                // captured is real model output a non-repairing turn would have
                // shown; emit it ahead of the error frame rather than lose it.
                if !emit_held_frames(&tx, &mut held_tool_frames).await {
                    client_connected = false;
                    outcome.client_aborted = true;
                }
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
            // Client disconnected; stop draining the upstream. Recorded so the
            // watchdog can abstain rather than score a person's hang-up as an
            // upstream defect.
            client_connected = false;
            outcome.client_aborted = true;
            break;
        }
    }

    // Backstop for any exit that never reached `Done` — a stream that simply
    // stopped, or a terminal condition whose own arm did not flush. The
    // hold-back exists so a repair can *replace* a tool call; it must never
    // delete one, so anything still held goes out verbatim. Withholding is
    // engaged whenever `repair.is_some()`, including when repair is disabled,
    // so without this a turn that repair would have left alone loses content.
    if client_connected && !held_tool_frames.is_empty() {
        warn!(
            frames = held_tool_frames.len(),
            "proxy: stream ended without a terminating Done; releasing held tool-call frames"
        );
        if !emit_held_frames(&tx, &mut held_tool_frames).await {
            client_connected = false;
            outcome.client_aborted = true;
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

    outcome.dialect_residue = residue.hit().map(ToOwned::to_owned);
    outcome
}

/// Issue the repair request, pushing SSE comments while it is in flight.
///
/// The whole point is that the client is receiving *nothing* during this
/// window: its tool-call frames are withheld pending the outcome, the
/// slot-wait keepalive has already stopped, and the drain loop that called us
/// has no timer. A comment every [`REPAIR_KEEPALIVE_INTERVAL`] keeps the
/// connection observably alive without showing the client anything.
///
/// Returns `None` if the client disconnected mid-flight — there is then no
/// one left to repair for, and the caller falls open to the original frames.
async fn send_reissue_keeping_the_wire_warm(
    builder: reqwest::RequestBuilder,
    tx: &tokio::sync::mpsc::Sender<Result<Bytes, std::io::Error>>,
) -> Option<reqwest::Result<reqwest::Response>> {
    let send = builder.timeout(REPAIR_REISSUE_TIMEOUT).send();
    tokio::pin!(send);

    let mut ticker = tokio::time::interval(REPAIR_KEEPALIVE_INTERVAL);
    // The first tick resolves immediately; consume it so the first comment is
    // one interval away rather than instant.
    ticker.tick().await;

    loop {
        tokio::select! {
            result = &mut send => return Some(result),
            _ = ticker.tick() => {
                if tx.send(Ok(Bytes::from_static(b":\n\n"))).await.is_err() {
                    return None;
                }
            }
        }
    }
}

/// Send withheld tool-call frames to the client verbatim, in order.
///
/// Empties `held`, so a later flush cannot emit the same frames twice —
/// Copilot executes tool calls, and a duplicate is a duplicated side effect.
///
/// Returns `false` if the client went away mid-flush.
async fn emit_held_frames(
    tx: &tokio::sync::mpsc::Sender<Result<Bytes, std::io::Error>>,
    held: &mut Vec<Bytes>,
) -> bool {
    for frame in std::mem::take(held) {
        if tx.send(Ok(frame)).await.is_err() {
            return false;
        }
    }
    true
}

/// Decide what to send in place of the withheld tool-call frames.
///
/// Returns the frames to emit, in order, before this turn's `Done`. On every
/// path that does not produce a strictly better call it returns the originals,
/// so a repair can slow a turn down but can never degrade it — the same
/// fail-open rule truncation and the loop guard follow.
async fn resolve_held_tool_calls(
    repair: Option<&RepairContext>,
    tool_calls: &crate::repair::ToolCallAccumulator,
    original_frames: Vec<Bytes>,
    encoder: &SseEncoder,
    outcome: &mut StreamOutcome,
    tx: &tokio::sync::mpsc::Sender<Result<Bytes, std::io::Error>>,
) -> Vec<Bytes> {
    let Some(ctx) = repair else {
        return original_frames;
    };

    // The validator reads a non-streaming response shape, so the assembled
    // deltas are wrapped to look like one.
    let assembled = serde_json::json!({
        "choices": [{"message": {"tool_calls": tool_calls.to_tool_calls()}}]
    });
    let Ok(assembled_bytes) = serde_json::to_vec(&assembled) else {
        return original_frames;
    };

    let decision = crate::repair::decide(&ctx.request_body, &assembled_bytes, ctx.enabled);
    // A call the validator could not judge at all leaves repair with no
    // opinion to act on. Recorded before the early return, because a client
    // whose tools all use `anyOf` gets zero repair coverage and, without
    // this, zero evidence that the repair rate beside it is measuring a much
    // smaller slice of traffic than it appears to.
    if matches!(
        decision,
        crate::repair::Decision::Forward(crate::repair::Skipped::Unvalidatable)
    ) {
        outcome.unvalidatable_schema = true;
    }
    let crate::repair::Decision::Reissue { body, violations } = decision else {
        return original_frames;
    };

    warn!(
        violations = ?violations,
        "tool call does not match the advertised schema; re-issuing with tool_choice=required"
    );
    outcome.repair_attempted = true;

    let Some(builder) = ctx.req_builder.try_clone() else {
        return original_frames;
    };
    let sent = send_reissue_keeping_the_wire_warm(builder.body(body), tx).await;
    let repaired = match sent {
        Some(Ok(resp)) if resp.status().is_success() => resp.bytes().await.ok(),
        Some(Ok(resp)) => {
            warn!(
                status = resp.status().as_u16(),
                "repair re-issue rejected upstream"
            );
            None
        }
        Some(Err(e)) => {
            warn!(error = %e, "repair re-issue failed");
            None
        }
        // The client went away while we were re-issuing on its behalf.
        None => return original_frames,
    };
    let Some(repaired) = repaired else {
        return original_frames;
    };

    // `choose` re-validates: a repair that is still wrong is discarded.
    let (chosen, did_repair) = crate::repair::choose(
        &ctx.request_body,
        Bytes::from(assembled_bytes),
        repaired.clone(),
    );
    if !did_repair || chosen != repaired {
        return original_frames;
    }

    let events = crate::repair::synthesize_tool_call_events(&repaired);
    if events.is_empty() {
        return original_frames;
    }

    outcome.repair_succeeded = true;
    events
        .iter()
        .filter_map(|ev| encoder.encode(ev).map(Bytes::from))
        .collect()
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

/// Forward a non-streaming JSON response from llama-server, running the same
/// dialect normalization the streaming path applies.
///
/// `residue_sink` — the metrics store and this request's snapshot sequence
/// number — receives the dialect drift-alarm flag when post-normalization
/// content still carries dialect markup. `None` for paths that record no
/// snapshot (embeddings).
pub(crate) async fn forward_non_streaming_response(
    response: reqwest::Response,
    cache_metrics: &CacheMetricsStore,
    dialect: Option<&DialectSpec>,
    residue_sink: Option<(&ContextMetricsStore, u64)>,
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
            let (body_bytes, residue) = normalize_non_streaming_body(body_bytes, dialect);
            if let (Some(marker), Some((metrics, seq))) = (residue, residue_sink) {
                warn!(
                    marker = %marker,
                    "dialect residue reached client-visible output (non-streaming)"
                );
                metrics.flag_dialect_residue(seq);
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

/// Run dialect normalization over a buffered non-streaming body.
///
/// The same parser the streaming path uses, driven once over the complete
/// content (`gglib_core::normalize::normalize_chat_completion_body`), so a
/// `stream: false` client gets structured `tool_calls` instead of raw
/// dialect markup. Parse failures forward the original bytes verbatim — a
/// body we cannot read is a body we must not rewrite. Normalization errors
/// get the same treatment as on the streaming path: logged, and the raw
/// markup surfaced as visibly-flagged assistant text rather than silently
/// dropped.
///
/// The second return value is the drift alarm's one-shot scan of each
/// choice's post-normalization content: the first dialect marker that
/// survived into client-visible text, if any.
fn normalize_non_streaming_body(
    body_bytes: Bytes,
    dialect: Option<&DialectSpec>,
) -> (Bytes, Option<String>) {
    let Ok(mut parsed) = serde_json::from_slice::<serde_json::Value>(&body_bytes) else {
        return (body_bytes, None);
    };

    let errors = gglib_core::normalize::normalize_chat_completion_body(&mut parsed, dialect);

    for err in &errors {
        warn!(?err, "normalization error in non-streaming response");
    }
    if !errors.is_empty()
        && let Some(content) = parsed
            .get_mut("choices")
            .and_then(|c| c.get_mut(0))
            .and_then(|c| c.get_mut("message"))
            .and_then(|m| m.get_mut("content"))
    {
        let mut text = content.as_str().unwrap_or_default().to_owned();
        for err in &errors {
            text.push_str(NORMALIZATION_NOTICE_PREFIX);
            text.push_str(&err.raw);
        }
        *content = serde_json::Value::String(text);
    }

    // Drift alarm: one-shot scan of each choice's post-normalization
    // content. Skipped when normalization errors were surfaced — their
    // recovery notice embeds the raw markup and already flags itself.
    let residue = if errors.is_empty() {
        parsed
            .get("choices")
            .and_then(serde_json::Value::as_array)
            .and_then(|choices| {
                choices.iter().find_map(|choice| {
                    choice
                        .get("message")
                        .and_then(|m| m.get("content"))
                        .and_then(serde_json::Value::as_str)
                        .and_then(|text| gglib_core::normalize::scan_complete(text, dialect))
                })
            })
    } else {
        None
    };

    (
        serde_json::to_vec(&parsed).map_or(body_bytes, Bytes::from),
        residue,
    )
}

#[cfg(test)]
#[path = "forward_tests.rs"]
mod forward_tests;
