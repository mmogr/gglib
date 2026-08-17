//! Request DTOs for `POST /api/agent/chat`.

use serde::Deserialize;

use gglib_core::domain::agent::{AgentConfig, AgentMessage};

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
#[serde(default)]
pub(crate) struct AgentRequestConfig {
    /// Maximum number of LLM→tool→LLM iterations.
    /// Clamped to [`MAX_ITERATIONS_CEILING`] server-side.
    pub max_iterations: Option<usize>,

    /// Maximum number of tool calls dispatched in parallel per iteration.
    /// Clamped to [`MAX_PARALLEL_TOOLS_CEILING`] server-side.
    pub max_parallel_tools: Option<usize>,

    /// Per-tool execution timeout in milliseconds.
    /// Clamped to [`MAX_TOOL_TIMEOUT_MS_CEILING`] server-side.
    pub tool_timeout_ms: Option<u64>,

    /// Substring/suffix patterns that classify a tool as observation-only.
    ///
    /// When **every** call in a batch matches a pattern, the higher
    /// `max_observation_steps` threshold is applied instead of the standard
    /// loop-detection threshold.  Matching is case-insensitive and uses
    /// `ends_with` OR `contains` semantics.
    ///
    /// `Some([])` disables observation classification entirely.
    /// `None` (field absent) keeps the built-in defaults
    /// (`["snapshot", "screenshot", "read_page", "navigate", "click"]`).
    pub observation_tools: Option<Vec<String>>,

    /// Maximum repetitions of an observation-only batch before loop detection
    /// fires.
    ///
    /// Clamped to `MAX_OBSERVATION_STEPS_CEILING` (100) server-side.
    /// `None` (field absent) keeps the built-in default of `15`.
    pub max_observation_steps: Option<usize>,
}

impl AgentRequestConfig {
    /// Build the validated [`AgentConfig`] for this request.
    ///
    /// `max_stagnation_steps` comes from persisted settings, not the request —
    /// it stays a server-side knob, consistent with this DTO's "safe subset"
    /// policy of not exposing internal strike limits to untrusted callers.
    pub(crate) fn into_agent_config(self, max_stagnation_steps: Option<usize>) -> AgentConfig {
        AgentConfig::from_user_params(
            self.max_iterations,
            self.max_parallel_tools,
            self.tool_timeout_ms,
            self.observation_tools,
            self.max_observation_steps,
            max_stagnation_steps,
        )
        .expect("clamped AgentConfig must pass validation")
    }
}

/// Request body for `POST /api/agent/chat`.
#[derive(Debug, Deserialize)]
pub(crate) struct AgentChatRequest {
    /// Port of the llama-server instance to drive.
    ///
    /// Must match a currently-running server (the same constraint as the chat
    /// proxy endpoint). Validated by [`validate_port`](crate::handlers::port_utils::validate_port)
    /// before the loop starts.
    pub port: u16,

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
    /// When `None` (or omitted), all fields default to the values in
    /// [`AgentConfig::default`], which match the TypeScript frontend constants.
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

    /// Optional model-name override forwarded to llama-server.
    ///
    /// When `None` (or omitted from the request body), the adapter lets
    /// llama-server pick the loaded model, which is the normal case.  Supply a
    /// value only when the server exposes multiple models and the caller needs
    /// to target a specific one.
    #[serde(default)]
    pub model: Option<String>,

    /// How hard to ask the model to think, where its chat template reads the
    /// variable.
    ///
    /// Conditional by construction: a template that does not read
    /// `reasoning_effort` ignores it in perfect silence, and stage 5b of the
    /// request pipeline deletes the key outright on a model whose observed caps
    /// say so (ADR 0007 decision 3). Unlike `/api/chat`, this path *does* run
    /// the pipeline, so that gate is in force here.
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
}

impl AgentChatRequest {
    /// The request's own sampling layer — the top rung of the hierarchy.
    ///
    /// # Why only two fields, on a DTO that accepts no other sampling
    ///
    /// This endpoint has never taken a `temperature`, a `top_p`, or anything
    /// else the sampler reads, and this does not open that door: the returned
    /// config names these two and leaves every other field `None`, so each one
    /// still gap-fills from the profile, per-model, global and floor layers
    /// exactly as before.
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

#[cfg(test)]
#[path = "dto_tests.rs"]
mod dto_tests;
