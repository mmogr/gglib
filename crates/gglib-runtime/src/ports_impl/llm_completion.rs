//! Concrete [`LlmCompletionPort`] adapter for a local llama-server instance.
//!
//! Translates domain [`AgentMessage`] / [`ToolDefinition`] values into the
//! OpenAI-compatible JSON wire format, POSTs to
//! `http://127.0.0.1:{port}/v1/chat/completions` with `"stream": true`, and
//! maps the response SSE frames back to [`LlmStreamEvent`] values.
//!
//! # Lifetime
//!
//! Create one adapter per agent invocation (construction is cheap — one
//! `reqwest::Client` allocation) and pass it as `Arc<dyn LlmCompletionPort>`
//! to [`gglib_agent::AgentLoop::new`].
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
    #[must_use]
    pub fn new(port: u16) -> Self {
        Self {
            url: format!("http://127.0.0.1:{port}/v1/chat/completions"),
            client: Client::new(),
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

                    // `[DONE]` is the stream terminator; llama-server sends it
                    // after the final data frame.
                    if data == "[DONE]" {
                        if !done_sent {
                            debug!("LLM stream ended with [DONE] but no prior finish_reason — emitting fallback Done");
                            yield Ok(LlmStreamEvent::Done { finish_reason: "stop".to_owned() });
                        }
                        break 'outer;
                    }

                    let parsed: Value = match serde_json::from_str(data) {
                        Ok(v) => v,
                        Err(e) => {
                            yield Err(anyhow!("SSE frame JSON parse error: {e} — data: {data}"));
                            break 'outer;
                        }
                    };

                    let choice = &parsed["choices"][0];
                    let delta = &choice["delta"];

                    // ── Text content delta ─────────────────────────────────
                    if let Some(content) = delta["content"].as_str()
                        && !content.is_empty()
                    {
                        yield Ok(LlmStreamEvent::TextDelta { content: content.to_owned() });
                    }

                    // ── Tool-call deltas ───────────────────────────────────
                    if let Some(tool_calls) = delta["tool_calls"].as_array() {
                        for tc in tool_calls {
                            let index = tc["index"].as_u64().unwrap_or(0) as usize;
                            let id        = tc["id"].as_str().map(str::to_owned);
                            let name      = tc["function"]["name"].as_str().map(str::to_owned);
                            let arguments = tc["function"]["arguments"].as_str().map(str::to_owned);
                            yield Ok(LlmStreamEvent::ToolCallDelta { index, id, name, arguments });
                        }
                    }

                    // ── Finish reason → Done ───────────────────────────────
                    // Emitted before the [DONE] sentinel; we emit `Done` here
                    // so the stream collector receives it before the stream
                    // ends, then simply skip the redundant [DONE].
                    if let Some(finish_reason) = choice["finish_reason"].as_str()
                        && !finish_reason.is_empty()
                    {
                        yield Ok(LlmStreamEvent::Done { finish_reason: finish_reason.to_owned() });
                        done_sent = true;
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
