//! The llama-server a local agent session talks to.
//!
//! Two answers to one question: `--port` names a llama-server already
//! running here, and nothing named asks the daemon to start the model here.
//! Both resolve to a loopback port with no credential. The third answer —
//! the machine on the other end of `gglib remote join` — is
//! [`Target`](crate::target::Target)'s, and lives with the other decisions
//! that depend on which machine a turn runs on.

use anyhow::{Context as _, Result};
use gglib_app_services::types::StartServerRequest;
use gglib_core::server_config::parse_ctx_size_flag;

use super::config::{AgentSessionParams, BannerInfo};
use crate::bootstrap::CliContext;
use crate::daemon_client::{self, StartServerBody};
use crate::handlers::model::resolver;
use crate::presentation::style;
use gglib_core::domain::{InferenceConfig, Model, ModelAction};

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
    let model = resolver::resolve_for(ctx, &params.model_identifier, ModelAction::Chat).await?;
    let body = start_body(&model, params.ctx_size.as_deref())?;

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
        .start_model_server(&body)
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

/// What the daemon is asked to start for a session on `model`, at the context
/// `--ctx-size` names.
///
/// Only that tier is resolved here, which is what makes `--ctx-size max` work:
/// it needs the model's own context length. The daemon applies the per-model
/// and global tiers itself, as it does for every other start request.
fn start_body(model: &Model, ctx_size: Option<&str>) -> Result<StartServerBody> {
    let ctx_arg = parse_ctx_size_flag(ctx_size)?;
    Ok(StartServerBody {
        id: model.id,
        config: StartServerRequest {
            context_length: ctx_arg.and_then(|arg| arg.resolve(model.context_length)),
            ..Default::default()
        },
    })
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

#[cfg(test)]
#[path = "upstream_tests.rs"]
mod tests;
