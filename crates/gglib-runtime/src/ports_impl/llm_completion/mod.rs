#![doc = include_str!("README.md")]
use std::pin::Pin;

use anyhow::{Result, anyhow};
use async_trait::async_trait;
use futures_core::Stream;
use futures_util::StreamExt as _;
use reqwest::Client;

use gglib_core::{
    domain::InferenceConfig,
    domain::agent::{AgentMessage, LlmStreamEvent, ToolDefinition},
    normalize::{NormalizingStream, get_parser},
    ports::{LlmCompletionPort, ResponseFormat},
    sse::SseStreamDecoder,
};

mod body;

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
    /// Model `format:*` tags consulted by [`gglib_core::normalize::get_parser`]
    /// when wrapping the SSE-derived stream in a [`NormalizingStream`].
    /// Empty (the default) selects the identity-passthrough parser.
    tags: Vec<String>,
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
            tags: Vec::new(),
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

    /// Set the model's `format:*` tags so the adapter can pick a
    /// dialect-specific parser when wrapping the SSE-derived stream in a
    /// [`NormalizingStream`].
    ///
    /// Pass an empty `Vec` (the default) to select the identity-passthrough
    /// parser, which is the right choice for any model that already speaks
    /// strict OpenAI tool-calling.
    #[must_use]
    pub fn with_tags(mut self, tags: Vec<String>) -> Self {
        self.tags = tags;
        self
    }
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
        response_format: Option<&ResponseFormat>,
    ) -> Result<Pin<Box<dyn Stream<Item = Result<LlmStreamEvent>> + Send>>> {
        let body = body::build_chat_body(
            &self.model,
            messages,
            tools,
            self.sampling.as_ref(),
            response_format,
        );

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

        // Wrap the raw SSE-derived stream in the universal normalization
        // layer.  Empty `tags` selects the identity-passthrough parser so
        // models that already emit strict OpenAI tool calls are unaffected.
        let parser = get_parser(&self.tags);
        let normalized = NormalizingStream::new(Box::pin(stream), parser);
        Ok(Box::pin(normalized))
    }
}
