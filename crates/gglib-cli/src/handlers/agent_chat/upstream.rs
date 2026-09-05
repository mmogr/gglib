//! Which machine an agent session talks to.
//!
//! Three answers to one question. `--port` names a llama-server already
//! running here; nothing named asks the daemon to start the model here; and
//! `--remote` names the machine on the other end of `gglib remote connect`
//! (ADR 0012). The first two resolve to a loopback port with no credential.
//! The third resolves to the tunnel's loopback port *with* one: that port is
//! the far machine's proxy, its listener injects nothing, and the key this
//! machine received when it paired is what gets a request through.

use anyhow::{Context as _, Result, anyhow};
use gglib_core::server_config::parse_ctx_size_flag;

use super::config::{AgentSessionParams, BannerInfo};
use crate::bootstrap::CliContext;
use crate::daemon_client::{self, DaemonProbe};
use crate::presentation::style;
use gglib_core::domain::InferenceConfig;

/// Where the completion adapter points, and with what.
pub(crate) struct Upstream {
    /// `http://127.0.0.1:<port>`, without the `/v1` — the adapter adds it.
    pub base_url: String,
    /// The far machine's key on the remote path; nothing for a local server.
    pub bearer: Option<String>,
}

/// Resolve the upstream for this session.
pub(crate) async fn resolve(
    ctx: &CliContext,
    params: &AgentSessionParams,
    banner: &BannerInfo,
) -> Result<Upstream> {
    if params.remote {
        return remote(ctx, banner).await;
    }
    let port = resolve_port(ctx, params, banner).await?;
    Ok(Upstream {
        base_url: format!("http://127.0.0.1:{port}"),
        bearer: None,
    })
}

/// The machine on the other end of the tunnel, as the daemon reports it.
async fn remote(ctx: &CliContext, banner: &BannerInfo) -> Result<Upstream> {
    let client = reqwest::Client::new();
    if !matches!(daemon_client::probe(&client).await, DaemonProbe::Running) {
        anyhow::bail!(
            "--remote needs the daemon running and connected to the other machine: \
             `gglib remote connect` first"
        );
    }
    let handle = daemon_client::DaemonHandle { client };
    let status = handle.remote_status().await?;
    let Some(connection) = status.connected else {
        anyhow::bail!(
            "not connected to a remote machine — `gglib remote connect [<ticket>-<code>]` first"
        );
    };
    let key = ctx
        .app
        .settings()
        .get()
        .await
        .map_err(|e| anyhow!("failed to load settings: {e}"))?
        .remote_api_key
        .filter(|k| !k.trim().is_empty())
        .ok_or_else(|| {
            anyhow!(
                "connected to a remote machine, but this one holds no key for it — pair again \
                 with the full `<ticket>-<code>` string from `gglib remote enable` there"
            )
        })?;

    if !banner.quiet {
        style::print_info_banner("Info", "\u{2139}\u{fe0f}");
        eprintln!(
            "  Asking the remote machine {} at {} ({})",
            connection.ticket_fingerprint, connection.base_url, connection.path
        );
        if let Some(ref s) = banner.sampling {
            print_sampling_lines(s);
        }
        style::print_banner_close();
    }

    Ok(Upstream {
        base_url: server_root(&connection.base_url),
        bearer: Some(key),
    })
}

/// `http://127.0.0.1:41234/v1` → `http://127.0.0.1:41234`.
///
/// The daemon reports the URL a client pastes, with the `/v1`; the adapter
/// builds `/v1/chat/completions` from the server root itself.
fn server_root(base_url: &str) -> String {
    let trimmed = base_url.trim_end_matches('/');
    trimmed.strip_suffix("/v1").unwrap_or(trimmed).to_owned()
}

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

#[cfg(test)]
mod tests {
    use super::server_root;

    #[test]
    fn the_v1_suffix_the_daemon_reports_is_removed_once() {
        assert_eq!(
            server_root("http://127.0.0.1:41234/v1"),
            "http://127.0.0.1:41234"
        );
        assert_eq!(
            server_root("http://127.0.0.1:41234/v1/"),
            "http://127.0.0.1:41234"
        );
        assert_eq!(
            server_root("http://127.0.0.1:41234"),
            "http://127.0.0.1:41234"
        );
    }
}
