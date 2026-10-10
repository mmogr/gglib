//! Request DTOs for `POST /api/agent/chat`.

use serde::Deserialize;

use gglib_core::Settings;
use gglib_core::domain::ModelRef;
use gglib_core::domain::agent::{AgentConfig, AgentMessage, TurnLimits};

/// User-facing configuration for a single agent chat request.
///
/// Exposes only the fields that are safe to accept from an untrusted HTTP
/// caller. Internal tuning parameters (`prune_*`, `max_repeated_batch_steps`,
/// `context_budget_chars`, etc.) are intentionally absent — they default to
/// their well-tested values and cannot be weaponised to exhaust server
/// resources.
///
/// All numeric fields are clamped server-side to the ceiling constants defined
/// in [`gglib_core::domain::agent::config`] to prevent resource exhaustion.
///
/// # Observation-tool fields
///
/// `observation_tools` and `max_observation_steps` are intentionally exposed
/// to callers because gglib is a BYO-MCP platform: users may connect arbitrary
/// MCP servers whose tool names are unknown at compile time.  Callers that want
/// to classify a custom tool as observation-only (and therefore subject to the
/// higher repetition threshold) should pass its name fragment here.
#[derive(Debug, Deserialize, Default)]
#[cfg_attr(feature = "ts-bindings", derive(ts_rs::TS), ts(export))]
#[serde(default)]
pub(crate) struct AgentRequestConfig {
    /// Maximum number of LLM→tool→LLM iterations.
    /// Clamped to [`MAX_ITERATIONS_CEILING`](gglib_core::domain::agent::config::MAX_ITERATIONS_CEILING)
    /// server-side. `None` (field absent) is the limit the run's conversation
    /// saved, then the stored `max_tool_iterations` setting, and the built-in
    /// default of 25 when none is stored.
    pub max_iterations: Option<usize>,

    /// Maximum number of tool calls dispatched in parallel per iteration.
    /// Clamped to [`MAX_PARALLEL_TOOLS_CEILING`](gglib_core::domain::agent::config::MAX_PARALLEL_TOOLS_CEILING)
    /// server-side.
    pub max_parallel_tools: Option<usize>,

    /// Per-tool execution timeout in milliseconds.
    /// Clamped to [`MAX_TOOL_TIMEOUT_MS_CEILING`](gglib_core::domain::agent::config::MAX_TOOL_TIMEOUT_MS_CEILING)
    /// server-side.
    #[cfg_attr(feature = "ts-bindings", ts(type = "number | null"))]
    pub tool_timeout_ms: Option<u64>,

    /// Substring/suffix patterns that classify a tool as observation-only.
    ///
    /// When **every** call in a batch matches a pattern, the higher
    /// `max_observation_steps` threshold is applied instead of the standard
    /// loop-detection threshold.  Matching is case-insensitive and uses
    /// `ends_with` OR `contains` semantics.
    ///
    /// `Some([])` disables observation classification entirely.
    /// `None` (field absent) keeps the built-in defaults, which cover
    /// browser tools and the read-only tools coding agents repeat — see
    /// `AgentConfig::default`.
    pub observation_tools: Option<Vec<String>>,

    /// Maximum repetitions of an observation-only batch **that keeps getting
    /// the same answer back** before loop detection fires.
    ///
    /// A repeat whose answer changed is not counted at all: the same call with
    /// a different result is an agent polling for output, not a loop.
    ///
    /// This value is read twice. For a batch that is **not** observation-only
    /// it is never the strike threshold, but it *is* the ceiling on how far
    /// changing answers may carry that batch — otherwise any tool whose output
    /// carries a clock would be exempt from the guard entirely. Raising this
    /// therefore loosens the guard for every batch, not only read-only ones.
    ///
    /// Clamped to `MAX_OBSERVATION_STEPS_CEILING` (100) server-side.
    /// `None` (field absent) keeps the built-in default of `15`.
    pub max_observation_steps: Option<usize>,
}

impl AgentRequestConfig {
    /// Build the validated [`AgentConfig`] for this request, its limits
    /// resolved against this machine's `settings` by the one rule the CLI
    /// uses too ([`TurnLimits::resolve`]).
    ///
    /// An omitted `max_iterations` is the stored `max_tool_iterations`: a
    /// conversation's saved limit is put in its place before this is called.
    /// `max_stagnation_steps` comes from the settings alone, not the
    /// request: it stays a server-side knob, consistent with this DTO's
    /// "safe subset" policy of not exposing internal strike limits to
    /// untrusted callers.
    pub(crate) fn into_agent_config(self, settings: Option<&Settings>) -> AgentConfig {
        let limits = TurnLimits::resolve(self.max_iterations, settings);
        AgentConfig::from_user_params(
            Some(limits.max_iterations),
            self.max_parallel_tools,
            self.tool_timeout_ms,
            self.observation_tools,
            self.max_observation_steps,
            limits.max_stagnation_steps,
        )
        .expect("clamped AgentConfig must pass validation")
    }
}

/// Request body for `POST /api/agent/chat`.
#[derive(Debug, Deserialize)]
#[cfg_attr(feature = "ts-bindings", derive(ts_rs::TS), ts(export))]
pub(crate) struct AgentChatRequest {
    /// Port of the llama-server instance to drive.
    ///
    /// Must match a currently-running server (the same constraint as the chat
    /// proxy endpoint). Validated by [`validate_port`](crate::handlers::port_utils::validate_port)
    /// before the loop starts. Ignored when [`Self::far`] is set.
    pub port: u16,

    /// A model of the paired machine to drive instead of a local
    /// llama-server (ADR 0012): that machine, and the model's id there.
    ///
    /// The daemon must be connected to that machine (`gglib remote join`)
    /// and hold the key from that pairing; the loop then talks to the
    /// tunnel's loopback port with that key and sends the id as the model,
    /// and `port` is not consulted. A ref to this machine is refused `400`,
    /// since a local model is driven by its port, and one to a machine this
    /// one is no longer connected to `409`. Absent means local.
    #[serde(default)]
    pub far: Option<ModelRef>,

    /// Full conversation history in domain form.
    ///
    /// Supports all four [`AgentMessage`] variants: `system`, `user`,
    /// `assistant` (with or without `tool_calls`), and `tool`.
    ///
    /// # Security note
    ///
    /// This field is not validated for structural consistency.  A client could
    /// forge `AgentMessage::Tool` entries with invented `tool_call_id` values,
    /// or `AgentMessage::Assistant` entries with arbitrary `tool_calls`, and
    /// the loop would accept them.  Known limitation: callers are trusted to
    /// supply a structurally sound history (i.e. every `Tool` message
    /// references an `id` that appeared in a preceding `Assistant.tool_calls`).
    pub messages: Vec<AgentMessage>,

    /// Optional loop tuning, restricted to safe user-facing fields.
    ///
    /// When `None` (or omitted), the iteration limit is the stored
    /// `max_tool_iterations` setting and every other field defaults to the
    /// value in [`AgentConfig::default`].
    pub config: Option<AgentRequestConfig>,

    /// Optional allowlist of tool names to expose to the model.
    ///
    /// - `None` (JSON `null` or field absent): all tools from all connected MCP
    ///   servers are available.
    /// - `Some([])` (JSON `[]`): **no tools** are exposed — tool calling is
    ///   effectively disabled.  Not equivalent to `None`; clients that want
    ///   all tools must use `null`, not `[]`.
    /// - `Some(["tool_a", "tool_b"])`: only the listed tools are sent to the LLM
    ///   and can be executed.
    pub tool_filter: Option<Vec<String>>,

    /// The model name forwarded to the local llama-server.
    ///
    /// Optional: `None` (or omitted) lets llama-server pick the model it
    /// loaded, which is the normal case, and a value is only needed when the
    /// server exposes several. Not read with [`Self::far`], which names its
    /// model by its id.
    #[serde(default)]
    pub model: Option<String>,

    /// How hard to ask the model to think, where its chat template reads the
    /// variable.
    ///
    /// Conditional by construction: a template that does not read
    /// `reasoning_effort` ignores it in perfect silence, and stage 5b of the
    /// request pipeline deletes the key outright on a model whose observed caps
    /// say so (ADR 0007 decision 3).
    ///
    /// No `none` level exists: omitting the field is what leaves the template's
    /// own default in place.
    #[serde(default)]
    pub reasoning_effort: Option<gglib_core::domain::ReasoningEffort>,

    /// Ceiling on each turn's thinking tokens. `-1` defers to the launch-time
    /// default; `0` stops thinking altogether.
    ///
    /// Enforced by llama.cpp's own sampler-side budget rather than by a
    /// template, so it holds on models where the effort level does nothing —
    /// which is why the two are separate fields and not one knob.
    #[serde(default)]
    pub reasoning_budget_tokens: Option<i32>,

    /// The message was sent with Draw pressed: the model is offered
    /// `builtin:generate_image` for this request, whatever `tool_filter`
    /// says. Absent or `false`, the image tool is in no tool list and cannot
    /// be called, `tool_filter: null` included. Refused `400
    /// drawing_unavailable` when this machine cannot draw for it
    /// (`GET /api/images/drawing` says why beforehand).
    #[serde(default)]
    #[cfg_attr(feature = "ts-bindings", ts(as = "Option<bool>", optional))]
    pub draw: bool,
}

impl AgentChatRequest {
    /// The request's own sampling layer — the top rung of the hierarchy.
    ///
    /// # Why only two fields, on a DTO that accepts no other sampling
    ///
    /// This endpoint has never taken a `temperature`, a `top_p`, or anything
    /// else the sampler reads, and this does not open that door: the returned
    /// config names these two and leaves every other field `None`, so each one
    /// resolves from the layers beneath it. For a local run those are the
    /// values of the model its port serves, this machine's global defaults
    /// and the floor (`remote_upstream::local`); no profile, which nothing
    /// in the request can name.
    ///
    /// The asymmetry is deliberate rather than an oversight to tidy up later.
    /// The sampler parameters are per-*model* tuning — they belong to the model
    /// row and the operator's settings, which is where an agent loop should read
    /// them from, and a caller overriding them per request is asking to
    /// un-tune the model. The reasoning controls are per-*turn* shape: how long
    /// this particular question is worth thinking about is a property of the
    /// question, not of the model, and there is no other layer that can know it.
    ///
    /// `None` when the request named neither, which keeps the adapter's
    /// "resolve entirely from the layers beneath" path — and its cheaper
    /// `with_sampling(None)` — for the overwhelmingly common case.
    pub(crate) const fn sampling_layer(&self) -> Option<gglib_core::domain::InferenceConfig> {
        if self.reasoning_effort.is_none() && self.reasoning_budget_tokens.is_none() {
            return None;
        }
        // Written out in full rather than with `..Default::default()`, so a
        // field added to `InferenceConfig` fails to compile here and someone
        // has to decide whether this endpoint should accept it — which is the
        // question the paragraph above answers, and the one a struct-update
        // shorthand would answer silently as "no, forever".
        Some(gglib_core::domain::InferenceConfig {
            reasoning_effort: self.reasoning_effort,
            reasoning_budget_tokens: self.reasoning_budget_tokens,
            temperature: None,
            top_p: None,
            top_k: None,
            max_tokens: None,
            repeat_penalty: None,
            presence_penalty: None,
            min_p: None,
            frequency_penalty: None,
            dry_multiplier: None,
            dry_base: None,
            dry_allowed_length: None,
            dry_penalty_last_n: None,
            dynatemp_range: None,
            dynatemp_exponent: None,
            top_n_sigma: None,
            seed: None,
        })
    }
}

/// Request body for an agent run, `PUT /api/runs/{id}?kind=agent`: the body
/// `/api/agent/chat` takes, and where to save the transcript.
#[derive(Debug, Deserialize)]
#[cfg_attr(feature = "ts-bindings", derive(ts_rs::TS), ts(export))]
pub(crate) struct AgentRunRequest {
    /// The agent chat request, as `/api/agent/chat` takes it.
    #[serde(flatten)]
    #[cfg_attr(feature = "ts-bindings", ts(flatten))]
    pub chat: AgentChatRequest,

    /// The saved conversation the daemon writes the transcript to: the
    /// request's last message when the run is created, if it is the user's,
    /// and the reply when the run ends. Absent, nothing is saved.
    #[serde(default)]
    #[cfg_attr(feature = "ts-bindings", ts(type = "number | null"))]
    pub conversation_id: Option<i64>,

    /// Whether the run answers the conversation's last question, already
    /// saved, rather than a message of its own: it then sends no messages,
    /// runs from the conversation's saved history, and saves only the
    /// reply. An edit or a regenerate leaves the chat to be answered this
    /// way (`POST /api/conversations/{id}/changes`), and so does Retry.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    #[cfg_attr(feature = "ts-bindings", ts(as = "Option<bool>", optional))]
    pub answer_saved: bool,

    /// The conversation's Thinking choice, said only on the run that changes
    /// it: `off` runs this turn and the conversation's later ones with a
    /// thinking budget of `0`, whatever `reasoning_budget_tokens` says;
    /// `default` forgets that. Absent, the run is as the conversation
    /// remembers. Without a `conversation_id` it holds for this run alone.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "ts-bindings", ts(optional))]
    pub thinking: Option<gglib_core::domain::Thinking>,
}

#[cfg(test)]
#[path = "dto_tests.rs"]
mod dto_tests;
