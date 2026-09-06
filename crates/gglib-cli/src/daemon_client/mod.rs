#![doc = include_str!("README.md")]

pub(crate) mod sse;

use std::time::Duration;

use anyhow::{Context, Result, bail};

use gglib_core::DAEMON_PORT;

/// How long one identity probe may take. The daemon answers `/health` from
/// memory; anything slower than this is not a healthy daemon.
const PROBE_TIMEOUT: Duration = Duration::from_millis(500);

/// How long to wait for an auto-launched daemon to come up. Startup includes
/// DB migration and the orphan sweep, so this is generous.
const LAUNCH_WAIT: Duration = Duration::from_secs(10);

/// The daemon's base URL. The port is a compile-time constant by design —
/// see `gglib_core::DAEMON_PORT`.
#[must_use]
pub(crate) fn base_url() -> String {
    format!("http://127.0.0.1:{DAEMON_PORT}")
}

/// Every daemon path this CLI calls, defined in shared vocabulary so the
/// daemon's own suite can pin them — see `gglib_core::contracts::http::daemon`.
pub(crate) use gglib_core::contracts::http::daemon as paths;

/// What answered (or didn't) on the daemon port.
#[derive(Debug)]
pub(crate) enum DaemonProbe {
    /// A gglib daemon answered with its identity marker.
    Running,
    /// Nothing is listening.
    NotRunning,
    /// Something answered, but it is not a gglib daemon.
    ForeignServer,
}

/// Identity-check the daemon port.
pub(crate) async fn probe(client: &reqwest::Client) -> DaemonProbe {
    let url = format!("{}{}", base_url(), paths::HEALTH_PATH);
    let response = match client.get(&url).timeout(PROBE_TIMEOUT).send().await {
        Ok(r) => r,
        Err(_) => return DaemonProbe::NotRunning,
    };
    match response.json::<serde_json::Value>().await {
        Ok(body) if body.get("service").and_then(|s| s.as_str()) == Some("gglib-daemon") => {
            warn_on_switch_mismatch(&body);
            warn_on_build_mismatch(&body);
            DaemonProbe::Running
        }
        _ => DaemonProbe::ForeignServer,
    }
}

/// Say so when this command's `GGLIB_DISABLE_*` switches are not the ones the
/// daemon is running with.
///
/// Only meaningful for a daemon that was *already up*: one this CLI spawns
/// inherits the environment, so the two agree by construction. A daemon
/// already running was started from some other environment, and every switch
/// set here is then silently ignored — the daemon does the work.
///
/// Printed rather than fatal. The command is still valid; it just is not
/// measuring what the operator thinks, and that is a thing to be told rather
/// than protected from.
fn warn_on_switch_mismatch(health: &serde_json::Value) {
    let daemon: Vec<String> = health
        .get("debug_switches")
        .and_then(|v| v.as_array())
        .map(|a| {
            a.iter()
                .filter_map(|v| v.as_str().map(str::to_owned))
                .collect()
        })
        .unwrap_or_default();

    let here = gglib_core::debug_switches::active();
    if let Some(message) = gglib_core::debug_switches::describe_mismatch(&here, &daemon) {
        eprintln!("  warning: {message}");
    }
}

/// Say so when the running daemon was built from different code than this
/// CLI.
///
/// `CARGO_PKG_VERSION` cannot catch this: measured live, a CLI carrying new
/// daemon routes used a same-version installed daemon and got an opaque 405.
/// Printed rather than fatal, like the switch mismatch above — minor skew is
/// routine in a dev tree — but it names the one action that resolves real
/// skew, because "405 Method Not Allowed" never will. A daemon predating the
/// fingerprint reports none, which is itself a mismatch worth naming.
fn warn_on_build_mismatch(body: &serde_json::Value) {
    let mine = gglib_build_info::FINGERPRINT;
    let theirs = body.get("fingerprint").and_then(|f| f.as_str());
    if theirs == Some(mine) {
        return;
    }
    eprintln!(
        "  note: the running daemon is a different build ({}) than this CLI ({mine}) — \
         if a command fails oddly, `gglib daemon stop` and re-run to respawn it from \
         this binary",
        theirs.unwrap_or("no fingerprint: an older build"),
    );
}

/// A connected daemon: the shared HTTP client plus the base URL.
pub(crate) struct DaemonHandle {
    /// Client for talking to the daemon. No global timeout — long calls
    /// (model start) set their own.
    pub client: reqwest::Client,
}

/// Find the daemon, launching it if nothing is running.
///
/// The launch is `current_exe() daemon run`, fully detached: its own process
/// group (so Ctrl-C on this command never reaches it), stdin closed, output
/// appended to `<data_root>/logs/daemon.log`.
///
/// # Errors
///
/// - the port is held by something that is not a gglib daemon,
/// - the daemon binary cannot be spawned,
/// - the launched daemon does not become healthy within the wait window
///   (the log file path is named in the error).
pub(crate) async fn ensure_daemon() -> Result<DaemonHandle> {
    let client = reqwest::Client::new();

    match probe(&client).await {
        DaemonProbe::Running => return Ok(DaemonHandle { client }),
        DaemonProbe::ForeignServer => bail!(
            "port {DAEMON_PORT} is in use by another program (not a gglib daemon). \
             Free the port and retry."
        ),
        DaemonProbe::NotRunning => {}
    }

    let log_path = spawn_daemon().context("could not launch the gglib daemon")?;
    eprintln!("  starting gglib daemon\u{2026}");

    let deadline = tokio::time::Instant::now() + LAUNCH_WAIT;
    loop {
        tokio::time::sleep(Duration::from_millis(250)).await;
        match probe(&client).await {
            DaemonProbe::Running => return Ok(DaemonHandle { client }),
            DaemonProbe::ForeignServer => {
                bail!("port {DAEMON_PORT} was taken by another program while the daemon started")
            }
            DaemonProbe::NotRunning => {}
        }
        if tokio::time::Instant::now() >= deadline {
            bail!(
                "the gglib daemon did not come up within {}s — check {}",
                LAUNCH_WAIT.as_secs(),
                log_path.display()
            );
        }
    }
}

/// Spawn `gglib daemon run` detached; returns the log file path.
fn spawn_daemon() -> Result<std::path::PathBuf> {
    let exe = std::env::current_exe().context("resolving the gglib binary path")?;

    let log_dir = gglib_core::paths::data_root()?.join("logs");
    std::fs::create_dir_all(&log_dir)?;
    let log_path = log_dir.join("daemon.log");
    let log = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&log_path)
        .with_context(|| format!("opening {}", log_path.display()))?;

    let mut cmd = std::process::Command::new(exe);
    cmd.args(["daemon", "run"])
        .stdin(std::process::Stdio::null())
        .stdout(log.try_clone()?)
        .stderr(log);

    // Detach from this command's process group so terminal signals (Ctrl-C)
    // sent to the foreground command never reach the daemon.
    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt;
        cmd.process_group(0);
    }

    cmd.spawn().context("spawning `gglib daemon run`")?;
    Ok(log_path)
}

mod calls;
mod remote;
pub(crate) mod wire;

pub(crate) use wire::{
    QueueDownloadBody, RemoteConnectBody, RemoteEnableBody, RemoteEnableDto, RemoteStatusDto,
    StartProxyBody,
};
