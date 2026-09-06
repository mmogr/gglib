//! Constructing an [`LlmCompletionAdapter`]: the two constructors and the
//! `with_*` builders.
//!
//! Split from `mod.rs`, unchanged, when the bearer builder arrived and that
//! file was at its budget. The struct and its request path stay there; this
//! is only how one is assembled.

use std::sync::Arc;
use std::sync::atomic::AtomicBool;

use gglib_core::domain::InferenceConfig;
use gglib_core::ports::{RetryObserver, UsageSink};
use gglib_core::request_pipeline::ModelContext;
use gglib_core::retry::RetryPolicy;
use reqwest::Client;

use super::{DEFAULT_SEND_TIMEOUT_SECS, LlmCompletionAdapter};

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
            bearer: None,
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

    /// Send `Authorization: Bearer <token>` on every request.
    ///
    /// For an upstream that demands a key — the remote tunnel's loopback
    /// port, which is another machine's proxy. `None` (the default) sends
    /// no header, which is right for a llama-server on loopback.
    #[must_use]
    pub fn with_bearer(mut self, bearer: Option<String>) -> Self {
        self.bearer = bearer.filter(|b| !b.trim().is_empty());
        self
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
}
