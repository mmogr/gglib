//! Maps [`AgentSessionParams`] to an [`AgentLoopPort`] composition root and
//! manages llama-server lifecycle (auto-start or port reuse).
//!
//! The only public surface is [`compose`], which returns the ready-to-use
//! `Arc<dyn AgentLoopPort>` and an optional [`ProcessHandle`] that the caller
//! must stop when the session ends (`Some` only when we auto-started the
//! server).  [`AgentConfig`] is built inline by the caller from the same args.

use std::sync::Arc;

use anyhow::{Context as _, Result};
use gglib_core::ports::AgentLoopPort;
use gglib_core::{ProcessHandle, ServerConfig};
use gglib_runtime::compose_agent_loop;

use crate::bootstrap::CliContext;
use crate::handlers::chat::ChatArgs;

// =============================================================================
// Types
// =============================================================================

/// Minimal parameter set needed to compose an agent session.
///
/// Extracted from [`ChatArgs`] so that different callers (interactive chat,
/// single-turn question) can compose the agent loop without constructing a
/// full `ChatArgs`.
#[derive(Debug, Clone)]
pub struct AgentSessionParams {
    /// Model name or ID used to start llama-server.
    pub model_identifier: String,
    /// Optional context-size override (numeric string or `"max"`).
    pub ctx_size: Option<String>,
    /// When set, reuse an already-running llama-server instead of auto-starting.
    pub port: Option<u16>,
    /// Tool allowlist (empty = all tools visible).
    pub tools: Vec<String>,
    /// Model-name override forwarded to llama-server routing.
    pub model_name: Option<String>,
}

impl From<&ChatArgs> for AgentSessionParams {
    fn from(args: &ChatArgs) -> Self {
        Self {
            model_identifier: args.identifier.clone(),
            ctx_size: args.ctx_size.clone(),
            port: args.port,
            tools: args.tools.clone(),
            model_name: args.model.clone(),
        }
    }
}

// =============================================================================
// Public API
// =============================================================================

/// Compose the agent loop ready to use for a session.
///
/// Returns `(agent, maybe_handle)`:
/// - `maybe_handle` is `Some(handle)` when we auto-started a llama-server.
///   The caller **must** call `ctx.runner.stop(&handle)` when the session ends.
/// - `maybe_handle` is `None` when the caller supplied a port (reuse).
pub async fn compose(
    ctx: &CliContext,
    params: &AgentSessionParams,
) -> Result<(Arc<dyn AgentLoopPort>, Option<ProcessHandle>)> {
    // 1. Resolve the LLM port — reuse or auto-start.
    let (port, maybe_handle) = resolve_port(ctx, params).await?;

    // 2. Initialise MCP servers (CLI bootstrap intentionally skips this).
    //    A failure is logged as a warning rather than aborting the session:
    //    the agent can still run without tools.
    if let Err(e) = ctx.mcp.initialize().await {
        tracing::warn!("MCP initialisation failed — tools may be unavailable: {e}");
    }

    // 3. Compose the agent loop.  When tools are specified the loop is
    //    restricted to the named allowlist; otherwise all MCP tools are visible.
    let tool_filter = if params.tools.is_empty() {
        None
    } else {
        Some(params.tools.iter().cloned().collect())
    };
    let agent = compose_agent_loop(
        format!("http://127.0.0.1:{port}"),
        ctx.http_client.clone(),
        params.model_name.clone(),
        Arc::clone(&ctx.mcp),
        tool_filter,
    );

    Ok((agent, maybe_handle))
}

// =============================================================================
// Private helpers
// =============================================================================

/// Return `(port, maybe_handle)`.
///
/// When a port is supplied the server is treated as externally managed
/// and no `ProcessHandle` is returned.  Otherwise a llama-server is spawned
/// via [`CliContext::runner`] and the resulting handle is returned so the
/// caller can stop it on exit.
async fn resolve_port(
    ctx: &CliContext,
    params: &AgentSessionParams,
) -> Result<(u16, Option<ProcessHandle>)> {
    if let Some(port) = params.port {
        tracing::debug!("reusing user-supplied llama-server on port {port}");
        return Ok((port, None));
    }

    // Auto-start: look up the model, then ask the process runner to start it.
    let model = ctx
        .app
        .models()
        .find_by_identifier(&params.model_identifier)
        .await
        .context("failed to look up model")?;

    // Parse an explicit numeric ctx_size; ignore "max" (pass None, let the
    // runner use the model's native context length).
    let context_size = params
        .ctx_size
        .as_deref()
        .and_then(|s| s.parse::<u64>().ok());

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
        .runner
        .start(server_config)
        .await
        .context("failed to start llama-server")?;

    println!("llama-server ready on port {}", handle.port);

    Ok((handle.port, Some(handle)))
}
