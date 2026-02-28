//! Concrete [`LlmCompletionPort`] adapter for a local llama-server instance.
//!
//! Translates domain [`AgentMessage`] / [`ToolDefinition`] values into the
//! OpenAI-compatible JSON wire format, POSTs to
//! `http://127.0.0.1:{port}/v1/chat/completions` with `"stream": true`, and
//! maps the response SSE frames back to [`LlmStreamEvent`] values.
//!
//! # Lifetime
//!
//! Prefer constructing one adapter **per request** via
//! [`LlmCompletionAdapter::with_client`] and passing a clone of the
//! application-level `reqwest::Client` (stored in `AppState`) so all requests
//! share a single connection pool.  The `new` constructor is still available
//! for standalone use (e.g. CLI) and allocates its own pool.
//!
//! ```ignore
//! let adapter = LlmCompletionAdapter::new(9000);
//! let agent   = AgentLoop::new(Arc::new(adapter), tool_executor);
//! ```

use std::pin::Pin;

use anyhow::{Result, anyhow};
use async_trait::async_trait;
use futures_core::Stream;
use futures_util::StreamExt as _;
use reqwest::Client;
use serde_json::{Value, json};
use tracing::debug;

use gglib_core::{
    domain::agent::{AgentMessage, LlmStreamEvent, ToolCall, ToolDefinition},
    ports::LlmCompletionPort,
};

// =============================================================================
// Adapter struct
// =============================================================================

/// Drives a llama-server instance via its OpenAI-compatible streaming API.
///
/// Implements [`LlmCompletionPort`] so the pure-domain `gglib-agent` crate can
/// call an LLM without knowing anything about HTTP, SSE framing, or the
/// OpenAI wire format.
pub struct LlmCompletionAdapter {
    url: String,
    client: Client,
}

impl LlmCompletionAdapter {
    /// Create a new adapter targeting `http://127.0.0.1:{port}/v1/chat/completions`.
    ///
    /// Allocates a fresh [`reqwest::Client`] — prefer [`with_client`](Self::with_client)
    /// when a shared client is available (e.g. from `AppState`) to avoid
    /// per-request connection-pool overhead.
    #[must_use]
    pub fn new(port: u16) -> Self {
        Self::with_client(port, Client::new())
    }

    /// Create an adapter that reuses an existing [`reqwest::Client`].
    ///
    /// Pass a clone of the application-level client (e.g. `state.http_client.clone()`)
    /// so all agent-chat requests share a single connection pool.
    #[must_use]
    pub fn with_client(port: u16, client: Client) -> Self {
        Self {
            url: format!("http://127.0.0.1:{port}/v1/chat/completions"),
            client,
        }
    }
}

// =============================================================================
// Wire-format helpers
// =============================================================================

/// Map a domain [`AgentMessage`] to the OpenAI `messages` array element.
fn message_to_openai(msg: &AgentMessage) -> Value {
    match msg {
        AgentMessage::System { content } => {
            json!({ "role": "system", "content": content })
        }
        AgentMessage::User { content } => {
            json!({ "role": "user", "content": content })
        }
        AgentMessage::Assistant {
            content,
            tool_calls,
        } => {
            // Null content is valid when the model only requests tool calls.
            let calls = tool_calls
                .as_deref()
                .map(|tcs| tcs.iter().map(tool_call_to_openai).collect::<Vec<_>>());
            json!({
                "role": "assistant",
                "content": content,
                "tool_calls": calls,
            })
        }
        AgentMessage::Tool {
            tool_call_id,
            content,
        } => {
            json!({ "role": "tool", "tool_call_id": tool_call_id, "content": content })
        }
    }
}

/// Map a domain [`ToolCall`] to the OpenAI `tool_calls` array element.
///
/// The OpenAI API requires `arguments` to be a **JSON string**, not an object.
fn tool_call_to_openai(tc: &ToolCall) -> Value {
    json!({
        "id": tc.id,
        "type": "function",
        "function": {
            "name": tc.name,
            // arguments must be a JSON *string* per OpenAI spec
            "arguments": tc.arguments.to_string(),
        },
    })
}

/// Map a domain [`ToolDefinition`] to the OpenAI `tools` array element.
fn tool_def_to_openai(def: &ToolDefinition) -> Value {
    let parameters = def
        .input_schema
        .clone()
        .unwrap_or_else(|| json!({ "type": "object", "properties": {} }));

    json!({
        "type": "function",
        "function": {
            "name": def.name,
            "description": def.description,
            "parameters": parameters,
        },
    })
}

// =============================================================================
// SSE frame parser (extracted for unit-testability)
// =============================================================================

/// Result of parsing a single SSE `data:` payload.
#[derive(Debug)]
pub(crate) enum SseParseResult {
    /// The value `[DONE]` — stream terminator, no events.
    Done,
    /// One or more events decoded from the JSON frame.
    Events(Vec<LlmStreamEvent>),
}

/// Parse a single SSE `data:` payload into zero or more [`LlmStreamEvent`]s.
///
/// Returns:
/// - `Ok(SseParseResult::Done)` when `data == "[DONE]"`
/// - `Ok(SseParseResult::Events(…))` for a valid JSON frame (may be empty
///   when the frame carries no content or tool-call deltas)
/// - `Err(…)` when the frame is not valid JSON
pub(crate) fn parse_sse_frame(data: &str) -> Result<SseParseResult> {
    if data == "[DONE]" {
        return Ok(SseParseResult::Done);
    }

    let parsed: Value = serde_json::from_str(data)
        .map_err(|e| anyhow!("SSE frame JSON parse error: {e} — data: {data}"))?;

    let choice = &parsed["choices"][0];
    let delta = &choice["delta"];

    let mut events: Vec<LlmStreamEvent> = Vec::new();

    // ── Text content delta ──────────────────────────────────────────────────
    if let Some(content) = delta["content"].as_str()
        && !content.is_empty()
    {
        events.push(LlmStreamEvent::TextDelta {
            content: content.to_owned(),
        });
    }

    // ── Tool-call deltas ────────────────────────────────────────────────────
    if let Some(tool_calls) = delta["tool_calls"].as_array() {
        for tc in tool_calls {
            let index = tc["index"].as_u64().unwrap_or(0) as usize;
            let id = tc["id"].as_str().map(str::to_owned);
            let name = tc["function"]["name"].as_str().map(str::to_owned);
            let arguments = tc["function"]["arguments"].as_str().map(str::to_owned);
            events.push(LlmStreamEvent::ToolCallDelta {
                index,
                id,
                name,
                arguments,
            });
        }
    }

    // ── Finish reason → Done ────────────────────────────────────────────────
    if let Some(finish_reason) = choice["finish_reason"].as_str()
        && !finish_reason.is_empty()
    {
        events.push(LlmStreamEvent::Done {
            finish_reason: finish_reason.to_owned(),
        });
    }

    Ok(SseParseResult::Events(events))
}

// =============================================================================
// LlmCompletionPort implementation
// =============================================================================

#[async_trait]
impl LlmCompletionPort for LlmCompletionAdapter {
    async fn chat_stream(
        &self,
        messages: &[AgentMessage],
        tools: &[ToolDefinition],
    ) -> Result<Pin<Box<dyn Stream<Item = Result<LlmStreamEvent>> + Send>>> {
        let openai_messages: Vec<Value> = messages.iter().map(message_to_openai).collect();
        let openai_tools: Vec<Value> = tools.iter().map(tool_def_to_openai).collect();

        // llama-server ignores the `model` field when serving a single model.
        let mut body = json!({
            "model": "",
            "messages": openai_messages,
            "stream": true,
        });
        if !openai_tools.is_empty() {
            body["tools"] = json!(openai_tools);
            body["tool_choice"] = json!("auto");
        }

        let response = self
            .client
            .post(&self.url)
            .header("Content-Type", "application/json")
            .json(&body)
            .send()
            .await
            .map_err(|e| anyhow!("request to llama-server failed: {e}"))?;

        if !response.status().is_success() {
            let status = response.status();
            let text = response.text().await.unwrap_or_default();
            return Err(anyhow!("llama-server returned {status}: {text}"));
        }

        let byte_stream = response.bytes_stream();

        // Build the typed event stream from the raw SSE byte stream.
        let stream = async_stream::stream! {
            let mut byte_stream = std::pin::pin!(byte_stream);
            let mut buf = String::new();
            // Set once we emit `Done` so we don't emit it twice if the server
            // sends both a `finish_reason` frame and a `[DONE]` sentinel.
            let mut done_sent = false;

            'outer: while let Some(chunk_result) = byte_stream.next().await {
                let chunk = match chunk_result {
                    Ok(c) => c,
                    Err(e) => {
                        yield Err(anyhow!("SSE byte-stream error: {e}"));
                        return;
                    }
                };

                buf.push_str(&String::from_utf8_lossy(&chunk));

                // Drain complete lines from the buffer.
                loop {
                    let Some(newline_pos) = buf.find('\n') else { break };
                    let line = buf[..newline_pos].trim_end_matches('\r').to_owned();
                    buf.drain(..=newline_pos);

                    // Skip comments and blank lines.
                    let Some(data) = line.strip_prefix("data: ") else { continue };

                    match parse_sse_frame(data) {
                        Ok(SseParseResult::Done) => {
                            if !done_sent {
                                debug!("LLM stream ended with [DONE] but no prior finish_reason — emitting fallback Done");
                                yield Ok(LlmStreamEvent::Done { finish_reason: "stop".to_owned() });
                            }
                            break 'outer;
                        }
                        Ok(SseParseResult::Events(events)) => {
                            for event in events {
                                if matches!(event, LlmStreamEvent::Done { .. }) {
                                    done_sent = true;
                                }
                                yield Ok(event);
                            }
                        }
                        Err(e) => {
                            yield Err(e);
                            break 'outer;
                        }
                    }
                }
            }

            // Guard: stream must always end with exactly one Done.
            if !done_sent {
                debug!("LLM byte-stream ended without [DONE] sentinel — emitting fallback Done");
                yield Ok(LlmStreamEvent::Done { finish_reason: "stop".to_owned() });
            }
        };

        Ok(Box::pin(stream))
    }
}

// =============================================================================
// Tests
// =============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    fn text_frame(content: &str) -> String {
        serde_json::json!({
            "choices": [{ "delta": { "content": content }, "finish_reason": null }]
        })
        .to_string()
    }

    fn finish_frame(reason: &str) -> String {
        serde_json::json!({
            "choices": [{ "delta": {}, "finish_reason": reason }]
        })
        .to_string()
    }

    fn tool_frame(index: usize, id: &str, name: &str, args: &str) -> String {
        serde_json::json!({
            "choices": [{
                "delta": {
                    "tool_calls": [{
                        "index": index,
                        "id": id,
                        "function": { "name": name, "arguments": args }
                    }]
                },
                "finish_reason": null
            }]
        })
        .to_string()
    }

    #[test]
    fn done_sentinel_returns_done_variant() {
        assert!(matches!(
            parse_sse_frame("[DONE]"),
            Ok(SseParseResult::Done)
        ));
    }

    #[test]
    fn text_delta_frame_produces_text_event() {
        let events = match parse_sse_frame(&text_frame("hello")) {
            Ok(SseParseResult::Events(e)) => e,
            other => panic!("unexpected: {other:?}"),
        };
        assert_eq!(events.len(), 1);
        assert!(matches!(
            &events[0],
            LlmStreamEvent::TextDelta { content } if content == "hello"
        ));
    }

    #[test]
    fn empty_content_produces_no_text_event() {
        let frame = serde_json::json!({
            "choices": [{ "delta": { "content": "" }, "finish_reason": null }]
        })
        .to_string();
        let events = match parse_sse_frame(&frame) {
            Ok(SseParseResult::Events(e)) => e,
            other => panic!("unexpected: {other:?}"),
        };
        assert!(
            events.is_empty(),
            "empty content should not produce TextDelta"
        );
    }

    #[test]
    fn finish_reason_produces_done_event() {
        let events = match parse_sse_frame(&finish_frame("stop")) {
            Ok(SseParseResult::Events(e)) => e,
            other => panic!("unexpected: {other:?}"),
        };
        assert_eq!(events.len(), 1);
        assert!(matches!(
            &events[0],
            LlmStreamEvent::Done { finish_reason } if finish_reason == "stop"
        ));
    }

    #[test]
    fn tool_call_delta_frame_is_parsed() {
        let events = match parse_sse_frame(&tool_frame(0, "tc1", "search", r#"{"q":"rust"}"#)) {
            Ok(SseParseResult::Events(e)) => e,
            other => panic!("unexpected: {other:?}"),
        };
        assert_eq!(events.len(), 1);
        assert!(matches!(
            &events[0],
            LlmStreamEvent::ToolCallDelta { index: 0, id: Some(id), name: Some(n), arguments: Some(a) }
            if id == "tc1" && n == "search" && a == r#"{"q":"rust"}"#
        ));
    }

    #[test]
    fn malformed_json_returns_error() {
        let result = parse_sse_frame("{ broken json }");
        assert!(result.is_err(), "malformed JSON should return Err");
    }

    #[test]
    fn frame_with_text_and_finish_reason_produces_both_events() {
        let frame = serde_json::json!({
            "choices": [{ "delta": { "content": "hi" }, "finish_reason": "stop" }]
        })
        .to_string();
        let events = match parse_sse_frame(&frame) {
            Ok(SseParseResult::Events(e)) => e,
            other => panic!("unexpected: {other:?}"),
        };
        assert_eq!(events.len(), 2);
        assert!(matches!(&events[0], LlmStreamEvent::TextDelta { .. }));
        assert!(matches!(&events[1], LlmStreamEvent::Done { .. }));
    }
}
