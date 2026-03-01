//! Maps [`ChatArgs`] flags to an [`AgentLoop`] composition root and manages
//! llama-server lifecycle (auto-start or port reuse).
//!
//! The only public surface is [`compose`], which returns the ready-to-use
//! [`AgentLoop`] and an optional [`ProcessHandle`] that the caller must stop
//! when the session ends (`Some` only when we auto-started the server).

use std::collections::HashSet;
use std::sync::Arc;

use anyhow::{Context as _, Result};
use gglib_agent::{AgentLoop, FilteredToolExecutor};
use gglib_core::domain::agent::AgentConfig;
use gglib_core::ports::ToolExecutorPort;
use gglib_core::{ProcessHandle, ServerConfig};
use gglib_mcp::McpToolExecutorAdapter;
use gglib_runtime::LlmCompletionAdapter;

use crate::bootstrap::CliContext;
use crate::handlers::chat::ChatArgs;

// =============================================================================
// Public API
// =============================================================================

/// Compose the [`AgentLoop`] ready to use for a chat session.
///
/// Returns `(loop, maybe_handle)`:
/// - `maybe_handle` is `Some(handle)` when we auto-started a llama-server.
///   The caller **must** call `ctx.runner().stop(&handle)` when the session ends.
/// - `maybe_handle` is `None` when the caller supplied `--port` (reuse).
pub async fn compose(
    ctx: &CliContext,
    args: &ChatArgs,
) -> Result<(AgentLoop, Option<ProcessHandle>)> {
    // 1. Resolve the LLM port — reuse or auto-start.
    let (port, maybe_handle) = resolve_port(ctx, args).await?;

    // 2. Initialise MCP servers (CLI bootstrap intentionally skips this).
    //    A failure is logged as a warning rather than aborting the session:
    //    the agent can still run without tools.
    if let Err(e) = ctx.mcp().initialize().await {
        tracing::warn!("MCP initialisation failed — tools may be unavailable: {e}");
    }

    // 3. Build the tool executor, optionally restricted to an allowlist.
    let tool_executor = build_tool_executor(args, ctx);

    // 4. Compose the agent loop (stateless — cheap to create).
    let llm = Arc::new(LlmCompletionAdapter::new(port, None::<String>));
    let agent_loop = AgentLoop::new(llm, tool_executor);

    Ok((agent_loop, maybe_handle))
}

/// Build an [`AgentConfig`] from CLI args.
///
/// Only `max_iterations` is overridden; all other parameters use their
/// well-tested defaults (matching the TypeScript frontend constants).
pub fn build_agent_config(args: &ChatArgs) -> AgentConfig {
    AgentConfig {
        max_iterations: args.max_iterations,
        ..AgentConfig::default()
    }
}

// =============================================================================
// Private helpers
// =============================================================================

/// Return `(port, maybe_handle)`.
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

/// Wrap the MCP executor in a [`FilteredToolExecutor`] when `--tools` is set.
fn build_tool_executor(args: &ChatArgs, ctx: &CliContext) -> Arc<dyn ToolExecutorPort> {
    let base: Arc<dyn ToolExecutorPort> =
        Arc::new(McpToolExecutorAdapter::new(Arc::clone(ctx.mcp())));

    match args.tools.as_deref() {
        None | Some("all") => base,
        Some(list) => {
            let allowed: HashSet<String> = list.split(',').map(|s| s.trim().to_owned()).collect();
            Arc::new(FilteredToolExecutor::new(base, allowed))
        }
    }
}
