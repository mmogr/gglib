//! Interactive agentic chat handler for `gglib chat --agent`.
//!
//! Entry point: [`run`].  Sub-modules keep each concern small and
//! independently readable:
//! - [`config`]   — resolves LLM port + MCP tools, composes an [`gglib_core::ports::AgentLoopPort`]
//! - [`renderer`] — maps [`gglib_core::AgentEvent`] variants to terminal output
//! - [`repl`]     — async REPL loop with `rustyline` + `spawn_blocking` input

pub mod config;
pub mod renderer;
pub mod repl;

use anyhow::Result;

use crate::bootstrap::CliContext;
use crate::handlers::chat::ChatArgs;

/// Entry point: start the interactive agentic REPL.
///
/// Called from [`crate::handlers::chat::execute`] when `args.agent` is `true`.
/// Manages the server lifecycle (auto-start / stop) around the REPL session.
pub async fn run(ctx: &CliContext, args: &ChatArgs) -> Result<()> {
    let (agent, maybe_handle) = config::compose(ctx, &args.into(), None).await?;

    let result = repl::run_repl(agent, args).await;

    if let Some(ref handle) = maybe_handle
        && let Err(e) = ctx.runner.stop(handle).await
    {
        tracing::warn!("failed to stop llama-server after agent chat: {e}");
    }

    result
}
