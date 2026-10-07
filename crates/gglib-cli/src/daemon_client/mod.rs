#![doc = include_str!("README.md")]

pub(crate) mod sse;

use std::future::Future;
use std::time::Duration;

use anyhow::{Context, Result, bail};

use gglib_core::DAEMON_PORT;

use crate::bootstrap::CliContext;

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
    #[cfg(test)]
    if let Ok(port) = STAND_IN_PORT.try_with(|port| *port) {
        return format!("http://127.0.0.1:{port}");
    }
    format!("http://127.0.0.1:{DAEMON_PORT}")
}

#[cfg(test)]
tokio::task_local! {
    /// In a test, the port of a stand-in that answers a call as the daemon
    /// would. A task that sets it sends its calls there, so a test of a call
    /// never reaches a daemon that may be running on this machine.
    pub(crate) static STAND_IN_PORT: u16;
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
    probe_within(client, PROBE_TIMEOUT).await
}

/// [`probe`], with a timeout of the caller's choosing, for the one caller
/// that acts on the daemon being absent rather than only reporting it.
#[allow(
    clippy::manual_let_else,
    reason = "grandfathered at lint inheritance, #1157"
)]
pub(crate) async fn probe_within(client: &reqwest::Client, timeout: Duration) -> DaemonProbe {
    let url = format!("{}{}", base_url(), paths::HEALTH_PATH);
    let response = match client.get(&url).timeout(timeout).send().await {
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

/// A connected daemon: the shared HTTP client plus the credential to present.
pub(crate) struct DaemonHandle {
    /// Client for talking to the daemon. No global timeout — long calls
    /// (model start) set their own.
    pub client: reqwest::Client,
    /// The bearer token `/api/*` wants, or `None` when this CLI has none to
    /// present. Resolved by [`auth::daemon_api_key`] and attached by
    /// [`DaemonHandle::request`], so no call site decides this for itself.
    pub api_key: Option<String>,
}

impl DaemonHandle {
    /// A handle over `client` carrying the credential this CLI presents
    /// ([`auth::daemon_api_key`]). It asks the daemon nothing: [`running`]
    /// and [`ensure_daemon`] are the ones that find out whether it is there.
    pub(crate) async fn new(ctx: &CliContext, client: reqwest::Client) -> Self {
        Self {
            client,
            api_key: auth::daemon_api_key(ctx).await,
        }
    }
}

/// Why [`running`] has no daemon to hand back.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Absent {
    /// Nothing is listening on the daemon's port.
    NotRunning,
    /// Something is, and it is not a gglib daemon.
    ForeignServer,
}

/// The daemon, when one is running, and never a launch. A command that only
/// reports on the daemon, or stops something on it, asks here and says in
/// its own words what [`Absent`] means for it.
pub(crate) async fn running(ctx: &CliContext) -> Result<DaemonHandle, Absent> {
    let client = gglib_proxy::loopback::client();
    match probe(&client).await {
        DaemonProbe::Running => Ok(DaemonHandle::new(ctx, client).await),
        DaemonProbe::NotRunning => Err(Absent::NotRunning),
        DaemonProbe::ForeignServer => Err(Absent::ForeignServer),
    }
}

/// Find the daemon, launching it if nothing is running, and present this
/// CLI's credential to it ([`auth::daemon_api_key`]).
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
pub(crate) async fn ensure_daemon(ctx: &CliContext) -> Result<DaemonHandle> {
    let api_key = auth::daemon_api_key(ctx).await;
    let client = gglib_proxy::loopback::client();

    match probe(&client).await {
        DaemonProbe::Running => return Ok(DaemonHandle { client, api_key }),
        DaemonProbe::ForeignServer => bail!(
            "port {DAEMON_PORT} is in use by another program (not a gglib daemon). \
             Free the port and retry."
        ),
        DaemonProbe::NotRunning => {}
    }

    let log_path = spawn_daemon().context("could not launch the gglib daemon")?;
    eprintln!("  starting gglib daemon\u{2026}");

    let prober = client.clone();
    let probe_it = || probe(&prober);
    wait_for_launch(client, api_key, &auth::Local::here(), &log_path, probe_it).await
}

/// Poll `probe` until the daemon this command launched answers, and hand back
/// a handle carrying the credential `local` gives now: the daemon minted a
/// new token as it started, after `had` was resolved.
async fn wait_for_launch<F: Future<Output = DaemonProbe>>(
    client: reqwest::Client,
    had: Option<String>,
    local: &auth::Local,
    log_path: &std::path::Path,
    mut probe: impl FnMut() -> F,
) -> Result<DaemonHandle> {
    let deadline = tokio::time::Instant::now() + LAUNCH_WAIT;
    loop {
        tokio::time::sleep(Duration::from_millis(250)).await;
        match probe().await {
            DaemonProbe::Running => {
                let api_key = local.credential(async { had }).await;
                return Ok(DaemonHandle { client, api_key });
            }
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
    // Never from a test: the binary launched would be the test harness
    // itself, which reads `daemon run` as two filters and runs those tests.
    if cfg!(test) {
        bail!("a test launches no daemon");
    }
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

pub(crate) mod auth;
mod calls;
mod remote;
mod repair;
pub(crate) mod runs;
pub(crate) mod wire;

pub(crate) use wire::{QueueDownloadBody, StartProxyBody, StartServerBody};

#[cfg(test)]
#[path = "handle_tests.rs"]
mod handle_tests;
