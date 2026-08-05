//! Maps [`AgentSessionParams`] to an [`AgentLoopPort`] composition root and
//! resolves the llama-server to talk to (daemon-started, or a user-supplied
//! port).
//!
//! The only public surface is [`compose`], which returns the ready-to-use
//! `Arc<dyn AgentLoopPort>`. The llama-server itself belongs to the daemon —
//! this process never spawns or stops one, and a model left warm after the
//! session is a feature, not a leak. [`AgentConfig`] is built inline by the
//! caller from the same args.

use std::path::PathBuf;
use std::sync::Arc;

use anyhow::{Context as _, Result};
use gglib_core::domain::InferenceConfig;
use gglib_core::ports::AgentLoopPort;
use gglib_core::request_pipeline;
use gglib_core::server_config::parse_ctx_size_flag;
use gglib_runtime::compose_agent_loop_with_sampling;

use crate::bootstrap::CliContext;
use crate::handlers::inference::chat::ChatArgs;
use crate::handlers::inference::shared::resolve_inference_config;
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
    /// Budget for retrying transient upstream failures, already resolved from
    /// `--no-retry` and the `GGLIB_LLM_RETRY_*` overrides.
    pub retry_policy: gglib_core::retry::RetryPolicy,
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
            retry_policy: args.retry_policy,
        }
    }
}

// =============================================================================
// Public API
// =============================================================================

/// Compose the agent loop ready to use for a session.
///
/// The llama-server behind the returned agent is either the one the caller
/// pointed at (`--port`) or one the daemon started; either way its lifetime
/// is not this session's concern.
///
/// When `sandbox_root` is `Some`, filesystem tools are restricted to that
/// directory.  Pass `None` for an unsandboxed session.
pub async fn compose(
    ctx: &CliContext,
    params: &AgentSessionParams,
    sandbox_root: Option<PathBuf>,
    sampling: Option<InferenceConfig>,
    banner: &BannerInfo,
) -> Result<Arc<dyn AgentLoopPort>> {
    // 1. Resolve the LLM port — reuse or ask the daemon to start the model.
    let port = resolve_port(ctx, params, banner).await?;

    // 2. Resolve inference parameters via the 4-level hierarchy.
    //    Look up the model so model-level defaults can be applied.  When the
    //    identifier is unknown (external port reuse with no catalog entry) the
    //    sampling is forwarded as-is.
    let resolved_sampling = match ctx
        .app
        .models()
        .find_by_identifier(&params.model_identifier)
        .await
        .ok()
    {
        Some(model) => {
            Some(resolve_inference_config(ctx, sampling.unwrap_or_default(), &model).await?)
        }
        None => sampling,
    };

    // 3. Initialise MCP servers (CLI bootstrap intentionally skips this).
    //    A failure is logged as a warning rather than aborting the session:
    //    the agent can still run without tools.
    if let Err(e) = ctx.mcp.initialize().await {
        tracing::warn!("MCP initialisation failed — tools may be unavailable: {e}");
    }
    // Pre-warm lazy servers so they are ready before the first agent iteration.
    ctx.mcp.prewarm_lazy().await;

    // 4. Compose the agent loop.  When tools are specified the loop is
    //    restricted to the named allowlist; otherwise all MCP tools are visible.
    let tool_filter = if params.tools.is_empty() {
        None
    } else {
        Some(params.tools.iter().cloned().collect())
    };
    let base_url = format!("http://127.0.0.1:{port}");
    let model_context =
        request_pipeline::resolve(ctx.catalog.as_ref(), Some(&params.model_identifier)).await;
    let agent = compose_agent_loop_with_sampling(
        base_url,
        ctx.http_client.clone(),
        params.model_name.clone(),
        model_context,
        Arc::clone(&ctx.mcp),
        tool_filter,
        sandbox_root,
        resolved_sampling,
        // No proxy dashboard in the CLI process — nowhere to report reuse.
        None,
        Some(params.retry_policy),
    );

    Ok(agent)
}

// =============================================================================
// Private helpers
// =============================================================================

/// Resolve the llama-server port for this session.
///
/// A caller-supplied `--port` is used as-is (externally managed server).
/// Otherwise the daemon — the one process that owns llama-server — is asked
/// to start (or reuse) the model, and the daemon keeps owning it after this
/// session ends.
async fn resolve_port(
    ctx: &CliContext,
    params: &AgentSessionParams,
    banner: &BannerInfo,
) -> Result<u16> {
    if let Some(port) = params.port {
        tracing::debug!("reusing user-supplied llama-server on port {port}");
        return Ok(port);
    }

    // Look up the model so the context flag can resolve against its metadata.
    let model = ctx
        .app
        .models()
        .find_by_identifier(&params.model_identifier)
        .await
        .context("failed to look up model")?;

    // Resolve the per-request context tier here (this is what makes
    // `--ctx-size max` work); the daemon applies the per-model and global
    // tiers itself, exactly as it does for every other start request.
    let ctx_arg = parse_ctx_size_flag(params.ctx_size.as_deref())?;
    let context_length = ctx_arg.and_then(|arg| arg.resolve(model.context_length));

    if !banner.quiet {
        style::print_info_banner("Info", "\u{2139}\u{fe0f}");
        eprintln!(
            "  Starting llama-server for '{}' via the gglib daemon (this may take a moment) \u{2026}",
            model.name
        );
    }

    let handle = crate::daemon_client::ensure_daemon().await?;
    let started = handle
        .start_model_server(model.id, context_length)
        .await
        .context("failed to start llama-server via the daemon")?;

    if !banner.quiet {
        eprintln!("  llama-server ready on port {}", started.port);

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

    Ok(started.port)
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
}
