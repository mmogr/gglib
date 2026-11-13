//! What a run's agent loop is composed from: [`LoopRequest`], built by the
//! door a turn came through (`hub_turn::plan` for a turn on one of this
//! machine's chats, `chat_turn::read` for a chat a paired phone keeps
//! itself), never read from a body.

use gglib_core::Settings;
use gglib_core::domain::ModelRef;
use gglib_core::domain::agent::{AgentConfig, AgentMessage, ToolLimits, TurnLimits};

/// The loop limits a turn names: those its chat's settings saved. Each one
/// left out is this machine's stored setting of that name, then the
/// built-in default.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(crate) struct NamedLimits {
    /// Maximum number of LLM→tool→LLM iterations.
    pub max_iterations: Option<usize>,
    /// Maximum number of tool calls dispatched in parallel per iteration.
    pub max_parallel_tools: Option<usize>,
    /// Per-tool execution timeout in milliseconds.
    pub tool_timeout_ms: Option<u64>,
}

impl NamedLimits {
    /// The validated [`AgentConfig`] these limits run with, each one left
    /// out resolved against this machine's `settings` by the one rule the
    /// CLI reads too ([`TurnLimits::resolve`], [`ToolLimits::resolve`]).
    /// `max_stagnation_steps` comes from the settings alone, and the
    /// observation tools and their threshold are the built-in defaults.
    pub(crate) fn into_agent_config(self, settings: Option<&Settings>) -> AgentConfig {
        let limits = TurnLimits::resolve(self.max_iterations, settings);
        let tools = ToolLimits::resolve(self.max_parallel_tools, self.tool_timeout_ms, settings);
        AgentConfig::from_user_params(
            Some(limits.max_iterations),
            tools.max_parallel_tools,
            tools.tool_timeout_ms,
            None,
            None,
            limits.max_stagnation_steps,
        )
        .expect("clamped AgentConfig must pass validation")
    }
}

/// One run's loop, as its door read the turn.
#[derive(Debug, Default)]
pub(crate) struct LoopRequest {
    /// A model of the paired machine to drive instead of this machine's
    /// (ADR 0012): that machine, and the model's id there. Absent, the turn
    /// goes through this machine's endpoint.
    pub far: Option<ModelRef>,

    /// The history, then the message: the system prompt and every saved row
    /// but a system one (`saved_history`), or a phone's whole chat.
    pub messages: Vec<AgentMessage>,

    /// The limits the chat's settings name.
    pub limits: NamedLimits,

    /// The tools the model is offered: `None` every tool, `Some([])` none,
    /// and otherwise only those named.
    pub tool_filter: Option<Vec<String>>,

    /// Ceiling on each turn's thinking tokens: `0` when the turn runs with
    /// Thinking off, and otherwise none, the model's default.
    pub reasoning_budget_tokens: Option<i32>,

    /// The message was sent with Draw pressed: the model is offered
    /// `builtin:generate_image` for this run, whatever `tool_filter` says.
    /// Without it the image tool is in no tool list and cannot be called.
    pub draw: bool,

    /// The image model the person chose for this message's picture: the
    /// `id` of its row in `GET /api/images/drawing`'s `models`. Only with
    /// `draw`.
    pub image_model: Option<u32>,

    /// What the chat a turn is on saved of how it samples.
    pub saved: super::chat_sampling::ChatSampling,
}

impl LoopRequest {
    /// The turn's own sampling layer — the top rung of the hierarchy: its
    /// thinking budget, and the token limit the chat saved
    /// ([`Self::saved`]), both budgets the endpoint they go through honours
    /// from any client. Every field neither names resolves from the layers
    /// beneath, at the endpoint.
    ///
    /// `None` when neither names anything, which keeps the adapter's
    /// "resolve entirely from the layers beneath" path for the common case.
    pub(crate) fn sampling_layer(&self) -> Option<gglib_core::domain::InferenceConfig> {
        if self.reasoning_budget_tokens.is_none() && self.saved.max_tokens.is_none() {
            return None;
        }
        // Written out in full rather than with `..Default::default()`, so a
        // field added to `InferenceConfig` fails to compile here and someone
        // has to decide whether a turn should carry it, rather than a
        // struct-update shorthand answering "no, forever" in silence.
        Some(gglib_core::domain::InferenceConfig {
            reasoning_effort: None,
            reasoning_budget_tokens: self.reasoning_budget_tokens,
            temperature: None,
            top_p: None,
            top_k: None,
            max_tokens: self.saved.max_tokens,
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
#[path = "loop_request_tests.rs"]
mod loop_request_tests;
