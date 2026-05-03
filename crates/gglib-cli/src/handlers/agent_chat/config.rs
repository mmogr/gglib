//! Maps [`AgentSessionParams`] to an [`AgentLoopPort`] composition root and
//! manages llama-server lifecycle (auto-start or port reuse).
//!
//! The only public surface is [`compose`], which returns the ready-to-use
//! `Arc<dyn AgentLoopPort>` and an optional [`ProcessHandle`] that the caller
//! must stop when the session ends (`Some` only when we auto-started the
//! server).  [`AgentConfig`] is built inline by the caller from the same args.

use std::path::PathBuf;
use std::sync::Arc;

use anyhow::{Context as _, Result};
use gglib_core::domain::InferenceConfig;
use gglib_core::ports::AgentLoopPort;
use gglib_core::{ProcessHandle, ServerConfig};
use gglib_runtime::compose_agent_loop_with_sampling;
use gglib_runtime::llama::args::{resolve_jinja_flag, resolve_reasoning_format};
use gglib_runtime::llama::{ContextInput, resolve_context_size};

use crate::bootstrap::CliContext;
use crate::handlers::inference::chat::ChatArgs;
use crate::handlers::inference::shared::log_context_info;
use crate::presentation::style;

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

/// Display metadata for the server-startup info banner.
///
/// Callers populate this with whatever session context they have so that
/// `resolve_port` can render a richer startup message.
#[derive(Debug, Clone, Default)]
pub struct BannerInfo {
    /// Suppress the banner entirely (e.g. `gglib q -Q`).
    pub quiet: bool,
    /// Sampling overrides to display (only non-default values are shown).
    pub sampling: Option<InferenceConfig>,
    /// Character count of prior conversation history being loaded (resume only).
    pub prior_history_chars: Option<usize>,
}

impl From<&ChatArgs> for AgentSessionParams {
    fn from(args: &ChatArgs) -> Self {
        // When --no-tools is set, use a sentinel allowlist that matches nothing
        // so the agent loop exposes zero tools to the model.
        let tools = if args.no_tools {
            vec!["__none__".into()]
        } else {
            args.tools.clone()
        };
        Self {
            model_identifier: args.identifier.clone(),
            ctx_size: args.context.ctx_size.clone(),
            port: args.port,
            tools,
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
///
/// When `sandbox_root` is `Some`, filesystem tools are restricted to that
/// directory.  Pass `None` for an unsandboxed session.
pub async fn compose(
    ctx: &CliContext,
    params: &AgentSessionParams,
    sandbox_root: Option<PathBuf>,
    sampling: Option<InferenceConfig>,
    banner: &BannerInfo,
) -> Result<(Arc<dyn AgentLoopPort>, Option<ProcessHandle>)> {
    // 1. Resolve the LLM port — reuse or auto-start.
    let (port, maybe_handle, started_model) = resolve_port(ctx, params, banner).await?;

    // 2. Resolve sampling through the shared hierarchy:
    // request override -> model defaults -> global defaults -> hardcoded fallback.
    let settings = ctx
        .app
        .settings()
        .get()
        .await
        .context("failed to load settings for inference resolution")?;

    let mut model_defaults = started_model
        .as_ref()
        .and_then(|m| m.inference_defaults.clone());
    if model_defaults.is_none() && !params.model_identifier.is_empty() {
        match ctx.app.models().get(&params.model_identifier).await {
            Ok(Some(model)) => {
                model_defaults = model.inference_defaults;
            }
            Ok(None) => {}
            Err(e) => tracing::warn!(
                model = %params.model_identifier,
                error = %e,
                "Failed to load model defaults for inference resolution"
            ),
        }
    }

    let resolved_sampling = Some(InferenceConfig::resolve_with_hierarchy(
        sampling.as_ref(),
        model_defaults.as_ref(),
        settings.inference_defaults.as_ref(),
    ));

    // 3. Initialise MCP servers (CLI bootstrap intentionally skips this).
    //    A failure is logged as a warning rather than aborting the session:
    //    the agent can still run without tools.
    if let Err(e) = ctx.mcp.initialize().await {
        tracing::warn!("MCP initialisation failed — tools may be unavailable: {e}");
    }

    // 4. Compose the agent loop.  When tools are specified the loop is
    //    restricted to the named allowlist; otherwise all MCP tools are visible.
    let tool_filter = if params.tools.is_empty() {
        None
    } else {
        Some(params.tools.iter().cloned().collect())
    };
    let base_url = format!("http://127.0.0.1:{port}");
    let agent = compose_agent_loop_with_sampling(
        base_url,
        ctx.http_client.clone(),
        params.model_name.clone(),
        Arc::clone(&ctx.mcp),
        tool_filter,
        sandbox_root,
        resolved_sampling,
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
    banner: &BannerInfo,
) -> Result<(u16, Option<ProcessHandle>, Option<gglib_core::Model>)> {
    if let Some(port) = params.port {
        tracing::debug!("reusing user-supplied llama-server on port {port}");
        return Ok((port, None, None));
    }

    // Auto-start: look up the model, then ask the process runner to start it.
    let model = ctx
        .app
        .models()
        .find_by_identifier(&params.model_identifier)
        .await
        .context("failed to look up model")?;

    // Resolve context size via the shared 3-level fallback chain.
    let settings = ctx.app.settings().get().await?;
    let context_resolution = resolve_context_size(ContextInput {
        flag: params.ctx_size.clone(),
        model_context_length: model.context_length,
        settings_default: settings.default_context_size,
    })?;

    let mut server_config = ServerConfig::new(
        model.id,
        model.name.clone(),
        model.file_path.clone(),
        ctx.base_port,
    );
    if let Some(ctx_size) = context_resolution.value {
        server_config = server_config.with_context_size(u64::from(ctx_size));
    }

    // Auto-detect jinja and reasoning format from model tags
    let jinja = resolve_jinja_flag(None, &model.tags);
    if jinja.enabled {
        tracing::debug!(source = ?jinja.source, "auto-enabling --jinja");
        server_config = server_config.with_jinja();
    }

    let reasoning = resolve_reasoning_format(None, &model.tags);
    if let Some(format) = reasoning.format {
        tracing::debug!(source = ?reasoning.source, format = %format, "auto-enabling --reasoning-format");
        server_config = server_config.with_reasoning_format(format);
    }

    if !banner.quiet {
        style::print_info_banner("Info", "\u{2139}\u{fe0f}");
        eprintln!(
            "  Starting llama-server for '{}' (this may take a moment) \u{2026}",
            model.name
        );
    }

    let handle = ctx
        .runner
        .start(server_config)
        .await
        .context("failed to start llama-server")?;

    if !banner.quiet {
        eprintln!("  llama-server ready on port {}", handle.port);

        // Context size
        log_context_info(&context_resolution);

        // Sampling overrides
        if let Some(ref s) = banner.sampling {
            print_sampling_lines(s);
        }

        // Conversation history usage (resume only)
        if let Some(chars) = banner.prior_history_chars {
            let budget = 180_000usize; // AgentConfig default
            let pct = (chars * 100).checked_div(budget).unwrap_or(0);
            eprintln!("  History: ~{chars} chars loaded (~{pct}% of context budget)");
        }

        style::print_banner_close();
    }

    Ok((handle.port, Some(handle), Some(model)))
}

/// Print non-default sampling parameter lines in the info banner.
fn print_sampling_lines(s: &InferenceConfig) {
    if let Some(v) = s.temperature {
        eprintln!("  Temperature: {v}");
    }
    if let Some(v) = s.top_p {
        eprintln!("  Top-p: {v}");
    }
    if let Some(v) = s.top_k {
        eprintln!("  Top-k: {v}");
    }
    if let Some(v) = s.max_tokens {
        eprintln!("  Max tokens: {v}");
    }
    if let Some(v) = s.repeat_penalty {
        eprintln!("  Repeat penalty: {v}");
    }
    if let Some(stop) = &s.stop {
        eprintln!("  Stop: {}", stop.join(", "));
    }
}
