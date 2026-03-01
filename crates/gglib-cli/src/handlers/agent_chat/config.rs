//! Maps [`ChatArgs`] flags to an [`AgentLoopPort`] composition root and
//! manages llama-server lifecycle (auto-start or port reuse).
//!
//! The only public surface is [`compose`], which returns the ready-to-use
//! `Arc<dyn AgentLoopPort>` and an optional [`ProcessHandle`] that the caller
//! must stop when the session ends (`Some` only when we auto-started the
//! server).  [`AgentConfig`] is built inline by the REPL from the same `args`.

use std::sync::Arc;

use anyhow::{Context as _, Result};
use gglib_agent::AgentLoop;
use gglib_core::ports::{AgentLoopPort, LlmCompletionPort, ToolExecutorPort};
use gglib_core::{ProcessHandle, ServerConfig};
use gglib_mcp::McpToolExecutorAdapter;
use gglib_runtime::LlmCompletionAdapter;

use crate::bootstrap::CliContext;
use crate::handlers::chat::ChatArgs;

// =============================================================================
// Public API
// =============================================================================

/// Compose the agent loop ready to use for a chat session.
///
/// Returns `(agent, maybe_handle)`:
/// - `maybe_handle` is `Some(handle)` when we auto-started a llama-server.
///   The caller **must** call `ctx.runner().stop(&handle)` when the session ends.
/// - `maybe_handle` is `None` when the caller supplied `--port` (reuse).
pub async fn compose(
    ctx: &CliContext,
    args: &ChatArgs,
) -> Result<(Arc<dyn AgentLoopPort>, Option<ProcessHandle>)> {
    // 1. Resolve the LLM port — reuse or auto-start.
    let (port, maybe_handle) = resolve_port(ctx, args).await?;

    // 2. Initialise MCP servers (CLI bootstrap intentionally skips this).
    //    A failure is logged as a warning rather than aborting the session:
    //    the agent can still run without tools.
    if let Err(e) = ctx.mcp().initialize().await {
        eprintln!("warning: MCP initialisation failed — tools may be unavailable: {e}");
        tracing::warn!("MCP initialisation failed — tools may be unavailable: {e}");
    }

    // 3. Compose the agent loop.  When `--tools` is supplied the loop is
    //    restricted to the named allowlist; otherwise all MCP tools are visible.
    let tool_filter = if args.tools.is_empty() {
        None
    } else {
        Some(args.tools.iter().cloned().collect())
    };
    let llm: Arc<dyn LlmCompletionPort> =
        Arc::new(LlmCompletionAdapter::with_client(port, reqwest::Client::new(), None::<String>));
    let tool_executor: Arc<dyn ToolExecutorPort> =
        Arc::new(McpToolExecutorAdapter::new(Arc::clone(ctx.mcp())));
    let agent = AgentLoop::build(llm, tool_executor, tool_filter);

    Ok((agent, maybe_handle))
}

// =============================================================================
// Private helpers
// =============================================================================

/// Return `(port, maybe_handle)`.
///
///
/// When `args.port` is supplied the server is treated as externally managed
/// and no `ProcessHandle` is returned.  Otherwise a llama-server is spawned
/// via [`CliContext::runner`] and the resulting handle is returned so the
/// caller can stop it on exit.
async fn resolve_port(ctx: &CliContext, args: &ChatArgs) -> Result<(u16, Option<ProcessHandle>)> {
    if let Some(port) = args.port {
        tracing::debug!("reusing user-supplied llama-server on port {port}");
        return Ok((port, None));
    }

    // Auto-start: look up the model, then ask the process runner to start it.
    let model = ctx
        .app()
        .models()
        .find_by_identifier(&args.identifier)
        .await
        .context("failed to look up model")?;

    // Parse an explicit numeric ctx_size; ignore "max" (pass None, let the
    // runner use the model's native context length).
    let context_size = args.ctx_size.as_deref().and_then(|s| s.parse::<u64>().ok());

    let mut server_config = ServerConfig::new(
        model.id,
        model.name.clone(),
        model.file_path.clone(),
        ctx.base_port,
    );
    if let Some(ctx_size) = context_size {
        server_config = server_config.with_context_size(ctx_size);
    }

    println!(
        "Starting llama-server for '{}' (this may take a moment) …",
        model.name
    );

    let handle = ctx
        .runner()
        .start(server_config)
        .await
        .context("failed to start llama-server")?;

    println!("llama-server ready on port {}", handle.port);

    Ok((handle.port, Some(handle)))
}
