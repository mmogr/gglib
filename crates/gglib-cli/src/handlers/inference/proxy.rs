//! Proxy command handler.
//!
//! `gglib proxy` asks the daemon to start the unpinned proxy — the
//! counterpart to [`serve`](super::serve), which starts the same proxy
//! pinned to one model. The daemon owns the process; this command starts
//! it, then attaches the live dashboard. Ctrl-C detaches and leaves the
//! endpoint serving — `gglib proxy stop` is what stops it.

use anyhow::Result;

use crate::bootstrap::CliContext;
use crate::daemon_client::{self, StartProxyBody};
use crate::shared_args::{AccessArgs, CacheArgs, SamplingArgs};
use gglib_core::server_config::{ServerConfigOptions, resolve_context_size};

/// Execute the proxy command.
///
/// Ensures the daemon is running, starts the proxy on it (idempotent), and
/// attaches the dashboard until Ctrl-C.
#[allow(clippy::too_many_arguments)]
pub(crate) async fn execute(
    ctx: &CliContext,
    host: String,
    port: u16,
    default_context: Option<String>,
    sampling: SamplingArgs,
    cache: CacheArgs,
    access: AccessArgs,
) -> Result<()> {
    let settings = ctx.app.settings().get().await?;
    let effective_context = resolve_context_size(&ServerConfigOptions {
        context_size: default_context
            .as_deref()
            .and_then(|s| s.parse::<u64>().ok()),
        global_default_ctx: settings.default_context_size,
        ..Default::default()
    });

    let handle = daemon_client::ensure_daemon().await?;
    let status = handle
        .start_proxy(&StartProxyBody {
            host: Some(host.clone()),
            port: Some(port),
            default_context: Some(effective_context),
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
        })
        .await?;

    let proxy_port = status.port.unwrap_or(port);
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
    let key = match api_key_flag {
        Some(flag) => Some(flag),
        None => ctx
            .app
            .settings()
            .get()
            .await
            .ok()
            .and_then(|s| s.proxy_api_key)
            .filter(|k| !k.trim().is_empty()),
    };

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
pub(crate) async fn stop() -> Result<()> {
    let client = reqwest::Client::new();
    match daemon_client::probe(&client).await {
        daemon_client::DaemonProbe::Running => {}
        _ => {
            eprintln!("  Daemon is not running \u{2014} no proxy to stop.");
            return Ok(());
        }
    }
    let handle = daemon_client::DaemonHandle { client };
    let status = handle.stop_proxy().await?;
    if status.running {
        anyhow::bail!("the daemon reported the proxy still running after stop");
    }
    eprintln!("  Proxy stopped. (The daemon keeps running: `gglib daemon stop` ends it.)");
    Ok(())
}
