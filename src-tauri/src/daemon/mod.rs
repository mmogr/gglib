//! The desktop app's connection to the gglib daemon.
//!
//! The daemon — not this app — owns llama-server. On startup the app probes
//! the fixed daemon port; if nothing answers it launches `gglib daemon run`
//! detached when a CLI binary can be found, and otherwise hosts the same
//! daemon composition in-process as a fallback (bundle-only installs). The
//! in-process fallback still goes through the daemon's file lock, so it
//! loses gracefully to an external daemon that starts first.
//!
//! Everything the app needs from the backend goes through the daemon's HTTP
//! API from here on; this module is the one place that knows the base URL.
//!
//! [`snapshot`] is what the daemon reports, and [`watch`] is the one task that
//! keeps it current.

mod snapshot;
mod watch;

pub use snapshot::DaemonSnapshot;
pub use watch::{Refresh, spawn as watch};

use std::time::Duration;

use gglib_app_services::types::AppSettings;
use gglib_core::DAEMON_PORT;
use tracing::{error, info, warn};

/// One identity probe's budget.
const PROBE_TIMEOUT: Duration = Duration::from_millis(500);

/// How long to wait for a launched (or in-process) daemon to come up.
const LAUNCH_WAIT: Duration = Duration::from_secs(15);

/// How this app came by the daemon it is talking to.
///
/// The distinction decides what quitting is allowed to take down with it, and
/// [`Daemon::connect_or_launch`] already knows it — it used to be flattened
/// into a single "hosted" bool, which could not tell a daemon we started from
/// one that was already serving somebody else.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Ownership {
    /// Already answering when we probed. Someone else's — a CLI session's, or
    /// an earlier app instance's — so this app does not get to end it.
    Adopted,
    /// We spawned `gglib daemon run` detached, so it is ours to stop.
    Launched,
    /// Running inside this process (bundle-only fallback). Its lifetime is
    /// this app's whether we like it or not.
    Hosted,
    /// Nothing was reachable when the app started, and nothing has been
    /// claimed since. The app runs anyway — see [`Daemon::disconnected`].
    Unresolved,
}

impl Ownership {
    /// Whether quitting this app should take the daemon with it.
    ///
    /// A daemon this app started or hosts is this app's to end. One that was
    /// already answering when we probed belongs to whoever started it — a CLI
    /// session, an earlier instance — and quitting a dashboard is no reason to
    /// take their endpoint away.
    #[must_use]
    pub const fn ends_with_the_app(self) -> bool {
        matches!(self, Self::Launched | Self::Hosted)
    }
}

/// A connected daemon.
pub struct Daemon {
    /// Shared HTTP client for daemon calls.
    pub client: reqwest::Client,
    /// How this app came by it — see [`Ownership`].
    pub ownership: Ownership,
}

/// The daemon's base URL — fixed loopback port by design.
pub fn base_url() -> String {
    format!("http://127.0.0.1:{DAEMON_PORT}")
}

/// What answered on the daemon port.
enum Probe {
    Running,
    NotRunning,
    Foreign,
}

async fn probe(client: &reqwest::Client) -> Probe {
    let url = format!("{}/health", base_url());
    let Ok(response) = client.get(&url).timeout(PROBE_TIMEOUT).send().await else {
        return Probe::NotRunning;
    };
    match response.json::<serde_json::Value>().await {
        Ok(body) if body.get("service").and_then(|s| s.as_str()) == Some("gglib-daemon") => {
            Probe::Running
        }
        _ => Probe::Foreign,
    }
}

impl Daemon {
    /// A client for a daemon that is not there.
    ///
    /// The app used to `expect` its way past a failed connection, so a port
    /// 9887 held by another program, or a `gglib daemon run` that died on
    /// startup, killed the process before `setup_app` had built a tray or
    /// shown a window: gglib simply never appeared, and the reason was in a
    /// log file. Coming up disconnected instead means the tray exists, reads
    /// "not running", and offers Start gglib Service — which is the state that
    /// affordance was added for.
    ///
    /// Every call made through this fails until a daemon answers, which is
    /// exactly what the watcher reports as unreachable.
    #[must_use]
    pub fn disconnected() -> Self {
        Self {
            client: reqwest::Client::new(),
            ownership: Ownership::Unresolved,
        }
    }

    /// Find the daemon, launching or hosting one if nothing is running.
    ///
    /// # Errors
    ///
    /// Fails when the port is held by a foreign program, or when neither an
    /// external launch nor the in-process fallback becomes healthy in time.
    pub async fn connect_or_launch() -> Result<Self, String> {
        let client = reqwest::Client::new();

        match probe(&client).await {
            Probe::Running => {
                info!("connected to running gglib daemon");
                return Ok(Self {
                    client,
                    ownership: Ownership::Adopted,
                });
            }
            Probe::Foreign => {
                return Err(format!(
                    "port {DAEMON_PORT} is held by another program (not a gglib daemon)"
                ));
            }
            Probe::NotRunning => {}
        }

        // Prefer an external daemon: it survives a crash of this app, and it
        // is the same process the CLI would have started.
        let ownership = match spawn_external_daemon() {
            Ok(()) => {
                info!("launched external gglib daemon");
                Ownership::Launched
            }
            Err(e) => {
                warn!("no external gglib binary ({e}); hosting the daemon in-process");
                tauri::async_runtime::spawn(async {
                    if let Err(e) =
                        gglib_axum::daemon::run_daemon(gglib_axum::daemon::DaemonOptions::default())
                            .await
                    {
                        error!("in-process daemon exited with error: {e}");
                    }
                });
                Ownership::Hosted
            }
        };

        wait_until_healthy(&client).await?;

        Ok(Self { client, ownership })
    }

    /// Start a daemon again after one has been stopped.
    ///
    /// The tray's Start gglib Service. Only ever launches an external daemon:
    /// the in-process fallback is a one-shot at startup, and hosting a second
    /// one inside a process that may still be unwinding the first is not worth
    /// the failure modes.
    ///
    /// [`Ownership`] is deliberately not revised. An app that adopted
    /// somebody's daemon, watched it stop, and started a new one keeps erring
    /// toward leaving it alone at quit, which is the safe direction to be
    /// wrong in.
    ///
    /// # Errors
    ///
    /// Fails when one is already running, when the port is held by a foreign
    /// program, when no `gglib` binary can be found, or when the launched
    /// daemon does not become healthy in time.
    pub async fn restart(&self) -> Result<(), String> {
        match probe(&self.client).await {
            Probe::Running => return Err("the gglib daemon is already running".to_owned()),
            Probe::Foreign => {
                return Err(format!(
                    "port {DAEMON_PORT} is held by another program (not a gglib daemon)"
                ));
            }
            Probe::NotRunning => {}
        }

        spawn_external_daemon()?;
        info!("relaunched external gglib daemon");

        wait_until_healthy(&self.client).await
    }

    /// GET a JSON value from the daemon.
    pub async fn get_json(&self, path: &str) -> Result<serde_json::Value, String> {
        let response = self
            .client
            .get(format!("{}{path}", base_url()))
            .timeout(Duration::from_secs(10))
            .send()
            .await
            .map_err(|e| e.to_string())?;
        response.json().await.map_err(|e| e.to_string())
    }

    /// POST to the daemon, returning the JSON response.
    pub async fn post_json(
        &self,
        path: &str,
        body: &serde_json::Value,
    ) -> Result<serde_json::Value, String> {
        let response = self
            .client
            .post(format!("{}{path}", base_url()))
            .json(body)
            .timeout(Duration::from_secs(30))
            .send()
            .await
            .map_err(|e| e.to_string())?;
        let status = response.status();
        let value: serde_json::Value = response.json().await.unwrap_or_default();
        if status.is_success() {
            Ok(value)
        } else {
            Err(value
                .get("error")
                .and_then(|e| e.as_str())
                .map_or_else(|| format!("daemon answered {status}"), String::from))
        }
    }

    /// Read the stored settings through the daemon.
    pub async fn settings(&self) -> Result<AppSettings, String> {
        let value = self.get_json("/api/config/settings").await?;
        serde_json::from_value(value).map_err(|e| e.to_string())
    }

    /// Ask the daemon to shut down.
    ///
    /// Returns as soon as the request is accepted; the daemon's own ordered
    /// teardown — proxy drained, every llama-server stopped gracefully,
    /// downloads cancelled — runs after. Pair with [`Self::wait_for_exit`]
    /// when the caller needs it finished.
    pub async fn request_shutdown(&self) {
        let _ = self
            .client
            .post(format!("{}/api/daemon/shutdown", base_url()))
            .timeout(Duration::from_secs(5))
            .send()
            .await;
    }

    /// Wait (bounded) for the daemon to stop answering.
    pub async fn wait_for_exit(&self, budget: Duration) {
        let deadline = tokio::time::Instant::now() + budget;
        while tokio::time::Instant::now() < deadline {
            if matches!(probe(&self.client).await, Probe::NotRunning) {
                return;
            }
            tokio::time::sleep(Duration::from_millis(250)).await;
        }
    }
}

/// Poll until a just-launched daemon answers, or the launch budget runs out.
async fn wait_until_healthy(client: &reqwest::Client) -> Result<(), String> {
    let deadline = tokio::time::Instant::now() + LAUNCH_WAIT;

    loop {
        tokio::time::sleep(Duration::from_millis(250)).await;

        match probe(client).await {
            Probe::Running => return Ok(()),
            Probe::Foreign => {
                return Err(format!(
                    "port {DAEMON_PORT} was taken by another program during startup"
                ));
            }
            Probe::NotRunning => {}
        }

        if tokio::time::Instant::now() >= deadline {
            return Err(format!(
                "the gglib daemon did not come up within {}s",
                LAUNCH_WAIT.as_secs()
            ));
        }
    }
}

/// Spawn `gglib daemon run` detached: sibling binary first, then `$PATH`.
fn spawn_external_daemon() -> Result<(), String> {
    let candidate = std::env::current_exe()
        .ok()
        .and_then(|exe| exe.parent().map(|dir| dir.join("gglib")))
        .filter(|p| p.exists());

    let program = match candidate {
        Some(path) => path,
        None => which_gglib().ok_or("no `gglib` binary next to the app or on PATH")?,
    };

    let log = daemon_log_file().map_err(|e| format!("opening daemon log: {e}"))?;

    let mut cmd = std::process::Command::new(program);
    cmd.args(["daemon", "run"])
        .stdin(std::process::Stdio::null())
        .stdout(log.try_clone().map_err(|e| e.to_string())?)
        .stderr(log);

    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt;
        cmd.process_group(0);
    }

    cmd.spawn().map_err(|e| e.to_string())?;
    Ok(())
}

/// Locate `gglib` on `$PATH` without shelling out.
fn which_gglib() -> Option<std::path::PathBuf> {
    let paths = std::env::var_os("PATH")?;
    std::env::split_paths(&paths)
        .map(|dir| dir.join("gglib"))
        .find(|candidate| candidate.is_file())
}

/// The log file an auto-launched daemon writes to.
fn daemon_log_file() -> std::io::Result<std::fs::File> {
    let dir = gglib_core::paths::data_root()
        .map_err(|e| std::io::Error::other(e.to_string()))?
        .join("logs");
    std::fs::create_dir_all(&dir)?;
    std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(dir.join("daemon.log"))
}
