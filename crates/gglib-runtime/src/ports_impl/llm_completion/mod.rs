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
    /// Sent as `Authorization: Bearer …` on every request, when set.
    ///
    /// `None` for a llama-server on loopback, which asks for nothing. Set for
    /// the remote tunnel's loopback port (ADR 0012), which is the far
    /// machine's proxy and demands its key; the listener there does not
    /// inject one, so this side has to. Never logged and absent from every
    /// `Debug` the adapter takes part in — the struct derives none.
    bearer: Option<String>,
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
mod builder;

impl LlmCompletionAdapter {
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
            self.bearer.as_deref(),
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
