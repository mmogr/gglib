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
//! let adapter = LlmCompletionAdapter::new(9000, None::<String>);
//! let agent   = AgentLoop::build(Arc::new(adapter), tool_executor, None);
//! ```

use std::pin::Pin;

use anyhow::{Result, anyhow};

/// Timeout (seconds) for the `.send()` phase of each LLM request.
///
/// Covers TCP connect + TLS handshake + HTTP response headers.  Does **not**
/// apply to the streaming body, which can take arbitrarily long during prompt
/// pre-fill.  Chosen conservatively; the llama-server is always local.
const LLM_CONNECT_TIMEOUT_SECS: u64 = 30;
use async_trait::async_trait;
use futures_core::Stream;
use futures_util::StreamExt as _;
use reqwest::Client;
use serde_json::{Value, json};

use gglib_core::{
    domain::agent::{AgentMessage, AssistantContent, LlmStreamEvent, ToolCall, ToolDefinition},
    ports::LlmCompletionPort,
};

mod sse_decoder;
mod sse_parser;
use sse_decoder::SseStreamDecoder;

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
    /// Forwarded verbatim as the `model` field in the OpenAI request body.
    ///
    /// llama-server ignores this field when serving a single model.  Set it
    /// when the server is serving multiple GGUF files by name (e.g. via
    /// `--model-alias`) or when routing through a proxy that selects backends
    /// by model name.
    model: String,
    client: Client,
}

impl LlmCompletionAdapter {
    /// Create a new adapter targeting `http://127.0.0.1:{port}/v1/chat/completions`.
    ///
    /// `model` is forwarded verbatim in the OpenAI `model` field.  Pass `None`
    /// to send an empty string, which is the right default for llama-server
    /// when it is serving a single model.
    ///
    /// Allocates a fresh [`reqwest::Client`] — prefer [`with_client`](Self::with_client)
    /// when a shared client is available (e.g. from `AppState`) to avoid
    /// per-request connection-pool overhead.
    #[must_use]
    pub fn new(port: u16, model: Option<String>) -> Self {
        Self::with_client(port, Client::new(), model)
    }

    /// Create an adapter that reuses an existing [`reqwest::Client`].
    ///
    /// `model` is forwarded verbatim in the OpenAI `model` field.  Pass `None`
    /// to send an empty string (the default for llama-server in single-model
    /// mode).  Pass a name when the server is routing by `--model-alias`.
    ///
    /// Pass a clone of the application-level client (e.g. `state.http_client.clone()`)
    /// so all agent-chat requests share a single connection pool.
    #[must_use]
    pub fn with_client(port: u16, client: Client, model: Option<String>) -> Self {
        Self {
            url: format!("http://127.0.0.1:{port}/v1/chat/completions"),
            model: model.unwrap_or_default(),
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
        AgentMessage::Assistant { content } => {
            match content {
                AssistantContent::Content(text) => json!({
                    "role": "assistant",
                    "content": text,
                }),
                AssistantContent::ToolCalls(tcs) => {
                    let calls: Vec<Value> = tcs.iter().map(tool_call_to_openai).collect();
                    json!({
                        "role": "assistant",
                        "content": serde_json::Value::Null,
                        "tool_calls": calls,
                    })
                }
                AssistantContent::Both(text, tcs) => {
                    let calls: Vec<Value> = tcs.iter().map(tool_call_to_openai).collect();
                    json!({
                        "role": "assistant",
                        "content": text,
                        "tool_calls": calls,
                    })
                }
            }
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

        let mut body = json!({
            "model": self.model,
            "messages": openai_messages,
            "stream": true,
        });
        if !openai_tools.is_empty() {
            body["tools"] = json!(openai_tools);
            body["tool_choice"] = json!("auto");
        }

        // Gate the connect + first-byte phase with a hard timeout so a
        // stalled or slow llama-server doesn’t hang the agent task
        // indefinitely.  The timeout covers only `.send()` (connect through
        // HTTP response headers); subsequent streaming body reads are not
        // gated here because prompt pre-fill can be arbitrarily long.
        let response = tokio::time::timeout(
            std::time::Duration::from_secs(LLM_CONNECT_TIMEOUT_SECS),
            self.client
                .post(&self.url)
                .header("Content-Type", "application/json")
                .json(&body)
                .send(),
        )
        .await
        .map_err(|_| anyhow!("llama-server connection timed out after {LLM_CONNECT_TIMEOUT_SECS}s"))?
        .map_err(|e| anyhow!("request to llama-server failed: {e}"))?;

        if !response.status().is_success() {
            let status = response.status();
            let text = response
                .text()
                .await
                .unwrap_or_else(|e| format!("<body read error: {e}>"));
            return Err(anyhow!("llama-server returned {status}: {text}"));
        }

        let byte_stream = response.bytes_stream();

        // Build the typed event stream from the raw SSE byte stream.
        let stream = async_stream::stream! {
            let mut decoder = SseStreamDecoder::new();
            let mut byte_stream = std::pin::pin!(byte_stream);

            'outer: while let Some(chunk_result) = byte_stream.next().await {
                let chunk = match chunk_result {
                    Ok(c) => c,
                    Err(e) => {
                        yield Err(anyhow!("SSE byte-stream error: {e}"));
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

        Ok(Box::pin(stream))
    }
}
