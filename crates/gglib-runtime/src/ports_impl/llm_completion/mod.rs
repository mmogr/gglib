#![doc = include_str!("README.md")]
use std::pin::Pin;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use anyhow::{Result, anyhow};
use async_trait::async_trait;
use futures_core::Stream;
use reqwest::Client;

use gglib_core::{
    domain::InferenceConfig,
    domain::agent::{AgentMessage, LlmStreamEvent, ToolDefinition},
    ports::{LlmCompletionPort, RetryObserver, UsageSink},
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
    /// [`request_pipeline::apply()`], which resolves the layers beneath them.
    sampling: Option<InferenceConfig>,
    /// Timeout (seconds) for the `.send()` phase (connect through response
    /// headers).  Defaults to [`DEFAULT_SEND_TIMEOUT_SECS`].
    send_timeout_secs: u64,
    /// The resolved per-model facts, from
    /// [`gglib_core::request_pipeline::resolve()`].  Drives request shaping
    /// (capabilities, inference defaults) and response-parser selection
    /// (`format:*` tags).  [`ModelContext::passthrough`] — the default —
    /// makes every transform a no-op and selects the identity parser.
    model_context: ModelContext,
    /// Optional destination for this request's token-usage figures.
    ///
    /// When set, the completed response's trailing `usage` is recorded into
    /// this sink — the single point that covers every agent-path consumer of
    /// the stream, and the only one that still reports when the agent loop
    /// aborts mid-run. `None` (the default) means nowhere to report, so
    /// recording is skipped: the case for CLI `gglib chat`/`q`, which run in a
    /// process with no dashboard.
    usage_sink: Option<Arc<dyn UsageSink>>,
    /// Bounds on retrying a transient upstream failure — see
    /// [`retry`] for why that is safe to do here.
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
    /// Skip the request-shaping pipeline entirely and send the bare body.
    ///
    /// The control arm of an A/B evaluation: no sampling resolution (the
    /// upstream's own defaults apply), no capability shaping, no truncation,
    /// no grammar. Off everywhere else — this exists so "bare llama-server"
    /// is measurable against "through the gglib pipeline" on identical
    /// requests, not as a general escape hatch.
    raw_passthrough: bool,
    /// Optional `tool_choice` written into the **first** request body of a
    /// run, and only the first.
    ///
    /// The agent path has no client to send one; benchmark harnesses set
    /// `"required"` on tasks whose expected outcome demands a call, so the
    /// opening request carries the same demand an agentic client would
    /// express.
    ///
    /// It is deliberately not repeated. A model forced to emit a tool call on
    /// *every* turn can never produce a final answer, so it re-emits its last
    /// batch until the loop guard aborts the run — which measures the harness,
    /// not the model. Later turns keep [`body::build_chat_body`]'s `"auto"`
    /// default, which is what an agentic client sends once it has its first
    /// tool result.
    first_turn_tool_choice: Option<String>,
    /// Consumed by the first [`Self::shaped_body`] call.
    ///
    /// One adapter is built per benchmark task, so this is per-run state, not
    /// global state. `shaped_body` runs once per turn — outside the retry loop
    /// — so a retried request cannot spend it early.
    first_turn_pending: AtomicBool,
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
            usage_sink: None,
            retry_policy: RetryPolicy::from_env(),
            retry_observer: None,
            raw_passthrough: false,
            first_turn_tool_choice: None,
            first_turn_pending: AtomicBool::new(true),
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

    /// Set the resolved per-model context, from
    /// [`gglib_core::request_pipeline::resolve()`].
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

    /// Report each response's token usage to `sink`.
    ///
    /// Pass the process's agent-path [`CacheMetricsStore`] when the caller runs
    /// in the proxy process (GUI chat) so prompt-cache reuse lands on the
    /// dashboard alongside the proxied figure; pass a benchmark tally when the
    /// caller needs the generated-token count to survive a guard-aborted run.
    /// Pass `None` (the default) when there is nothing to report to — recording
    /// then costs nothing.
    ///
    /// [`CacheMetricsStore`]: gglib_core::cache_metrics::CacheMetricsStore
    #[must_use]
    pub fn with_usage_sink(mut self, sink: Option<Arc<dyn UsageSink>>) -> Self {
        self.usage_sink = sink;
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

    /// Bypass the request-shaping pipeline and send the bare body.
    ///
    /// The A/B evaluation's control arm — see the field docs on
    /// [`Self`]. Leave off (the default) for every production path.
    #[must_use]
    pub fn with_raw_passthrough(mut self, raw: bool) -> Self {
        self.raw_passthrough = raw;
        self
    }

    /// Write a `tool_choice` value into this run's **first** request body.
    ///
    /// Set before the pipeline runs, so the shaping stages (including the
    /// dialect grammar stage) read it exactly as they would a client's.
    /// Subsequent turns fall back to `build_chat_body`'s `"auto"`, letting the
    /// model finish — see [`Self::first_turn_tool_choice`] for why repeating
    /// the demand measures the harness rather than the model.
    #[must_use]
    pub fn with_first_turn_tool_choice(mut self, tool_choice: Option<String>) -> Self {
        self.first_turn_tool_choice = tool_choice;
        self
    }

    /// Build the request body and run the shared request-shaping pipeline over
    /// it — everything that happens before the bytes leave this process.
    ///
    /// Separate from [`chat_stream`](LlmCompletionPort::chat_stream) so the
    /// outgoing body can be asserted on directly, without an HTTP round trip.
    ///
    /// Deliberately **not idempotent**: the first call consumes
    /// [`Self::first_turn_pending`], so a second call on the same adapter
    /// omits `first_turn_tool_choice`. That is the contract — one adapter
    /// serves one run, and the demand belongs to its opening turn.
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
    ) -> Result<serde_json::Value> {
        let mut body = body::build_chat_body(&self.model, messages, tools, self.sampling.as_ref());

        // Written before the pipeline runs so the shaping stages read it
        // exactly as they would an external client's tool_choice — and only on
        // the first turn, so the model can still finish. See the
        // `first_turn_tool_choice` field docs.
        if let Some(tool_choice) = &self.first_turn_tool_choice
            && self.first_turn_pending.swap(false, Ordering::Relaxed)
        {
            body["tool_choice"] = serde_json::Value::String(tool_choice.clone());
        }

        // Control arm of an A/B evaluation: the bare body, upstream defaults,
        // no shaping. See the `raw_passthrough` field docs.
        if self.raw_passthrough {
            return Ok(body);
        }

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
        // `trust_client_sampling: true` unconditionally: `Settings.trust_client_sampling`
        // gates an *external* client's request body against a boilerplate value it
        // may have no user-facing control over (VS Code Copilot's hardcoded
        // `temperature: 0`, for one). `self.sampling` is not that — it is gglib's own
        // typed caller config (CLI flags, the agent loop's resolved settings), built by
        // trusted in-process code, so it must always resolve as the top layer
        // regardless of that setting.
        //
        // The truncation budget comes from the model itself. There is no live
        // serving context to measure here and no learned chars-per-token ratio
        // — those belong to the proxy, which observes usage frames — so an
        // unknown model yields no budget and the stage is skipped.
        let report = request_pipeline::apply(
            &mut body,
            &self.model_context,
            &SamplingLayers {
                trust_client_sampling: true,
                // Unconditionally on: there is no settings snapshot in
                // process, and this path is the agent loop, whose turns with
                // tools are exactly what the ceiling exists for. The
                // `GGLIB_DISABLE_AGENTIC_SAMPLING` env switch still reaches it.
                agentic_adjustments: true,
                ..Default::default()
            },
            self.model_context.context_budget_chars(),
        )
        .map_err(|e| anyhow!("conversation exceeds the model's context budget: {e}"))?;

        if report.truncation.messages_truncated > 0 {
            tracing::info!(
                messages_truncated = report.truncation.messages_truncated,
                payload_chars_before = report.truncation.payload_chars_before,
                payload_chars_after = report.truncation.payload_chars_after,
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
    ) -> Result<Pin<Box<dyn Stream<Item = Result<LlmStreamEvent>> + Send>>> {
        // Shaped once, outside the retry loop: the pipeline runs truncation and
        // logs what it trimmed, and neither should repeat per attempt. The body
        // is deterministic, so every attempt sends identical bytes.
        let body = self.shaped_body(messages, tools)?;

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
            self.model_context.dialect.as_ref(),
            self.usage_sink.clone(),
        ))
    }
}

#[cfg(test)]
#[path = "shaping_tests.rs"]
mod shaping_tests;
