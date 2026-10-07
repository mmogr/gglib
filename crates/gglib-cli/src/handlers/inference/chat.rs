//! Chat command handler.
//!
//! Delegates all interactive chat to the agentic REPL via `agent_chat::run()`.

use anyhow::Result;

use crate::bootstrap::CliContext;
use crate::shared_args::{ContextArgs, SamplingArgs};
use crate::target::Target;

/// Arguments for the chat command.
#[derive(Debug, Clone)]
pub(crate) struct ChatArgs {
    pub identifier: String,
    pub context: ContextArgs,
    pub system_prompt: Option<String>,
    pub sampling: SamplingArgs,
    /// Budget for retrying transient upstream failures, already resolved from
    /// `--no-retry` and the `GGLIB_LLM_RETRY_*` overrides.
    pub retry_policy: gglib_core::retry::RetryPolicy,
    /// Disable tools — run as a plain LLM chat.
    pub no_tools: bool,
    pub port: Option<u16>,
    /// Which machine the turn runs on (ADR 0013).
    pub target: Target,
    /// `--max-iterations`, or on a resume the limit the chat saved: the
    /// turn's own, which `TurnLimits::resolve` puts before the stored one.
    pub max_iterations: Option<usize>,
    pub tools: Vec<String>,
    pub tool_timeout_ms: Option<u64>,
    pub max_parallel: Option<usize>,
    /// `--image`: the files attached to the first message.
    pub images: Vec<std::path::PathBuf>,
    /// Mirror of the global `--verbose` / `-v` flag for agentic mode rendering.
    pub verbose: bool,
    /// Optional model-name override for llama-server routing.
    pub model: Option<String>,
    /// Named sampling profile, the flag form of a `{model}:{profile}` suffix.
    pub profile: Option<String>,
    /// Resume a previous conversation by ID.
    pub continue_id: Option<i64>,
    /// Observation-tool name patterns for the dual-threshold loop guard.
    /// An empty vec means "use defaults" (see `AgentConfig::observation_tools`).
    pub observation_tools: Vec<String>,
    /// Elevated repetition limit for observation-only tool batches.
    pub max_observation_steps: Option<usize>,
}

/// Execute the chat command — always routes to the agentic REPL.
pub(crate) async fn execute(ctx: &CliContext, args: ChatArgs) -> Result<()> {
    crate::handlers::agent_chat::run(ctx, &args).await
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn chat_args_struct_is_send() {
        fn assert_send<T: Send>() {}
        assert_send::<ChatArgs>();
    }
}

#[cfg(test)]
#[path = "image_refusal_tests.rs"]
mod image_refusal_tests;
