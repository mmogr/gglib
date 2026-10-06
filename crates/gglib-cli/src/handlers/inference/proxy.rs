//! Proxy command handler.
//!
//! `gglib proxy` asks the daemon to start the unpinned proxy — the
//! counterpart to [`serve`](super::serve), which starts the same proxy
//! pinned to one model. The daemon owns the process; this command starts
//! it, then attaches the live dashboard. Ctrl-C detaches and leaves the
//! endpoint serving — `gglib proxy stop` is what stops it.

use anyhow::{Context, Result};

use crate::bootstrap::CliContext;
use crate::daemon_client::{self, DaemonHandle, StartProxyBody};
use crate::shared_args::{AccessArgs, CacheArgs, SamplingArgs};
use gglib_core::settings::CONTEXT_SIZE_RANGE;

/// The context the daemon should serve when a client names none.
///
/// Passed through, not resolved. Resolving the chain here would turn "the user
/// set nothing" into "the user set 4096" and send it as an explicit value, so
/// the daemon's own `BuiltInDefault -> None` filter could not see that nobody
/// had chosen it, and the launch would never reach the rung that fits the
/// context to the machine. `up`, `serve` and the benchmark harness
/// (`benchmark/{agentic,compare,tune}` in gglib-app-services) pass the setting
/// through too, and `scripts/check_context_floor.sh` fails on an
/// `.unwrap_or(DEFAULT_CONTEXT_SIZE)` in any of them.
///
/// The flag is validated rather than silently discarded: a typo is an error,
/// not a different context served without a word. `CtxSizeArg::parse` is
/// deliberately *not* reused: it advertises "a positive number or 'max'", and
/// `max` has no meaning for a proxy that serves every model and therefore has
/// no single trained context in scope. Pointing the user at a value this
/// command rejects one line later would be worse than no hint.
fn resolve_default_context(
    flag: Option<&str>,
    settings: &gglib_core::Settings,
) -> Result<Option<u64>> {
    let Some(raw) = flag else {
        return Ok(settings.default_context_size);
    };
    let trimmed = raw.trim();
    let invalid = || {
        format!(
            "Invalid --default-context '{trimmed}'. Use a number from {} to {}; 'max' is not \
             supported here because `gglib proxy` serves every model, so no single trained \
             context is in scope. Omit the flag to fall back to the app settings \
             `default_context_size`, or, with that unset too, to per-launch sizing.",
            CONTEXT_SIZE_RANGE.start(),
            CONTEXT_SIZE_RANGE.end(),
        )
    };
    let parsed = trimmed.parse::<u64>().with_context(invalid)?;
    // Bounded here rather than left to the daemon. This is the same value
    // `validate_settings` holds to `CONTEXT_SIZE_RANGE` and the same one
    // `--default-context-size` documents with that range, so accepting `1` on
    // this surface alone would make three descriptions of one number disagree.
    if !CONTEXT_SIZE_RANGE.contains(&parsed) {
        anyhow::bail!(invalid());
    }
    Ok(Some(parsed))
}

/// What `gglib proxy` asks the daemon to start.
///
/// `port` is the `--port` flag as typed. An absent flag travels as no port,
/// which the daemon resolves to the stored `proxy_port`, as it does for the
/// desktop app and the tray; a default filled in here would outrank that
/// setting.
fn start_body(
    host: String,
    port: Option<u16>,
    default_context: Option<u64>,
    sampling: SamplingArgs,
    cache: &CacheArgs,
    access: &AccessArgs,
) -> StartProxyBody {
    StartProxyBody {
        host: Some(host),
        port,
        default_context,
        cache: Some(cache.cache),
        slot_dir: cache.slot_dir.clone(),
        pinned: None,
        cache_disk_gb: cache.cache_disk_gb,
        inference_override: sampling.into_override(),
        // `gglib proxy` serves every model; a single default profile has no
        // model in scope to attach to. Its clients name `{model}:{profile}`.
        default_profile: None,
        api_key: access.api_key.clone(),
        allowed_hosts: access.allowed_hosts.clone(),
    }
}

/// Start the proxy `body` describes on the daemon, and answer the port it is
/// on.
///
/// The daemon's answer is the port: with no `--port` the daemon chose it, and
/// a proxy that was already running answers with the port it holds, whatever
/// was asked. A daemon that reports none is not serving one; what is answered
/// then is where the body asked for it, or the stored `proxy_port` the daemon
/// falls back to.
///
/// Shared by `proxy`, `serve` and `up`, so none of them has a port of its own
/// to report.
pub(in crate::handlers) async fn start_on(
    handle: &DaemonHandle,
    body: &StartProxyBody,
    settings: &gglib_core::Settings,
) -> Result<u16> {
    let status = handle.start_proxy(body).await?;
    Ok(status
        .port
        .or(body.port)
        .unwrap_or_else(|| settings.effective_proxy_port()))
}

/// Execute the proxy command.
///
/// Ensures the daemon is running, starts the proxy on it (idempotent), and
/// attaches the dashboard until Ctrl-C.
#[allow(clippy::too_many_arguments)]
pub(crate) async fn execute(
    ctx: &CliContext,
    host: String,
    port: Option<u16>,
    default_context: Option<String>,
    sampling: SamplingArgs,
    cache: CacheArgs,
    access: AccessArgs,
) -> Result<()> {
    let settings = ctx.app.settings().get().await?;
    let default_context = resolve_default_context(default_context.as_deref(), &settings)?;
    let body = start_body(host, port, default_context, sampling, &cache, &access);

    let handle = daemon_client::ensure_daemon(ctx).await?;
    let proxy_port = start_on(&handle, &body, &settings).await?;
    attach_dashboard(ctx, proxy_port, access.api_key).await
}

/// Attach the live dashboard to the running proxy, and print the detach
/// hint when the user leaves it.
///
/// Shared by `proxy`, `serve` and `up`: the daemon owns the process, so the
/// foreground command's job after starting it is to show it working.
pub(in crate::handlers) async fn attach_dashboard(
    ctx: &CliContext,
    proxy_port: u16,
    api_key_flag: Option<String>,
) -> Result<()> {
    eprintln!();
    eprintln!(
        "  Proxy running on the gglib daemon \u{2014} attaching dashboard (Ctrl-C detaches)."
    );

    // The stored key is the same row the daemon's supervisor resolves, so the
    // dashboard presents whatever the proxy demands.
    let key = daemon_client::auth::proxy_key(ctx, api_key_flag).await;

    let result =
        crate::handlers::proxy_dashboard::execute("127.0.0.1".into(), proxy_port, key.as_deref())
            .await;

    eprintln!();
    eprintln!("  Detached. The proxy is still serving on port {proxy_port}.");
    eprintln!("    re-attach:  gglib proxy dashboard --port {proxy_port}");
    eprintln!("    stop it:    gglib proxy stop");
    eprintln!();
    result
}

/// Execute `gglib proxy stop`.
pub(crate) async fn stop(ctx: &CliContext) -> Result<()> {
    let Ok(handle) = daemon_client::running(ctx).await else {
        eprintln!("  Daemon is not running \u{2014} no proxy to stop.");
        return Ok(());
    };
    let status = handle.stop_proxy().await?;
    if status.running {
        anyhow::bail!("the daemon reported the proxy still running after stop");
    }
    eprintln!("  Proxy stopped. (The daemon keeps running: `gglib daemon stop` ends it.)");
    Ok(())
}

#[cfg(test)]
#[path = "proxy_tests.rs"]
pub(in crate::handlers) mod tests;
