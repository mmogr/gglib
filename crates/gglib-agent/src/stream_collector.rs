//! Streaming LLM response collector.
//!
//! Consumes a [`LlmCompletionPort`] stream, forwarding text deltas and
//! reasoning deltas to the caller's [`AgentEvent`] channel **as they arrive**
//! and accumulating incremental tool-call deltas in memory until the stream
//! terminates.
//!
//! Reasoning deltas ([`LlmStreamEvent::ReasoningDelta`]) are forwarded live
//! as [`AgentEvent::ReasoningDelta`] and accumulated in a separate buffer; they
//! are never mixed into the `content` field and are not sent back as context.
//!
//! # Why separate from the main loop?
//!
//! This module exists to keep the real-time UX concern (forward text now)
//! isolated from the tool-execution concern (wait for all deltas, then act).
//! The agent loop only sees a clean [`CollectedResponse`] after this function
//! returns — it never touches `LlmStreamEvent` directly.

use std::pin::Pin;

use anyhow::Result;
use futures_util::StreamExt as _;
use gglib_core::normalize::NormalizationErrorKind;
use gglib_core::{AgentEvent, LlmStreamEvent, ToolCall};
use tokio::sync::mpsc;
use tracing::warn;

use crate::util::emit_error_event;

// =============================================================================
// Constants
// =============================================================================

/// Hard upper bound on the tool-call slot index accepted during streaming.
///
/// This is a `DoS` guard: if an LLM emits an absurdly large `index` value the
/// collector would otherwise allocate a huge `partials` Vec before `Done`
/// arrives.  64 simultaneous tool calls is far beyond any realistic scenario;
/// the value is intentionally large enough to never constrain normal usage
/// while still protecting against malformed streams.
///
/// Note the distinction between this constant and
/// [`AgentConfig::max_parallel_tools`]:
///
/// | Concern | Enforced by |
/// |---------|-------------|
/// | Streaming slot index `DoS` protection | `MAX_TOOL_CALL_INDEX` (this constant, checked inside `collect_stream`) |
/// | Runtime concurrency cap for tool execution | [`AgentConfig::max_parallel_tools`] (checked by the agent loop *after* `collect_stream` returns) |
///
/// Setting `max_parallel_tools` to a value smaller than `MAX_TOOL_CALL_INDEX`
/// does **not** prevent a model from emitting more tool-call slots in the
/// stream — it only limits how many are executed concurrently.  The agent
/// loop recovers from an oversized batch by feeding the model a synthetic
/// tool error asking it to retry with a smaller one.
///
/// # Reaching it truncates; it does not fail the turn
///
/// Deltas at or beyond this index are dropped and counted in
/// [`CollectedResponse::tool_calls_truncated`]. That is a deliberate change
/// from the original behaviour, which aborted the whole stream: a model that
/// ran away emitting tool calls produced "internal agent error", zero tokens
/// and zero iterations after minutes of real work, with the failure
/// misattributed to gglib rather than to the model. The allocation this guard
/// exists to bound is a 64-element Vec; destroying the turn to avoid it was
/// the more expensive outcome by a wide margin.
pub const MAX_TOOL_CALL_INDEX: usize = 64;

// =============================================================================
// Output type
// =============================================================================

/// The fully-assembled response from a single LLM call.
///
/// This is what the agent loop receives after
/// [`collect_stream`] has processed the entire stream.
#[derive(Debug)]
pub struct CollectedResponse {
    /// All text content fragments joined into a single string.
    pub content: String,
    /// All reasoning/CoT fragments joined into a single string.
    ///
    /// Empty for models that do not emit `reasoning_content` frames.
    /// Present for informational purposes (logging, CLI rendering); it is
    /// **not** fed back into the conversation history.
    pub reasoning_content: String,
    /// Tool calls requested by the model (empty when the model answered directly).
    pub tool_calls: Vec<ToolCall>,
    /// The `finish_reason` from the [`LlmStreamEvent::Done`] terminus event.
    pub finish_reason: String,
    /// Tool-call deltas dropped because their `index` reached
    /// [`MAX_TOOL_CALL_INDEX`].
    ///
    /// Non-zero means the model ran away emitting tool calls. The response is
    /// still well-formed — the first [`MAX_TOOL_CALL_INDEX`] slots are intact
    /// — so the caller can apply its own policy rather than losing the turn.
    /// See [`collect_stream`]'s notes on why this is a count and not an error.
    pub tool_calls_truncated: usize,
    /// Completion-token count from the stream's trailing
    /// [`LlmStreamEvent::Usage`] event.
    ///
    /// `None` when the upstream never reported usage — absent and zero are
    /// distinct, exactly as on the event itself. Per the `OpenAI` streaming
    /// convention the usage chunk arrives *after* `Done`, which is why the
    /// collector drains the stream to its end instead of returning at the
    /// `Done` event.
    pub completion_tokens: Option<u32>,
}

// =============================================================================
// Partial tool-call accumulator
// =============================================================================

/// Mutable accumulator for a single tool-call that arrives in fragments.
///
/// `id` and `name` are `None` until the first delta for this index arrives
/// (the LLM emits them in the opening delta alongside `index`).  `arguments`
/// accumulates as further deltas arrive and may remain empty for no-arg tools.
#[derive(Default)]
struct PartialToolCall {
    /// Call identifier — `None` until received in the first delta.
    id: Option<String>,
    /// Tool name — `None` until received in the first delta.
    name: Option<String>,
    /// Accumulated JSON string (fragments are concatenated, not parsed yet).
    arguments: String,
}

// =============================================================================
// Collector
// =============================================================================

/// Consume a streaming LLM response, forwarding text live and assembling
/// tool calls.
///
/// # Behaviour
///
/// - [`LlmStreamEvent::TextDelta`] — appends to an internal text buffer and
///   immediately sends [`AgentEvent::TextDelta`] on `tx`.  Send failures are
///   ignored (the receiver may have dropped if the client disconnected).
/// - [`LlmStreamEvent::ToolCallDelta`] — upserts into a `Vec<PartialToolCall>`
///   keyed by `index`, extending the `arguments` string.
/// - [`LlmStreamEvent::Done`] — assembles the partials into [`ToolCall`]s
///   (parsing the accumulated arguments JSON string into `serde_json::Value`).
///   Returns the completed [`CollectedResponse`].
///
/// # Errors
///
/// - Infrastructure errors (an `Err` item in the stream) are returned immediately.
/// - A tool-call index ≥ [`MAX_TOOL_CALL_INDEX`] is **dropped, not fatal** —
///   see [`CollectedResponse::tool_calls_truncated`]. The guard still bounds
///   the `partials` Vec, which is all it was ever for; it no longer destroys
///   the turn to do it. Tool-call *concurrency* remains a separate concern —
///   the caller (agent loop) enforces [`AgentConfig::max_parallel_tools`]
///   after this function returns, and already recovers from an oversized
///   batch by telling the model to retry with a smaller one.
/// - Malformed tool-call arguments (not valid JSON) cause `collect_stream` to
///   emit an [`AgentEvent::Error`] on `tx` and return `Err`. This ensures the
///   SSE client always sees the failure reason before the stream closes.
pub async fn collect_stream(
    mut stream: Pin<Box<dyn futures_core::Stream<Item = Result<LlmStreamEvent>> + Send>>,
    tx: &mpsc::Sender<AgentEvent>,
) -> Result<CollectedResponse> {
    let mut text_buf = String::new();
    let mut reasoning_buf = String::new();
    // Indexed by the tool-call `index` from the stream deltas.
    let mut partials: Vec<PartialToolCall> = Vec::new();
    // Deltas dropped for exceeding MAX_TOOL_CALL_INDEX. Counted rather than
    // fatal — see `upsert_tool_call_delta`.
    let mut tool_calls_truncated: usize = 0;
    // Tracks whether at least one event was received before the stream ended.
    // Used to distinguish a hard connectivity failure (zero events) from a
    // mid-response truncation (some events, no Done frame).
    let mut got_any_event = false;
    // Set at `Done`; the loop keeps draining afterwards because the OpenAI
    // usage chunk legitimately trails `Done` (before the byte stream closes).
    let mut finished: Option<(String, Vec<ToolCall>)> = None;
    let mut completion_tokens: Option<u32> = None;

    while let Some(event) = stream.next().await {
        got_any_event = true;
        match event? {
            LlmStreamEvent::TextDelta { content } => {
                text_buf.push_str(&content);
                // Forward immediately; ignore send errors (client may have disconnected).
                let _ = tx.send(AgentEvent::TextDelta { content }).await;
            }

            LlmStreamEvent::ReasoningDelta { content } => {
                reasoning_buf.push_str(&content);
                // Forward immediately so CoT tokens appear in real time in the UI.
                let _ = tx.send(AgentEvent::ReasoningDelta { content }).await;
            }

            LlmStreamEvent::PromptProgress {
                processed,
                total,
                cached,
                time_ms,
            } => {
                // Forward pre-fill progress so consumers can display it.
                let _ = tx
                    .send(AgentEvent::PromptProgress {
                        processed,
                        total,
                        cached,
                        time_ms,
                    })
                    .await;
            }

            LlmStreamEvent::ToolCallDelta {
                index,
                id,
                name,
                arguments,
            } => {
                if upsert_tool_call_delta(&mut partials, index, id, name, arguments) {
                    tool_calls_truncated += 1;
                }
            }

            LlmStreamEvent::Done { finish_reason } => {
                let tool_calls = assemble_tool_calls(std::mem::take(&mut partials), tx).await?;
                finished = Some((finish_reason, tool_calls));
                // Do not return yet: the trailing usage chunk arrives after
                // Done. The decoder ends the stream right after the [DONE]
                // sentinel, so this drains at most a few trailer events.
            }

            LlmStreamEvent::NormalizationError { kind, raw } => {
                handle_normalization_error(tx, &kind, &raw).await;
            }

            // Wire-facing telemetry; the completion count is kept so callers
            // can compute generation throughput (the tune/eval speed axis).
            LlmStreamEvent::Usage {
                completion_tokens: ct,
                ..
            } => {
                completion_tokens = Some(ct);
            }

            LlmStreamEvent::UpstreamError {
                message,
                error_type,
                code,
            } => return Err(upstream_error(&message, &error_type, &code)),
        }
    }

    if let Some((finish_reason, tool_calls)) = finished {
        return Ok(CollectedResponse {
            content: text_buf,
            reasoning_content: reasoning_buf,
            tool_calls,
            finish_reason,
            tool_calls_truncated,
            completion_tokens,
        });
    }

    // The stream ended without a Done event.  Distinguish two failure modes:
    // - Zero events: hard connectivity failure (server unreachable, refused connection).
    // - Some events, no Done: stream truncated mid-response.
    if got_any_event {
        anyhow::bail!("LLM stream ended without a Done event (stream truncated mid-response)")
    }
    anyhow::bail!("LLM stream yielded zero events (connection refused or server unreachable)")
}

// =============================================================================
// Private helpers
// =============================================================================

/// Fold one `ToolCallDelta` into the partial-call accumulator at `index`.
///
/// Bounds the index against [`MAX_TOOL_CALL_INDEX`] (a malformed stream must
/// not drive a huge allocation), grows the vec on demand, and logs when a
/// delta overwrites an already-seen `id`/`name` — a should-never-happen the
/// old inline code also surfaced.
///
/// # Returns `true` when the delta was dropped
///
/// It used to `bail!`, which lost the whole turn: `collect_stream` returned
/// `Err`, the agent loop reported "internal agent error", and the caller saw
/// zero tokens and zero iterations after two minutes of real work. Measured on
/// Qwen3.5-4B, that fired on 2 of 2 agentic eval runs and scored the affected
/// task 0 — a guard doing more damage than the unbounded allocation it exists
/// to prevent, and misreporting a model behaviour as a gglib fault.
///
/// A runaway tool-call stream already has correct handling one layer up:
/// `agent_loop` compares `tool_calls.len()` against `max_parallel_tools` and
/// recovers by feeding the model a synthetic tool error asking it to retry
/// with a smaller batch. Bailing here preempted that with a worse outcome.
fn upsert_tool_call_delta(
    partials: &mut Vec<PartialToolCall>,
    index: usize,
    id: Option<String>,
    name: Option<String>,
    arguments: Option<String>,
) -> bool {
    if index >= MAX_TOOL_CALL_INDEX {
        return true;
    }
    if partials.len() <= index {
        partials.resize_with(index + 1, PartialToolCall::default);
    }
    let p = &mut partials[index];
    if let Some(id) = id {
        if let Some(ref existing) = p.id {
            warn!(
                tool_index = index,
                existing = %existing,
                new = %id,
                "ToolCallDelta overwrote existing id"
            );
        }
        p.id = Some(id);
    }
    if let Some(name) = name {
        if let Some(ref existing) = p.name {
            warn!(
                tool_index = index,
                existing = %existing,
                new = %name,
                "ToolCallDelta overwrote existing name"
            );
        }
        p.name = Some(name);
    }
    if let Some(args) = arguments {
        p.arguments.push_str(&args);
    }
    false
}

/// Surface a non-fatal `NormalizationError` from the dialect parser.
///
/// The stream is **not** aborted — the parser already swallowed the
/// offending bytes.  We log via `tracing` for operators and emit an
/// `AgentEvent::SystemWarning` so UIs can render a non-blocking notice.
async fn handle_normalization_error(
    tx: &mpsc::Sender<AgentEvent>,
    kind: &NormalizationErrorKind,
    raw: &str,
) {
    warn!(
        ?kind,
        raw = %raw,
        "normalization error from LLM stream parser"
    );
    let _ = tx
        .send(AgentEvent::SystemWarning {
            message: format!("LLM normalization issue: {kind:?}"),
            suggested_action: None,
        })
        .await;
}

/// Build the error returned when an upstream `LlmStreamEvent::UpstreamError`
/// frame arrives mid-stream — a genuine upstream failure (e.g. a
/// context-length overflow discovered mid-generation) rather than a
/// connectivity problem, so it gets a specific message instead of the
/// generic "stream ended without a Done event" bail below.
fn upstream_error(message: &str, error_type: &str, code: &str) -> anyhow::Error {
    anyhow::anyhow!(
        "upstream reported an error mid-stream: {message} (type={error_type}, code={code})"
    )
}

/// Assemble accumulated [`PartialToolCall`]s into domain [`ToolCall`] values.
///
/// Slots where `id` or `name` never arrived are treated as errors — an absent
/// `id` would produce an unmatchable `ToolResult`, and an absent `name` cannot
/// be routed to a tool executor.
///
/// Malformed JSON in `arguments` is also treated as an error: the model
/// violated the protocol and feeding garbage to a tool executor would be
/// worse than aborting early.
///
/// On any error an [`AgentEvent::Error`] is emitted on `tx` before bailing
/// so the SSE client always sees the failure reason.
async fn assemble_tool_calls(
    partials: Vec<PartialToolCall>,
    tx: &mpsc::Sender<AgentEvent>,
) -> Result<Vec<ToolCall>> {
    let mut tool_calls = Vec::with_capacity(partials.len());
    for p in partials {
        let (id, name) = match (p.id, p.name) {
            (Some(id), Some(name)) => (id, name),
            (id, name) => {
                let message = format!(
                    "incomplete tool-call partial at Done: missing {} \
                     (id={:?}, name={:?}) — aborting to prevent incoherent context",
                    missing_fields_desc(id.as_deref(), name.as_deref()),
                    id,
                    name,
                );
                warn!(%message, "aborting stream collection due to incomplete tool-call partial");
                return bail_stream(tx, message).await;
            }
        };
        let raw = p.arguments;
        let args_str = if raw.is_empty() { "{}" } else { &raw };
        let arguments = match serde_json::from_str::<serde_json::Value>(args_str) {
            Ok(v) => v,
            Err(e) => {
                let message = format!("tool '{name}' (id: {id}) has malformed JSON arguments: {e}");
                warn!(
                    tool_name = %name,
                    raw_args = %args_str,
                    error = %e,
                    "tool-call arguments are not valid JSON"
                );
                return bail_stream(tx, message).await;
            }
        };
        tool_calls.push(ToolCall {
            id,
            name,
            arguments,
        });
    }
    Ok(tool_calls)
}

/// Emit an [`AgentEvent::Error`] on `tx` and bail with the same message.
///
/// Mirrors `bail_internal` in the agent loop, but returns `anyhow::Result<T>`
/// rather than `Result<_, AgentError>`. Used to consolidate the repeated
/// "emit error event + bail" pattern in the [`LlmStreamEvent::Done`] assembly
/// code so error handling logic lives in exactly one place.
async fn bail_stream<T>(tx: &mpsc::Sender<AgentEvent>, msg: String) -> Result<T> {
    emit_error_event(tx, &msg).await;
    anyhow::bail!("{msg}")
}

/// Describe which fields of an incomplete tool-call partial are missing.
///
/// Extracted from the `format!` call at the `Done` assembly site to make the
/// three-branch logic independently testable and avoid deep nesting.
fn missing_fields_desc(id: Option<&str>, name: Option<&str>) -> &'static str {
    debug_assert!(
        id.is_none() || name.is_none(),
        "missing_fields_desc called with both fields present"
    );
    match (id, name) {
        (None, None) => "id and name",
        (None, Some(_)) => "id",
        (Some(_), None) => "name",
        (Some(_), Some(_)) => unreachable!("called with both fields present"),
    }
}

// Tests live in tests/unit_stream_collector.rs so they can follow the same
// external-test pattern used by the rest of the crate.
