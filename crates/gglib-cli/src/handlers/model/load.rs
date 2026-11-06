//! `gglib model load` and `gglib model unload`.
//!
//! The CLI twins of the app's Load and Unload. The daemon is asked to start
//! (`POST /api/servers/start`) or stop (`POST /api/servers/stop`) a model's
//! server, the routes the app calls, and it keeps owning the process after
//! this command returns. `--pin` asks first for the endpoint pinned to the
//! model (`POST /api/proxy/start-pinned`: the app's pinned start, which runs
//! `gglib serve`'s cascade on the daemon) and then for the model, in that
//! order, since a plain start brings up an unpinned endpoint when none is
//! running, which a pinned start then refuses. Every answer printed is the
//! daemon's; the pinned line lays out its status, as `daemon status` does.

use anyhow::{Context as _, Result};

use super::resolver;
use crate::bootstrap::CliContext;
use crate::daemon_client::wire::{StartPinnedBody, StopServerBody};
use crate::daemon_client::{self, Absent, DaemonHandle, StartProxyBody, StartServerBody};
use crate::handlers::agent_chat::upstream::start_body;

/// Execute `gglib model load <id> [--context N|max] [--pin]`.
pub(crate) async fn load(
    ctx: &CliContext,
    identifier: &str,
    context: Option<&str>,
    pin: bool,
) -> Result<()> {
    let model = resolver::resolve_model_identifier(ctx, identifier).await?;
    // A bad `--context` is refused before anything is installed or asked.
    let body = start_body(&model, context)?;
    // The engine the model runs under has to be here before the daemon is
    // asked for it, as `serve` and `chat` make sure of.
    crate::handlers::config::engine::ensure_installed(ctx, model.runtime(), false).await?;
    let handle = daemon_client::ensure_daemon(ctx).await?;
    for line in load_on(&handle, &body, pin).await? {
        eprintln!("{line}");
    }
    Ok(())
}

/// Ask the daemon, in order, for the endpoint pinned to the model when `pin`
/// and then for the model loaded. The lines are the daemon's answers, one per
/// request: the pinned status laid out, then the start's own sentence. A
/// refusal is the daemon's sentence.
pub(super) async fn load_on(
    handle: &DaemonHandle,
    body: &StartServerBody,
    pin: bool,
) -> Result<Vec<String>> {
    let mut lines = Vec::new();
    if pin {
        let pinned = handle
            .start_pinned_proxy(&StartPinnedBody {
                model_id: body.id,
                options: body.config.clone(),
                proxy: StartProxyBody::default(),
            })
            .await
            .context("the daemon did not pin the endpoint")?;
        // A pinned start that answers no port left nothing listening; the
        // model is not loaded behind it.
        let Some(port) = pinned.port else {
            anyhow::bail!("the daemon answered the pinned start with no listening endpoint");
        };
        lines.push(pinned.pinned_model.map_or_else(
            || format!("  Endpoint on port {port}"),
            |model| format!("  Endpoint on port {port}, pinned to {model}"),
        ));
    }
    let started = handle
        .start_model_server(body)
        .await
        .context("the daemon did not load the model")?;
    lines.push(format!("  {}", started.message));
    Ok(lines)
}

/// Execute `gglib model unload <id>`.
pub(crate) async fn unload(ctx: &CliContext, identifier: &str) -> Result<()> {
    let model = resolver::resolve_model_identifier(ctx, identifier).await?;
    let handle = match daemon_client::running(ctx).await {
        Ok(handle) => handle,
        Err(Absent::NotRunning) => {
            eprintln!("  Daemon is not running \u{2014} nothing is loaded.");
            return Ok(());
        }
        Err(Absent::ForeignServer) => anyhow::bail!(
            "port {} is held by another program (not a gglib daemon) \u{2014} nothing to unload",
            gglib_core::daemon_port()
        ),
    };
    eprintln!("{}", unload_on(&handle, model.id).await?);
    Ok(())
}

/// Ask the daemon to stop model `id`'s server. The line is the daemon's
/// sentence; a model that is not loaded is its refusal, in its words.
pub(super) async fn unload_on(handle: &DaemonHandle, id: i64) -> Result<String> {
    let stopped = handle
        .stop_model_server(&StopServerBody { model_id: id })
        .await
        .context("the daemon did not unload the model")?;
    Ok(format!("  {stopped}"))
}

#[cfg(test)]
#[path = "load_tests.rs"]
mod tests;
