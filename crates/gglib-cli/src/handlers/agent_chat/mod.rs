//! Interactive agentic chat handler for `gglib chat`.
//!
//! Entry point: [`run`].  Sub-modules keep each concern small and
//! independently readable:
//! - [`config`]   — resolves LLM port + MCP tools, composes an [`gglib_core::ports::AgentLoopPort`]
//! - [`renderer`] — maps [`gglib_core::AgentEvent`] variants to terminal output
//! - [`repl`]     — async REPL loop with `rustyline` + `spawn_blocking` input
//! - [`tool_format`] — tool-result summary formatters
//! - [`markdown`] — Markdown normalisation + termimad rendering
//! - [`thinking_dispatch`] — `RenderContext`, thinking-event dispatch, spinner coordination

pub mod config;
mod markdown;
pub mod persistence;
pub mod renderer;
pub mod repl;
mod thinking_dispatch;
mod tool_format;

use anyhow::Result;

use crate::bootstrap::CliContext;
use crate::handlers::inference::chat::ChatArgs;

use self::persistence::Conversation;

/// Entry point: start the interactive agentic REPL.
///
/// Manages the server lifecycle (auto-start / stop) around the REPL session.
pub async fn run(ctx: &CliContext, args: &ChatArgs) -> Result<()> {
    let inference_config = args.sampling.clone().into_inference_config();
    let sampling = if inference_config == Default::default() {
        None
    } else {
        Some(inference_config)
    };
    let (agent, maybe_handle) = config::compose(ctx, &args.into(), None, sampling).await?;

    // Create a conversation for persistence (best-effort).
    let persistence =
        match Conversation::create(ctx.app.chat_history(), args.system_prompt.clone()).await {
            Ok(conv) => Some(conv),
            Err(e) => {
                tracing::warn!("failed to create agent conversation: {e}");
                None
            }
        };

    let result = repl::run_repl(agent, args, persistence).await;

    if let Some(ref handle) = maybe_handle
        && let Err(e) = ctx.runner.stop(handle).await
    {
        tracing::warn!("failed to stop llama-server after agent chat: {e}");
    }

    result
}
