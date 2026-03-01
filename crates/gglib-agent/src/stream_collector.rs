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

// =============================================================================
// Constants
// =============================================================================

/// Hard upper bound on the tool-call slot index accepted during streaming.
///
/// This is a DoS guard: if an LLM emits an absurdly large `index` value the
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
/// | Streaming slot index DoS protection | `MAX_TOOL_CALL_INDEX` (this constant, checked inside `collect_stream`) |
/// | Runtime concurrency cap for tool execution | [`AgentConfig::max_parallel_tools`] (checked by the agent loop *after* `collect_stream` returns) |
///
/// Setting `max_parallel_tools` to a value smaller than `MAX_TOOL_CALL_INDEX`
/// does **not** prevent a model from emitting more tool-call slots in the
/// stream — it only limits how many are executed concurrently.  The agent
/// loop rejects oversized batches before execution via
/// [`AgentError::ParallelToolLimitExceeded`].
pub(crate) const MAX_TOOL_CALL_INDEX: usize = 64;

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
                            let _ = tx.send(AgentEvent::Error { message: message.clone() }).await;
                            anyhow::bail!("{message}");
                        }
                    };
                    let raw = p.arguments;
                    let args_str = if raw.is_empty() { "{}" } else { &raw };
                    let arguments = match serde_json::from_str::<serde_json::Value>(args_str) {
                        Ok(v) => v,
                        Err(e) => {
                            let message = format!(
                                "tool '{}' (id: {}) has malformed JSON arguments: {e}",
                                name, id
                            );
                            warn!(
                                tool_name = %name,
                                raw_args = %args_str,
                                error = %e,
                                "tool-call arguments are not valid JSON"
                            );
                            let _ = tx.send(AgentEvent::Error { message: message.clone() }).await;
                            anyhow::bail!("{message}");
                        }
                    };
                    tool_calls.push(ToolCall { id, name, arguments });
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
    } else {
        anyhow::bail!("LLM stream yielded zero events (connection refused or server unreachable)")
    }
}

// =============================================================================
// Private helpers
// =============================================================================

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

// =============================================================================
// Tests
// =============================================================================

#[cfg(test)]
mod tests {
    use futures_util::stream;
    use gglib_core::LlmStreamEvent;
    use tokio::sync::mpsc;

    use super::*;

    fn make_stream(
        events: Vec<LlmStreamEvent>,
    ) -> Pin<Box<dyn futures_core::Stream<Item = Result<LlmStreamEvent>> + Send>> {
        Box::pin(stream::iter(events.into_iter().map(Ok)))
    }

    #[tokio::test]
    async fn text_delta_forwarded_and_accumulated() {
        let (tx, mut rx) = mpsc::channel(16);
        let stream = make_stream(vec![
            LlmStreamEvent::TextDelta {
                content: "Hello, ".into(),
            },
            LlmStreamEvent::TextDelta {
                content: "world!".into(),
            },
            LlmStreamEvent::Done {
                finish_reason: "stop".into(),
            },
        ]);

        let response = collect_stream(stream, &tx).await.unwrap();
        assert_eq!(response.content, "Hello, world!");
        assert_eq!(response.finish_reason, "stop");
        assert!(response.tool_calls.is_empty());

        // Both text deltas should have been forwarded to the channel.
        let evt1 = rx.recv().await.unwrap();
        let evt2 = rx.recv().await.unwrap();
        assert!(matches!(evt1, AgentEvent::TextDelta { content } if content == "Hello, "));
        assert!(matches!(evt2, AgentEvent::TextDelta { content } if content == "world!"));
    }

    #[tokio::test]
    async fn reasoning_delta_forwarded_and_accumulated_separately() {
        let (tx, mut rx) = mpsc::channel(16);
        let stream = make_stream(vec![
            LlmStreamEvent::ReasoningDelta {
                content: "Let me think".into(),
            },
            LlmStreamEvent::ReasoningDelta {
                content: " about this.".into(),
            },
            LlmStreamEvent::TextDelta {
                content: "Answer.".into(),
            },
            LlmStreamEvent::Done {
                finish_reason: "stop".into(),
            },
        ]);

        let response = collect_stream(stream, &tx).await.unwrap();
        // Reasoning content is accumulated separately and NOT mixed into content.
        assert_eq!(response.reasoning_content, "Let me think about this.");
        assert_eq!(response.content, "Answer.");
        assert!(response.tool_calls.is_empty());

        // Both reasoning deltas and the text delta are forwarded live.
        let evt1 = rx.recv().await.unwrap();
        let evt2 = rx.recv().await.unwrap();
        let evt3 = rx.recv().await.unwrap();
        assert!(
            matches!(evt1, AgentEvent::ReasoningDelta { content } if content == "Let me think")
        );
        assert!(
            matches!(evt2, AgentEvent::ReasoningDelta { content } if content == " about this.")
        );
        assert!(matches!(evt3, AgentEvent::TextDelta { content } if content == "Answer."));
    }

    #[tokio::test]
    async fn tool_call_deltas_assembled_into_tool_calls() {
        let (tx, _rx) = mpsc::channel(16);
        let stream = make_stream(vec![
            LlmStreamEvent::ToolCallDelta {
                index: 0,
                id: Some("call_1".into()),
                name: Some("fs_read".into()),
                arguments: Some("{\"pat".into()),
            },
            LlmStreamEvent::ToolCallDelta {
                index: 0,
                id: None,
                name: None,
                arguments: Some("h\": \"/tmp\"}".into()),
            },
            LlmStreamEvent::Done {
                finish_reason: "tool_calls".into(),
            },
        ]);

        let response = collect_stream(stream, &tx).await.unwrap();
        assert_eq!(response.tool_calls.len(), 1);
        let tc = &response.tool_calls[0];
        assert_eq!(tc.id, "call_1");
        assert_eq!(tc.name, "fs_read");
        assert_eq!(tc.arguments["path"], "/tmp");
    }

    #[tokio::test]
    async fn multiple_tool_calls_assembled_by_index() {
        let (tx, _rx) = mpsc::channel(16);
        let stream = make_stream(vec![
            LlmStreamEvent::ToolCallDelta {
                index: 0,
                id: Some("c0".into()),
                name: Some("tool_a".into()),
                arguments: Some("{}".into()),
            },
            LlmStreamEvent::ToolCallDelta {
                index: 1,
                id: Some("c1".into()),
                name: Some("tool_b".into()),
                arguments: Some("{}".into()),
            },
            LlmStreamEvent::Done {
                finish_reason: "tool_calls".into(),
            },
        ]);

        let response = collect_stream(stream, &tx).await.unwrap();
        assert_eq!(response.tool_calls.len(), 2);
        assert_eq!(response.tool_calls[0].name, "tool_a");
        assert_eq!(response.tool_calls[1].name, "tool_b");
    }

    #[tokio::test]
    async fn missing_done_event_returns_error() {
        let (tx, _rx) = mpsc::channel(16);
        let stream = make_stream(vec![
            LlmStreamEvent::TextDelta {
                content: "partial".into(),
            },
            // No Done event
        ]);
        assert!(collect_stream(stream, &tx).await.is_err());
    }

    #[tokio::test]
    async fn malformed_json_arguments_emit_error_and_fail() {
        // Malformed JSON must hard-fail with a visible AgentEvent::Error so the
        // SSE client always sees why the stream was terminated.
        let (tx, mut rx) = mpsc::channel(16);
        let stream = make_stream(vec![
            LlmStreamEvent::ToolCallDelta {
                index: 0,
                id: Some("c1".into()),
                name: Some("do_thing".into()),
                arguments: Some("{ NOT VALID JSON ".into()),
            },
            LlmStreamEvent::Done {
                finish_reason: "tool_calls".into(),
            },
        ]);

        // collect_stream must return Err on malformed JSON.
        assert!(collect_stream(stream, &tx).await.is_err());

        // An AgentEvent::Error must have been sent before the bail.
        drop(tx); // close sender so try_recv can drain
        let events: Vec<_> = std::iter::from_fn(|| rx.try_recv().ok()).collect();
        assert!(
            events.iter().any(|e| matches!(e, AgentEvent::Error { .. })),
            "AgentEvent::Error must be emitted for malformed JSON arguments"
        );
    }

    #[tokio::test]
    async fn oversized_tool_call_index_returns_error() {
        // An index >= MAX_TOOL_CALL_INDEX must be rejected immediately to
        // prevent unbounded Vec growth from a malformed or adversarial stream.
        let (tx, _rx) = mpsc::channel(16);
        let stream = make_stream(vec![
            LlmStreamEvent::ToolCallDelta {
                index: MAX_TOOL_CALL_INDEX, // at or beyond the hard limit → rejected
                id: Some("c0".into()),
                name: Some("do_thing".into()),
                arguments: Some("{}".into()),
            },
            LlmStreamEvent::Done {
                finish_reason: "tool_calls".into(),
            },
        ]);

        assert!(collect_stream(stream, &tx).await.is_err());
    }
}
