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
    ports::{AttachmentStore, LlmCompletionPort, RetryObserver, UsageSink},
    request_pipeline::{self, ModelContext, SamplingDecision, SamplingLayers},
    retry::RetryPolicy,
};

mod body;
mod far_machine;
mod images;
mod retry;
mod stream;
mod writing_time;

pub use far_machine::FarMachine;

/// Default timeout (seconds) for the `.send()` phase of each LLM request.
///
/// The body always asks for `return_progress`, so llama-server begins its
/// reply, headers and then a progress frame per prompt batch, once a slot
/// takes the request: `.send()` waits for a slot, not for prefill. This is a
/// **safety net** against an unreachable or wedged server; once headers
/// arrive, each read of the body is bounded instead (`stream_idle_timeout`).
const DEFAULT_SEND_TIMEOUT_SECS: u64 = 600;

// =============================================================================
// Adapter struct
// =============================================================================

/// Drives a llama-server instance via its OpenAI-compatible streaming API.
///
/// Implements [`LlmCompletionPort`] so the pure-domain `gglib-agent` crate can
/// call an LLM without knowing anything about HTTP, SSE framing, or the
/// `OpenAI` wire format.
pub struct LlmCompletionAdapter {
    url: String,
    /// Forwarded verbatim as the `model` field in the `OpenAI` request body.
    ///
    /// llama-server ignores this field when serving a single model.  Set it
    /// when the server is serving multiple GGUF files by name (e.g. via
    /// `--model-alias`) or when routing through a proxy that selects backends
    /// by model name.
    model: String,
    client: Client,
    /// The other machine this turn is going to, when it is going to one.
    ///
    /// `None` for a llama-server on loopback, which asks for nothing and has
    /// no name. Set for the remote tunnel's loopback port (ADR 0012), which
    /// is the far machine's proxy and demands its key; the listener there
    /// does not inject one, so this side has to. Its identity rides along
    /// with the key because a refusal from that machine has to name it —
    /// see [`FarMachine`]. Never logged and absent from every `Debug` the
    /// adapter takes part in: the struct derives none, and neither does that
    /// one.
    far_machine: Option<FarMachine>,
    /// Where the images a message names by id are read from (`images.rs`).
    /// `None` for a caller whose messages carry none.
    attachments: Option<Arc<dyn AttachmentStore>>,
    /// What a person chose for this turn, and nothing a stored layer supplied
    /// — the top layer of the hierarchy, equivalent to what an external client
    /// sends the proxy. Written into the body by [`body::build_chat_body`] and
    /// read back out by [`request_pipeline::apply()`], which folds
    /// [`Self::layers`], the model's own defaults and the floor beneath it.
    /// A value that fold passes over is taken back out of the body
    /// ([`erase_passed_over`]).
    ///
    /// A value a stored layer supplied must never be handed in here: it would
    /// reach the pipeline as a person's choice, which outranks every layer
    /// and which the agentic temperature ceiling never lowers.
    sampling: Option<InferenceConfig>,
    /// The stored layers beneath [`Self::sampling`]: the profile the caller
    /// selected and the settings' global defaults. Handed to
    /// [`request_pipeline::apply()`] as they are, so the ladder is folded
    /// there and nowhere before it. Empty (the default) for a caller with
    /// neither.
    layers: SamplingLayers,
    /// Told what each request's sampling resolved to. `None` (the default)
    /// for a caller with nothing to say about it.
    sampling_observer: Option<SamplingObserver>,
    /// Timeout (seconds) for the `.send()` phase (connect through response
    /// headers).  Defaults to [`DEFAULT_SEND_TIMEOUT_SECS`].
    send_timeout_secs: u64,
    /// How long one read of the reply may wait before the run's stream ends:
    /// the proxy's bound, [`gglib_proxy::STREAM_IDLE_TIMEOUT`], since a run
    /// reads llama-server past the proxy. Tests shorten it.
    stream_idle_timeout: std::time::Duration,
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

/// Told what one request's sampling resolved to, once the pipeline has
/// decided it: the values sent and the rung that supplied each.
///
/// The ladder is folded in [`request_pipeline::apply()`] and nowhere else, so
/// a caller that has something to say about the outcome — the terminal, of a
/// flag the ladder passed over — is told here rather than folding a ladder
/// of its own to find out.
pub type SamplingObserver = Arc<dyn Fn(&SamplingDecision) + Send + Sync>;

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
        images: &images::ImageUrls,
    ) -> Result<serde_json::Value> {
        let sampling = self.sampling.as_ref();
        let mut body = body::build_chat_body(&self.model, messages, tools, sampling, images);

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

        // The same pipeline, in the same order, that the proxy runs, and the
        // one place this turn's sampling ladder is folded. `build_chat_body`
        // has already written the caller's own parameters into the body, which
        // is exactly where an external client's would be, so `apply` reads
        // them back as the top layer and resolves the caller's stored layers
        // (`self.layers`), the model's and the floor beneath them.
        //
        // `trust_client_sampling: true` unconditionally: `Settings.trust_client_sampling`
        // gates an *external* client's request body against a boilerplate value it
        // may have no user-facing control over (VS Code Copilot's hardcoded
        // `temperature: 0`, for one). `self.sampling` is not that — it is gglib's own
        // typed caller config (terminal flags, a run's reasoning controls), built by
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
                // Unconditionally on: no caller hands over
                // `Settings.agentic_sampling`, and this path is the agent
                // loop, whose turns with tools are exactly what the ceiling
                // exists for. The `GGLIB_DISABLE_AGENTIC_SAMPLING` env switch
                // still reaches it.
                agentic_adjustments: true,
                ..self.layers.clone()
            },
            self.model_context.context_budget(),
        )
        .map_err(|e| anyhow!("conversation exceeds the model's context budget: {e}"))?;

        if let Some(named) = sampling {
            erase_passed_over(&mut body, named, &report.sampling);
        }

        if let Some(observe) = &self.sampling_observer {
            observe(&report.sampling);
        }

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

/// Take out of `body` each parameter of `named` that the fold read and did
/// not adopt, so the request carries what `decision` says and nothing beside
/// it.
///
/// [`body::build_chat_body`] wrote `named` into the body for the fold to read
/// as its top layer, and the fold only inserts what it resolved. A penalty
/// named without the temperature it travels with is passed over when a layer
/// beneath names one, and where the floor has no value to write over it the
/// caller's key would ride on to the model.
///
/// A value the reader did not take as written stays: it was never in the fold
/// to be passed over, and the pipeline leaves it for llama-server to answer,
/// as it does an external client's.
fn erase_passed_over(
    body: &mut serde_json::Value,
    named: &InferenceConfig,
    decision: &SamplingDecision,
) {
    let Some(sent) = body.as_object_mut() else {
        return;
    };
    let adopted = decision.resolved.to_openai_json_patch();
    let unread = |key: &str| {
        let issues = &decision.client_fields_rejected;
        issues.iter().any(|issue| issue.field() == key)
    };
    for key in named.to_openai_json_patch().keys() {
        if !adopted.contains_key(key) && !unread(key) {
            sent.remove(key);
        }
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
        // Before it, each image the messages name is read from the store: a
        // refusal here (an id not stored, too many bytes) sends nothing.
        let images = images::resolve(self.attachments.as_ref(), messages).await?;
        let body = self.shaped_body(messages, tools, &images)?;

        // Each attempt's connect + first-byte phase is bounded by the send
        // timeout, and the whole sequence by the policy's own deadline, so a
        // stalled llama-server can neither hang the agent task nor multiply the
        // timeout by the attempt count. The timeout covers `.send()`, TCP
        // connect through response headers, which wait for a slot but not for
        // prefill (see `DEFAULT_SEND_TIMEOUT_SECS`).
        //
        // Retrying is safe only because it all happens here, before a single
        // body byte is read: see the `retry` module docs.
        let response = retry::send_with_retry(
            &self.client,
            &self.url,
            self.far_machine.as_ref(),
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
            self.stream_idle_timeout,
        ))
    }
}

#[cfg(test)]
#[path = "shaping_tests.rs"]
mod shaping_tests;
