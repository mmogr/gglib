#![doc = include_str!("README.md")]
use std::pin::Pin;
use std::sync::Arc;

use anyhow::{Result, anyhow};
use async_trait::async_trait;
use futures_core::Stream;
use reqwest::Client;

use gglib_core::{
    domain::InferenceConfig,
    domain::agent::{AgentMessage, LlmStreamEvent, ToolDefinition},
    ports::{CacheMetricsSink, LlmCompletionPort, ResponseFormat, RetryObserver},
    request_pipeline::{self, ModelContext, SamplingLayers},
    retry::RetryPolicy,
};

mod body;
mod retry;
mod stream;

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
    /// Optional destination for this request's prompt-cache reuse figures.
    ///
    /// When set, the completed response's trailing `usage` is recorded into
    /// this sink — the single point that covers every agent-path consumer of
    /// the stream (both `stream_collector` and `structured_output`). `None`
    /// (the default) means nowhere to report, so recording is skipped: the
    /// case for CLI `gglib chat`/`q`, which run in a process with no dashboard.
    cache_metrics: Option<Arc<dyn CacheMetricsSink>>,
    /// Bounds on retrying a transient upstream failure — see
    /// [`retry`](self::retry) for why that is safe to do here.
    ///
    /// The default budget is deliberately modest: the proxy already absorbs
    /// `ModelLoading` server-side, so what reaches this adapter is startup
    /// *contention*, which by definition means something upstream has already
    /// waited a long time.
    retry_policy: RetryPolicy,
    /// Optional destination for this request's retry activity.
    ///
    /// When set, each backoff and the eventual give-up are reported here — the
    /// agent HTTP handler turns them into
    /// [`AgentEvent::SystemWarning`](gglib_core::domain::agent::AgentEvent::SystemWarning)
    /// frames so a waiting user sees "retrying" rather than a frozen cursor.
    /// `None` (the default) means nowhere to report, so the calls are no-ops:
    /// the case for CLI `gglib chat`/`q`, which render the loop's events
    /// directly.
    retry_observer: Option<Arc<dyn RetryObserver>>,
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
            cache_metrics: None,
            retry_policy: RetryPolicy::from_env(),
            retry_observer: None,
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

    /// Report this adapter's prompt-cache reuse to `sink`.
    ///
    /// Pass the process's agent-path [`CacheMetricsStore`] when the caller runs
    /// in the proxy process (council, GUI chat) so reuse lands on the dashboard
    /// alongside the proxied figure. Pass `None` (the default) when there is no
    /// dashboard to report to — recording then costs nothing.
    ///
    /// [`CacheMetricsStore`]: gglib_core::cache_metrics::CacheMetricsStore
    #[must_use]
    pub fn with_cache_metrics_sink(mut self, sink: Option<Arc<dyn CacheMetricsSink>>) -> Self {
        self.cache_metrics = sink;
        self
    }

    /// Override the retry budget for transient upstream failures.
    ///
    /// Pass `RetryPolicy { max_attempts: 1, .. }` to disable retrying — that is
    /// what the CLI's `--no-retry` resolves to.
    #[must_use]
    pub fn with_retry_policy(mut self, policy: RetryPolicy) -> Self {
        self.retry_policy = policy;
        self
    }

    /// Report this adapter's retry activity to `observer`.
    ///
    /// Pass an observer when there is a live stream to notify, so a user
    /// waiting on a contended model is told the request is being retried rather
    /// than watching a stalled cursor. Pass `None` (the default) when there is
    /// nothing to notify — reporting then costs nothing.
    #[must_use]
    pub fn with_retry_observer(mut self, observer: Option<Arc<dyn RetryObserver>>) -> Self {
        self.retry_observer = observer;
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
        // Shaped once, outside the retry loop: the pipeline runs truncation and
        // logs what it trimmed, and neither should repeat per attempt. The body
        // is deterministic, so every attempt sends identical bytes.
        let body = self.shaped_body(messages, tools, response_format)?;

        // Each attempt's connect + first-byte phase is bounded by the send
        // timeout, and the whole sequence by the policy's own deadline, so a
        // stalled llama-server can neither hang the agent task nor multiply the
        // timeout by the attempt count. The timeout covers `.send()` — TCP
        // connect through HTTP response headers — which includes prompt
        // pre-fill because llama-server doesn't send headers until pre-fill
        // finishes.
        //
        // Retrying is safe only because it all happens here, before a single
        // body byte is read: see the `retry` module docs.
        let response = retry::send_with_retry(
            &self.client,
            &self.url,
            &body,
            std::time::Duration::from_secs(self.send_timeout_secs),
            &self.retry_policy,
            self.retry_observer.as_ref(),
        )
        .await?;

        // Decode, normalize, and (when a sink is set) tap prompt-cache usage.
        Ok(stream::normalized_event_stream(
            response,
            &self.model_context.tags,
            self.cache_metrics.clone(),
        ))
    }
}

#[cfg(test)]
#[path = "shaping_tests.rs"]
mod shaping_tests;
