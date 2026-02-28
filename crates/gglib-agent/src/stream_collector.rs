//! Streaming LLM response collector.
//!
//! Consumes a [`LlmCompletionPort`] stream, forwarding text deltas to the
//! caller's [`AgentEvent`] channel **as they arrive** and accumulating
//! incremental tool-call deltas in memory until the stream terminates.
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
    /// Tool calls requested by the model (empty when the model answered directly).
    pub tool_calls: Vec<ToolCall>,
    /// The `finish_reason` from the [`LlmStreamEvent::Done`] terminus event.
    pub finish_reason: String,
}

// =============================================================================
// Partial tool-call accumulator
// =============================================================================

/// Mutable accumulator for a single tool-call that arrives in fragments.
#[derive(Default)]
struct PartialToolCall {
    id: String,
    name: String,
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
/// - A tool-call index ≥ `max_parallel_tools` is rejected immediately to
///   prevent a malicious or buggy LLM from triggering an unbounded allocation.
/// - Malformed tool-call arguments (not valid JSON) are silently replaced with
///   an empty object and a `warn` log entry rather than hard-failing the loop.
pub async fn collect_stream(
    mut stream: Pin<Box<dyn futures_core::Stream<Item = Result<LlmStreamEvent>> + Send>>,
    tx: &mpsc::Sender<AgentEvent>,
    max_parallel_tools: usize,
) -> Result<CollectedResponse> {
    let mut text_buf = String::new();
    // Indexed by the tool-call `index` from the stream deltas.
    let mut partials: Vec<PartialToolCall> = Vec::new();

    while let Some(event) = stream.next().await {
        match event? {
            LlmStreamEvent::TextDelta { content } => {
                text_buf.push_str(&content);
                // Forward immediately; ignore send errors (client may have disconnected).
                let _ = tx.send(AgentEvent::TextDelta { content }).await;
            }

            LlmStreamEvent::ToolCallDelta {
                index,
                id,
                name,
                arguments,
            } => {
                // Guard against a pathological index that would cause a huge allocation.
                if index >= max_parallel_tools {
                    anyhow::bail!(
                        "tool-call index {index} exceeds max_parallel_tools ({max_parallel_tools})"
                    );
                }
                // Ensure the partials vec has a slot at `index`.
                if partials.len() <= index {
                    partials.resize_with(index + 1, PartialToolCall::default);
                }
                let p = &mut partials[index];
                if let Some(id) = id {
                    p.id = id;
                }
                if let Some(name) = name {
                    p.name = name;
                }
                if let Some(args) = arguments {
                    p.arguments.push_str(&args);
                }
            }

            LlmStreamEvent::Done { finish_reason } => {
                // Assemble the partial tool calls into domain ToolCall values.
                let tool_calls = partials
                    .into_iter()
                    .enumerate()
                    .filter(|(_, p)| !p.name.is_empty()) // skip empty slots
                    .map(|(_, p)| {
                        let args_str = if p.arguments.is_empty() {
                            "{}"
                        } else {
                            &p.arguments
                        };
                        let arguments: serde_json::Value = serde_json::from_str(args_str)
                            .unwrap_or_else(|e| {
                                warn!(
                                    tool_name = %p.name,
                                    raw_args = %args_str,
                                    error = %e,
                                    "tool-call arguments are not valid JSON; using empty object"
                                );
                                serde_json::Value::Object(serde_json::Map::default())
                            });
                        ToolCall {
                            id: p.id,
                            name: p.name,
                            arguments,
                        }
                    })
                    .collect();

                return Ok(CollectedResponse {
                    content: text_buf,
                    tool_calls,
                    finish_reason,
                });
            }
        }
    }

    // The stream ended without a Done event — treat as an infrastructure error.
    anyhow::bail!("LLM stream ended without a Done event")
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

        let response = collect_stream(stream, &tx, 8).await.unwrap();
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

        let response = collect_stream(stream, &tx, 8).await.unwrap();
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

        let response = collect_stream(stream, &tx, 8).await.unwrap();
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
        assert!(collect_stream(stream, &tx, 8).await.is_err());
    }
}
