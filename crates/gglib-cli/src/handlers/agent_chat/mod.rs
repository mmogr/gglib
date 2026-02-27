//! Interactive agentic chat handler for `gglib chat --agent`.
//!
//! This module composes `gglib-agent`'s [`AgentLoop`] with an MCP tool
//! executor and a live `llama-server` to run a multi-turn, tool-calling
//! conversation in the terminal.
//!
//! Sub-modules keep each concern small and independently readable:
//! - [`config`]   — maps `ChatArgs` flags to [`AgentConfig`] / tool executor
//! - [`renderer`] — maps [`AgentEvent`] variants to terminal output
//! - [`repl`]     — async REPL loop with `rustyline` + `spawn_blocking` input

pub mod config;
pub mod renderer;
pub mod repl;

use std::sync::Arc;

use anyhow::Result;
use gglib_core::ports::AgentLoopPort;

use crate::bootstrap::CliContext;
use crate::handlers::chat::ChatArgs;

/// Entry point: start the interactive agentic REPL.
///
/// Called from [`crate::handlers::chat::execute`] when `args.agent` is `true`.
/// Manages the server lifecycle (auto-start / stop) around the REPL session.
pub async fn run(ctx: &CliContext, args: &ChatArgs) -> Result<()> {
    let (agent_loop, maybe_handle) = config::compose(ctx, args).await?;

    // Wrap in Arc<dyn AgentLoopPort> so the REPL can cheaply clone the
    // reference for each spawned per-turn task without requiring AgentLoop
    // to implement Clone.
    let agent: Arc<dyn AgentLoopPort> = Arc::new(agent_loop);

    let result = repl::run_repl(agent, args).await;

    if let Some(ref handle) = maybe_handle {
        if let Err(e) = ctx.runner().stop(handle).await {
            tracing::warn!("failed to stop llama-server after agent chat: {e}");
        }
    }

    result
}
