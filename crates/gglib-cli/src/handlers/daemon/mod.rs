#![doc = include_str!("README.md")]

use std::io::{IsTerminal as _, Write as _};

use anyhow::Result;

use crate::daemon_client::{self, Absent, DaemonProbe, ProxyStatusDto};
use crate::presentation::style;
use crate::target::Target;
use gglib_axum::{DaemonLock, DaemonOptions, SHUTDOWN_WATCHDOG, run_daemon};
use gglib_core::daemon_port;

/// Execute `gglib daemon run`: host the daemon in the foreground.
pub(crate) async fn run(allowed_hosts: Vec<String>) -> Result<()> {
    run_daemon(DaemonOptions { allowed_hosts }).await
}

/// The report's lines for the proxy: its state (a pinned one with its model,
/// `Pinned to <model>`) and, while it listens, where a client reaches it and
/// whether a request needs the key. Every word is the daemon's.
fn proxy_lines(proxy: &ProxyStatusDto) -> Vec<String> {
    let state = proxy
        .pinned_to
        .clone()
        .unwrap_or_else(|| proxy.state.to_string());
    let mut lines = vec![proxy.base_url.as_ref().map_or_else(
        || format!("  Proxy:   {state}"),
        |url| format!("  Proxy:   {state} at {url}"),
    )];
    if let Some(key) = proxy.key {
        lines.push(format!("  API key: {key}"));
    }
    lines
}

/// Execute `gglib daemon status`.
pub(crate) async fn status() -> Result<()> {
    style::print_info_banner("Daemon", "\u{2139}\u{fe0f}");
    match daemon_client::running().await {
        Ok(handle) => {
            eprintln!("  Status:  running at {}", daemon_client::base_url());
            if let Ok(dir) = gglib_core::paths::data_root()
                && let Some(holder) = DaemonLock::read_holder(&dir)
            {
                eprintln!("  PID:     {}", holder.pid);
            }
            match handle.proxy_status().await {
                Ok(proxy) => {
                    for line in proxy_lines(&proxy) {
                        eprintln!("{line}");
                    }
                }
                Err(e) => eprintln!("  Proxy:   status unavailable ({e})"),
            }
        }
        Err(Absent::NotRunning) => {
            eprintln!("  Status:  not running");
            eprintln!("  Start it with any runtime command, or `gglib daemon run`.");
        }
        Err(Absent::ForeignServer) => {
            eprintln!(
                "  Status:  port {} is held by another program (not a gglib daemon)",
                daemon_port()
            );
        }
    }
    style::print_banner_close();
    Ok(())
}

/// Execute `gglib daemon stop`: this machine's daemon, or with `--remote`
/// the paired machine's, through the tunnel.
pub(crate) async fn stop(target: Target, yes: bool) -> Result<()> {
    target
        .run(async || stop_here().await, async || stop_far(yes).await)
        .await
}

/// Request shutdown from the daemon on this machine and wait for it to land.
async fn stop_here() -> Result<()> {
    let handle = match daemon_client::running().await {
        Ok(handle) => handle,
        Err(Absent::NotRunning) => {
            eprintln!("  Daemon is not running.");
            return Ok(());
        }
        Err(Absent::ForeignServer) => anyhow::bail!(
            "port {} is held by another program (not a gglib daemon) — nothing to stop",
            daemon_port()
        ),
    };
    if !handle.shutdown_daemon().await? {
        anyhow::bail!(
            "the server on port {} refused the shutdown request \
             (not running as a daemon)",
            daemon_port()
        );
    }

    // The daemon stops its models after it stops answering, and lets go of
    // its lock only when that is done, so wait for both. Two seconds past
    // its own force-exit watchdog, which ends it whatever is left.
    let budget = SHUTDOWN_WATCHDOG + std::time::Duration::from_secs(2);
    let deadline = tokio::time::Instant::now() + budget;
    eprint!("  Stopping daemon");
    while tokio::time::Instant::now() < deadline {
        tokio::time::sleep(std::time::Duration::from_millis(500)).await;
        let lock_held = gglib_core::paths::data_root().is_ok_and(|dir| DaemonLock::is_held(&dir));
        if !lock_held
            && matches!(
                daemon_client::probe(&handle.client).await,
                DaemonProbe::NotRunning
            )
        {
            eprintln!(" \u{2014} stopped.");
            return Ok(());
        }
        eprint!(".");
    }
    eprintln!();
    anyhow::bail!(
        "the daemon accepted the shutdown request but is still stopping after {}s",
        budget.as_secs()
    )
}

/// Stop the paired machine's daemon through the tunnel, then disconnect.
///
/// Asks first, because the far side cannot be restarted from here. `--yes`
/// skips the question; so does a stdin that is not a terminal, on the theory
/// that a script passing `--remote --yes` has read the help. The stop itself
/// is this machine's daemon's to carry out: it owns the tunnel, it already
/// does this for the desktop app, and the request it sends asks the far
/// proxy to type the same word.
async fn stop_far(yes: bool) -> Result<()> {
    let Ok(handle) = daemon_client::running().await else {
        anyhow::bail!("the daemon is not running, so nothing is connected to a remote")
    };
    let status = handle.remote_status().await?;
    if status.connected.is_none() {
        anyhow::bail!("not connected to a remote \u{2014} `gglib remote join` first");
    }

    if !yes && std::io::stdin().is_terminal() && !confirm(status.paired_shown())? {
        eprintln!("  Left it running.");
        return Ok(());
    }
    handle.remote_kill().await?;
    eprintln!("{}", stopping_line(status.paired_shown()));
    eprintln!("  Nothing brings it back except someone at that machine.");
    Ok(())
}

/// What a stop of the paired machine's daemon says once it is under way,
/// naming that machine as every other surface does: by its name, never its
/// fingerprint.
fn stopping_line(machine: &str) -> String {
    format!("  \u{1f6d1} The gglib daemon on {machine} is stopping, and this side is disconnected.")
}

/// What the question says the stop will take down, and on which machine.
fn question(machine: &str) -> String {
    format!("  This stops the gglib daemon on {machine}: its proxy, its models, its downloads.")
}

/// The question, and the one answer that means yes.
fn confirm(machine: &str) -> Result<bool> {
    eprintln!("{}", question(machine));
    eprintln!("  It cannot be started again from here.");
    eprint!("  Type `shutdown` to go ahead: ");
    std::io::stderr().flush()?;
    let mut input = String::new();
    std::io::stdin().read_line(&mut input)?;
    Ok(input.trim() == "shutdown")
}

#[cfg(test)]
#[path = "mod_tests.rs"]
mod mod_tests;
