//! The llama-server a local agent session talks to.
//!
//! Two answers to one question: `--port` names a llama-server already
//! running here, and nothing named asks the daemon to start the model here.
//! Both resolve to a loopback port with no credential. The third answer —
//! the machine on the other end of `gglib remote connect` — is
//! [`Target`](crate::target::Target)'s, and lives with the other decisions
//! that depend on which machine a turn runs on.

use anyhow::{Context as _, Result};
use gglib_core::server_config::parse_ctx_size_flag;

use super::config::{AgentSessionParams, BannerInfo};
use crate::bootstrap::CliContext;
use crate::daemon_client;
use crate::presentation::style;
use gglib_core::domain::InferenceConfig;

/// Resolve the llama-server port for this session.
///
/// A caller-supplied `--port` is used as-is (externally managed server).
/// Otherwise the daemon — the one process that owns llama-server — is asked
/// to start (or reuse) the model, and the daemon keeps owning it after this
/// session ends.
pub(crate) async fn resolve_port(
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

    let handle =
        crate::daemon_client::ensure_daemon(daemon_client::auth::daemon_api_key(ctx).await).await?;
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
pub(crate) fn print_sampling_lines(s: &InferenceConfig) {
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
