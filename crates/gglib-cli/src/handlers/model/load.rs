//! `gglib model load` and `gglib model unload`.
//!
//! The CLI twins of the app's Load and Unload. The daemon is asked to start
//! (`POST /api/servers/start`) or stop (`POST /api/servers/stop`) a model's
//! server, the routes the app calls, and it keeps owning the process after
//! this command returns. `--pin` asks first for the endpoint pinned to the
//! model (`POST /api/proxy/start-pinned`: the app's pinned start, which runs
//! `gglib serve`'s cascade on the daemon) and then for the model, in that
//! order, since a plain start brings up an unpinned endpoint when none is
//! running. An endpoint already on otherwise is switched to the model only
//! with a yes, as `gglib serve` asks for one (`handlers::inference::switch`).
//! Every answer printed is the
//! daemon's; the pinned line lays out its status, as `daemon status` does.

use anyhow::{Context as _, Result};

use super::resolver;
use super::start_body::start_body;
use crate::bootstrap::CliContext;
use crate::daemon_client::wire::{StartPinnedBody, StopServerBody};
use crate::daemon_client::{
    self, Absent, DaemonHandle, StartProxyBody, StartServerBody, StartedOrDiffers,
};
use crate::handlers::inference::switch::{self, Consent};

/// Execute `gglib model load <id> [--context-size N|max] [--pin] [-y]`.
pub(crate) async fn load(
    ctx: &CliContext,
    identifier: &str,
    context: Option<&str>,
    pin: bool,
    yes: bool,
) -> Result<()> {
    let model = resolver::resolve_model_identifier(ctx, identifier).await?;
    // A bad `--context-size` is refused before anything is installed or asked.
    let body = start_body(&model, context)?;
    // The engine the model runs under has to be here before the daemon is
    // asked for it, as `serve` and `chat` make sure of.
    crate::handlers::config::engine::ensure_installed(model.runtime(), false).await?;
    // A pinned load starts the endpoint its own way, so a daemon launched
    // for it leaves the endpoint to it.
    let handle = if pin {
        daemon_client::ensure_daemon_for_endpoint().await?
    } else {
        daemon_client::ensure_daemon().await?
    };
    for line in load_on(&handle, &body, pin, &mut Consent::here(yes)).await? {
        eprintln!("{line}");
    }
    Ok(())
}

/// Ask the daemon, in order, for the endpoint pinned to the model when `pin`
/// and then for the model loaded. The lines are the daemon's answers, one per
/// request: the pinned status laid out, then the start's own sentence. A
/// refusal is the daemon's sentence. An endpoint on otherwise is switched to
/// the model only with `consent`; without it nothing is loaded.
pub(super) async fn load_on(
    handle: &DaemonHandle,
    body: &StartServerBody,
    pin: bool,
    consent: &mut Consent,
) -> Result<Vec<String>> {
    let mut lines = Vec::new();
    if pin {
        let asked = StartPinnedBody {
            model_id: body.id,
            options: body.config.clone(),
            proxy: StartProxyBody::default(),
        };
        let pinned = match handle
            .start_pinned_proxy_or_differs(&asked)
            .await
            .context("the daemon did not pin the endpoint")?
        {
            StartedOrDiffers::Started(status) => status,
            StartedOrDiffers::Differs(change) => {
                // A no is an error: nothing is loaded, and a script after
                // this must not go on as if it were.
                anyhow::ensure!(
                    consent.allows(&change)?,
                    "{}",
                    switch::kept(&change).trim_start()
                );
                let replacing = StartPinnedBody {
                    proxy: StartProxyBody {
                        replace: true,
                        ..StartProxyBody::default()
                    },
                    ..asked
                };
                handle
                    .start_pinned_proxy(&replacing)
                    .await
                    .context("the daemon did not pin the endpoint")?
            }
        };
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
    // A model that did not load is said in the daemon's sentence alone,
    // which already says that and why (#1959 C5).
    let started = handle.start_model_server(body).await.map_err(|e| {
        if e.is::<daemon_client::LoadFailed>() {
            e
        } else {
            e.context("the daemon did not load the model")
        }
    })?;
    lines.push(format!("  {}", started.message));
    // Loaded, with the endpoint off: why, in the status's sentence (#2047).
    lines.extend(started.endpoint_off.map(|why| format!("  {why}")));
    Ok(lines)
}

/// Execute `gglib model unload <id>`.
pub(crate) async fn unload(ctx: &CliContext, identifier: &str) -> Result<()> {
    let model = resolver::resolve_model_identifier(ctx, identifier).await?;
    let handle = match daemon_client::running().await {
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
