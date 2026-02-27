//! Async REPL loop with `rustyline` input (wrapped in `spawn_blocking`)
//! and `tokio::select!` Ctrl+C cancellation.
//!
//! The full implementation is added in Commit 4.

use anyhow::Result;

use gglib_agent::AgentLoop;

use crate::handlers::chat::ChatArgs;

/// Run the interactive agent REPL until the user quits or EOF.
pub async fn run_repl(_agent_loop: &AgentLoop, _args: &ChatArgs) -> Result<()> {
    todo!("agent_chat::repl::run_repl — full implementation in Commit 4")
}
