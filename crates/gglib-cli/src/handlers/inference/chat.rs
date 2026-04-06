//! Chat command handler.
//!
//! Delegates all interactive chat to the agentic REPL via `agent_chat::run()`.

use anyhow::Result;

use crate::bootstrap::CliContext;
use crate::shared_args::{ContextArgs, SamplingArgs};

/// Arguments for the chat command.
#[derive(Debug, Clone)]
pub struct ChatArgs {
    pub identifier: String,
    pub context: ContextArgs,
    pub system_prompt: Option<String>,
    pub sampling: SamplingArgs,
    /// Disable tools — run as a plain LLM chat.
    pub no_tools: bool,
    pub port: Option<u16>,
    pub max_iterations: usize,
    pub tools: Vec<String>,
    pub tool_timeout_ms: Option<u64>,
    pub max_parallel: Option<usize>,
    /// Mirror of the global `--verbose` / `-v` flag for agentic mode rendering.
    pub verbose: bool,
    /// Optional model-name override for llama-server routing.
    pub model: Option<String>,
    /// Resume a previous conversation by ID.
    pub continue_id: Option<i64>,
}

/// Execute the chat command — always routes to the agentic REPL.
pub async fn execute(ctx: &CliContext, args: ChatArgs) -> Result<()> {
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
