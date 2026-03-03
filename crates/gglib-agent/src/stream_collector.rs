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
/// loop rejects oversized batches before execution via
/// [`AgentError::ParallelToolLimitExceeded`].
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
/// - A tool-call index ≥ [`MAX_TOOL_CALL_INDEX`] is rejected immediately.
///   This guard bounds the `partials` Vec that grows during streaming; without
///   it a malformed stream could allocate unbounded memory before `Done` arrives.
///   Tool-call *concurrency* is a separate concern — the caller (agent loop)
///   enforces [`AgentConfig::max_parallel_tools`] after this function returns.
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
    // Tracks whether at least one event was received before the stream ended.
    // Used to distinguish a hard connectivity failure (zero events) from a
    // mid-response truncation (some events, no Done frame).
    let mut got_any_event = false;

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

            LlmStreamEvent::ToolCallDelta {
                index,
                id,
                name,
                arguments,
            } => {
                // Guard against a pathological index that would cause a huge allocation.
                if index >= MAX_TOOL_CALL_INDEX {
                    anyhow::bail!(
                        "tool-call index {index} exceeds hard limit ({MAX_TOOL_CALL_INDEX})"
                    );
                }
                // Ensure the partials vec has a slot at `index`.
                if partials.len() <= index {
                    partials.resize_with(index + 1, PartialToolCall::default);
                }
                let p = &mut partials[index];
                if let Some(id) = id {
                    p.id = Some(id);
                }
                if let Some(name) = name {
                    p.name = Some(name);
                }
                if let Some(args) = arguments {
                    p.arguments.push_str(&args);
                }
            }

            LlmStreamEvent::Done { finish_reason } => {
                // Assemble the partial tool calls into domain ToolCall values.
                // Slots where `id` or `name` never arrived are skipped — an
                // absent id would produce an unmatchable ToolResult.
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
                            let message = format!(
                                "tool '{name}' (id: {id}) has malformed JSON arguments: {e}"
                            );
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

                return Ok(CollectedResponse {
                    content: text_buf,
                    reasoning_content: reasoning_buf,
                    tool_calls,
                    finish_reason,
                });
            }
        }
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
    match (id, name) {
        (None, None) => "id and name",
        (None, Some(_)) => "id",
        (Some(_), None) => "name",
        (Some(_), Some(_)) => unreachable!("called with both fields present"),
    }
}

// Tests live in tests/unit_stream_collector.rs so they can follow the same
// external-test pattern used by the rest of the crate.
