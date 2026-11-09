//! Chat command handler.
//!
//! Delegates all interactive chat to the agentic REPL via `agent_chat::run()`.

use anyhow::Result;

use crate::bootstrap::CliContext;
use crate::shared_args::SamplingArgs;
use crate::target::Target;

/// Arguments for the chat command.
#[derive(Debug, Clone)]
pub(crate) struct ChatArgs {
    pub identifier: String,
    pub system_prompt: Option<String>,
    pub sampling: SamplingArgs,
    /// Disable tools — run as a plain LLM chat.
    pub no_tools: bool,
    /// Which machine the turn runs on (ADR 0013).
    pub target: Target,
    /// `--max-iterations`: saved with the chat, whose turns the daemon
    /// limits by it.
    pub max_iterations: Option<usize>,
    pub tools: Vec<String>,
    pub tool_timeout_ms: Option<u64>,
    pub max_parallel: Option<usize>,
    /// `--image`: the files attached to the first message.
    pub images: Vec<std::path::PathBuf>,
    /// Mirror of the global `--verbose` / `-v` flag for agentic mode rendering.
    pub verbose: bool,
    /// Named sampling profile, the flag form of a `{model}:{profile}` suffix.
    pub profile: Option<String>,
    /// Resume a previous conversation by ID.
    pub continue_id: Option<i64>,
    /// `--thinking`: the Thinking choice this command line names for the
    /// chat, as a turn says one (`on` is the turn's `default`).
    pub thinking: Option<gglib_core::domain::Thinking>,
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
