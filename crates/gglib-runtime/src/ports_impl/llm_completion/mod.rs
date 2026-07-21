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
    request_pipeline::{self, ModelContext, SamplingLayers},
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
    /// The caller's own sampling parameters — the top layer of the hierarchy,
    /// equivalent to what an external client sends the proxy. Written into the
    /// body by [`body::build_chat_body`] and read back out by
    /// [`request_pipeline::apply`], which resolves the layers beneath them.
    sampling: Option<InferenceConfig>,
    /// Timeout (seconds) for the `.send()` phase (connect through response
    /// headers).  Defaults to [`DEFAULT_SEND_TIMEOUT_SECS`].
    send_timeout_secs: u64,
    /// The resolved per-model facts, from
    /// [`gglib_core::request_pipeline::resolve`].  Drives request shaping
    /// (capabilities, inference defaults) and response-parser selection
    /// (`format:*` tags).  [`ModelContext::passthrough`] — the default —
    /// makes every transform a no-op and selects the identity parser.
    model_context: ModelContext,
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
            model_context: ModelContext::passthrough(),
        }
    }

    /// Set the caller's own sampling parameters.
    ///
    /// These are the *highest* layer of the hierarchy, not the final word: the
    /// model's stored defaults and the hardcoded fallbacks still fill in every
    /// field left unset. Pass `None` to resolve entirely from those layers.
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

    /// Set the resolved per-model context, from
    /// [`gglib_core::request_pipeline::resolve`].
    ///
    /// This is what gives the in-process agent path the same per-model handling
    /// the proxy has always had: capability-aware message coalescing, the
    /// per-model layer of the sampling hierarchy, and a dialect-specific
    /// response parser. Pass [`ModelContext::passthrough`] (the default) when
    /// the model is unknown — every transform becomes a no-op and the identity
    /// parser is selected, which is the right choice for any model that already
    /// speaks strict `OpenAI` tool-calling.
    #[must_use]
    pub fn with_model_context(mut self, model_context: ModelContext) -> Self {
        self.model_context = model_context;
        self
    }

    /// Build the request body and run the shared request-shaping pipeline over
    /// it — everything that happens before the bytes leave this process.
    ///
    /// Separate from [`chat_stream`](LlmCompletionPort::chat_stream) so the
    /// outgoing body can be asserted on directly, without an HTTP round trip.
    ///
    /// # Errors
    ///
    /// When the conversation cannot be made to fit the model's context budget.
    /// Failing here is the point: the alternative is sending a prompt that is
    /// already known to overflow and reading the failure back out of
    /// llama-server, with worse diagnostics and a wasted pre-fill.
    fn shaped_body(
        &self,
        messages: &[AgentMessage],
        tools: &[ToolDefinition],
        response_format: Option<&ResponseFormat>,
    ) -> Result<serde_json::Value> {
        let mut body = body::build_chat_body(
            &self.model,
            messages,
            tools,
            self.sampling.as_ref(),
            response_format,
        );

        // The same pipeline, in the same order, that the proxy runs.
        // `build_chat_body` has already written the caller's sampling
        // parameters into the body, which is exactly where an external client's
        // would be, so `apply` reads them back as the top layer and resolves
        // the model and hardcoded layers beneath them.
        //
        // Neither remaining layer applies in-process: there is no
        // `{model}:{profile}` suffix to select a profile, and the global
        // settings layer is already folded into `sampling` by the callers that
        // have one.
        //
        // The truncation budget comes from the model itself. There is no live
        // serving context to measure here and no learned chars-per-token ratio
        // — those belong to the proxy, which observes usage frames — so an
        // unknown model yields no budget and the stage is skipped.
        let report = request_pipeline::apply(
            &mut body,
            &self.model_context,
            &SamplingLayers::default(),
            self.model_context.context_budget_chars(),
        )
        .map_err(|e| anyhow!("conversation exceeds the model's context budget: {e}"))?;

        if report.messages_truncated > 0 {
            tracing::info!(
                messages_truncated = report.messages_truncated,
                payload_chars_before = report.payload_chars_before,
                payload_chars_after = report.payload_chars_after,
                "history truncated: reduced payload before sending upstream"
            );
        }

        Ok(body)
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
        let body = self.shaped_body(messages, tools, response_format)?;

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
        let parser = get_parser(&self.model_context.tags);
        let normalized = NormalizingStream::new(Box::pin(stream), parser);
        Ok(Box::pin(normalized))
    }
}

#[cfg(test)]
#[path = "shaping_tests.rs"]
mod shaping_tests;
