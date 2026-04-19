//! Concrete [`LlmCompletionPort`] adapter for a llama-server instance.
//!
//! Translates domain [`AgentMessage`] / [`ToolDefinition`] values into the
//! OpenAI-compatible JSON wire format, POSTs to
//! `{base_url}/v1/chat/completions` with `"stream": true`, and maps the
//! response SSE frames back to [`LlmStreamEvent`] values.
//!
//! The `base_url` is the server root without a trailing path component,
//! e.g. `"http://127.0.0.1:9000"`.  This allows the adapter to target any
//! reachable host (Docker networks, remote servers, CI environments).
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
//! let adapter = LlmCompletionAdapter::new("http://127.0.0.1:9000", None::<String>);
//! let agent   = AgentLoop::build(Arc::new(adapter), tool_executor, None);
//! ```

use std::pin::Pin;

use anyhow::{Result, anyhow};
use async_trait::async_trait;
use futures_core::Stream;
use futures_util::StreamExt as _;
use reqwest::Client;
use serde_json::{Value, json};

use gglib_core::{
    domain::InferenceConfig,
    domain::agent::{AgentMessage, LlmStreamEvent, ToolCall, ToolDefinition},
    ports::LlmCompletionPort,
};

mod sse_decoder;
mod sse_parser;
use sse_decoder::SseStreamDecoder;

/// Default timeout (seconds) for the `.send()` phase of each LLM request.
///
/// With `return_progress: true` in the request body, llama-server sends HTTP
/// response headers immediately (before prompt pre-fill), so `.send()`
/// completes in well under a second for any reachable server.  This timeout
/// is therefore a **safety net** against a truly unreachable or hung server,
/// not a pre-fill time limit.  The generous value avoids false positives
/// while still bounding resource usage for a dead connection.
const DEFAULT_SEND_TIMEOUT_SECS: u64 = 600;

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
    /// Optional sampling overrides injected into every request body.
    sampling: Option<InferenceConfig>,
    /// Timeout (seconds) for the `.send()` phase (connect through response
    /// headers).  Defaults to [`DEFAULT_SEND_TIMEOUT_SECS`].
    send_timeout_secs: u64,
}

/// Build the completions endpoint URL from a base URL.
///
/// Trims any trailing slash from `base_url` before appending the path so
/// callers do not need to normalise their input.
fn completions_url(base_url: &str) -> String {
    format!("{}/v1/chat/completions", base_url.trim_end_matches('/'))
}

impl LlmCompletionAdapter {
    /// Create a new adapter targeting `{base_url}/v1/chat/completions`.
    ///
    /// `base_url` is the server root without a trailing slash, e.g.
    /// `"http://127.0.0.1:9000"`.  This accepts any reachable host, not just
    /// loopback.
    ///
    /// `model` is forwarded verbatim in the OpenAI `model` field.  Pass `None`
    /// to send an empty string, which is the right default for llama-server
    /// when it is serving a single model.
    ///
    /// Allocates a fresh [`reqwest::Client`] — prefer [`with_client`](Self::with_client)
    /// when a shared client is available (e.g. from `AppState`) to avoid
    /// per-request connection-pool overhead.
    #[must_use]
    pub fn new(base_url: impl Into<String>, model: Option<String>) -> Self {
        Self::with_client(base_url, Client::new(), model)
    }

    /// Create an adapter that reuses an existing [`reqwest::Client`].
    ///
    /// `base_url` is the server root without a trailing slash, e.g.
    /// `"http://127.0.0.1:9000"`.  A trailing slash is tolerated and stripped.
    ///
    /// `model` is forwarded verbatim in the OpenAI `model` field.  Pass `None`
    /// to send an empty string (the default for llama-server in single-model
    /// mode).  Pass a name when the server is routing by `--model-alias`.
    ///
    /// Pass a clone of the application-level client (e.g. `state.http_client.clone()`)
    /// so all agent-chat requests share a single connection pool.
    #[must_use]
    pub fn with_client(base_url: impl Into<String>, client: Client, model: Option<String>) -> Self {
        Self {
            url: completions_url(&base_url.into()),
            model: model.unwrap_or_default(),
            client,
            sampling: None,
            send_timeout_secs: DEFAULT_SEND_TIMEOUT_SECS,
        }
    }

    /// Set optional sampling parameters injected into every request body.
    #[must_use]
    pub fn with_sampling(mut self, sampling: Option<InferenceConfig>) -> Self {
        self.sampling = sampling;
        self
    }

    /// Override the send-phase timeout (connect through first response
    /// headers).  The default is [`DEFAULT_SEND_TIMEOUT_SECS`] (120 s).
    #[must_use]
    pub fn with_send_timeout(mut self, secs: u64) -> Self {
        self.send_timeout_secs = secs;
        self
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
            // When tool_calls are present but text is None, omit the
            // "content" field entirely rather than sending `"content": null`.
            // Some LLM backends do not handle an explicit null well when
            // tool_calls is populated.  When there are no tool_calls and
            // text is None, we still send null to signal an empty reply.
            let has_tool_calls = !content.tool_calls.is_empty();
            let mut obj = if content.text.is_none() && has_tool_calls {
                json!({ "role": "assistant" })
            } else {
                json!({
                    "role": "assistant",
                    "content": content.text.as_deref().map_or(Value::Null, |s| Value::String(s.to_owned())),
                })
            };
            if has_tool_calls {
                let calls: Vec<Value> =
                    content.tool_calls.iter().map(tool_call_to_openai).collect();
                obj["tool_calls"] = Value::Array(calls);
            }
            obj
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
            "return_progress": true,
        });
        if !openai_tools.is_empty() {
            body["tools"] = json!(openai_tools);
            body["tool_choice"] = json!("auto");
        }
        if let Some(ref s) = self.sampling {
            if let Some(t) = s.temperature {
                body["temperature"] = json!(t);
            }
            if let Some(p) = s.top_p {
                body["top_p"] = json!(p);
            }
            if let Some(k) = s.top_k {
                body["top_k"] = json!(k);
            }
            if let Some(m) = s.max_tokens {
                body["max_tokens"] = json!(m);
            }
            if let Some(r) = s.repeat_penalty {
                body["repeat_penalty"] = json!(r);
            }
        }

        // Gate the connect + first-byte phase with a hard timeout so a
        // stalled or unresponsive llama-server doesn't hang the agent task
        // indefinitely.  The timeout covers `.send()` — TCP connect through
        // HTTP response headers — which includes prompt pre-fill because
        // llama-server doesn't send headers until pre-fill finishes.
        let timeout_secs = self.send_timeout_secs;
        let response = tokio::time::timeout(
            std::time::Duration::from_secs(timeout_secs),
            self.client.post(&self.url).json(&body).send(),
        )
        .await
        .map_err(|_| anyhow!("llama-server connection timed out after {timeout_secs}s"))?
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
            let mut decoder = SseStreamDecoder::default();
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
