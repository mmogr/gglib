#![doc = include_str!("README.md")]

mod launch;
mod service;
mod snapshot;
mod token;
mod watch;

use launch::spawn_external_daemon;
pub(crate) use service::{Launch, Service, Started};
pub(crate) use snapshot::DaemonSnapshot;
pub(crate) use token::daemon_token;
pub(crate) use watch::{Refresh, spawn as watch};

use std::time::Duration;

use gglib_app_services::types::AppSettings;
use gglib_core::daemon_port;
use tauri::Manager;
use tracing::{error, info, warn};

/// One identity probe's budget.
const PROBE_TIMEOUT: Duration = Duration::from_millis(500);

/// How long to wait for a launched (or in-process) daemon to come up.
///
/// With the first probe before it and the last poll after it, a launch
/// settles a little over this after it begins (about 16.25 s at most).
pub(crate) const LAUNCH_WAIT: Duration = Duration::from_secs(15);

/// How this app came by the daemon it is talking to.
///
/// The distinction decides what quitting is allowed to take down with it, and
/// [`Daemon::connect_or_launch`] already knows it. A single "hosted" bool
/// could not tell a daemon we started from one that was already serving
/// somebody else.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Ownership {
    /// Already answering when we probed. Someone else's — a CLI session's, or
    /// an earlier app instance's — so this app does not get to end it.
    Adopted,
    /// We spawned `gglib daemon run` detached, so it is ours to stop.
    Launched,
    /// Running inside this process (bundle-only fallback). Its lifetime is
    /// this app's whether we like it or not.
    Hosted,
    /// Not known yet (a launch is in flight), or the launch failed and
    /// nothing has been claimed since. The app runs anyway, and a daemon
    /// found later ([`Service::found`]) stays unresolved: not this app's to
    /// stop.
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
    pub(crate) const fn ends_with_the_app(self) -> bool {
        matches!(self, Self::Launched | Self::Hosted)
    }
}

/// The app's daemon: the client it is reached through, and whether it is
/// known yet.
pub(crate) struct Daemon {
    /// Shared HTTP client for daemon calls.
    pub client: reqwest::Client,
    /// Whether a launch is in flight, and once not, how it ended and how
    /// this app came by the daemon (see [`Ownership`]).
    pub service: Service,
}

/// The daemon's base URL, on the loopback port `gglib_core::daemon_port`
/// names.
pub(crate) fn base_url() -> String {
    format!("http://127.0.0.1:{}", daemon_port())
}

/// What answered on the daemon port.
enum Probe {
    Running,
    NotRunning,
    Foreign,
}

/// How long a window's question of whether the daemon answers may take:
/// what the page's own health probe is given (`HEALTH_LIMIT_MS`).
const ANSWERS_TIMEOUT: Duration = Duration::from_millis(1500);

async fn probe(client: &reqwest::Client) -> Probe {
    probe_within(client, PROBE_TIMEOUT).await
}

async fn probe_within(client: &reqwest::Client, timeout: Duration) -> Probe {
    let url = format!("{}/health", base_url());
    let Ok(response) = client.get(&url).timeout(timeout).send().await else {
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
    /// The app's daemon before anything is known about it, and the launch
    /// that will find out ([`Self::connect_or_launch`]).
    ///
    /// Needs nothing a window, the tray or the daemon provides, so the state
    /// holding it is managed before any of them exists.
    #[must_use]
    pub(crate) fn new() -> (Self, Launch) {
        let (service, launch) = Service::launching();
        let daemon = Self {
            client: gglib_proxy::loopback::client(),
            service,
        };
        (daemon, launch)
    }

    /// How this app came by its daemon, once the launch in flight (if any)
    /// has settled, whether or not it came up: a failed Start gglib keeps
    /// the ownership of the daemon it replaced. The wait is the launch's own
    /// budget at most.
    pub(crate) async fn ownership(&self) -> Ownership {
        self.service.ownership().await
    }

    /// Find the daemon, launching or hosting one if nothing is running, and
    /// settle `launch` with how that ended.
    ///
    /// A daemon that will not start is a state, not a crash: the window and
    /// the tray are already up, the page is told why, and the tray offers
    /// Start gglib.
    ///
    /// # Errors
    ///
    /// Fails when the port is held by a foreign program, or when neither an
    /// external launch nor the in-process fallback becomes healthy in time.
    pub(crate) async fn connect_or_launch(&self, launch: Launch) -> Result<Ownership, String> {
        let outcome = self.find_or_start().await;
        launch.settle(outcome.clone());
        outcome
    }

    async fn find_or_start(&self) -> Result<Ownership, String> {
        let client = &self.client;

        match probe(client).await {
            Probe::Running => {
                info!("connected to running gglib daemon");
                return Ok(Ownership::Adopted);
            }
            Probe::Foreign => {
                return Err(format!(
                    "port {} is held by another program (not a gglib daemon)",
                    daemon_port()
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
                        gglib_axum::run_daemon(gglib_axum::DaemonOptions::default()).await
                    {
                        error!("in-process daemon exited with error: {e}");
                    }
                });
                Ownership::Hosted
            }
        };

        wait_until_healthy(client).await?;

        Ok(ownership)
    }

    /// Start a daemon again after one has been stopped.
    ///
    /// The tray's Start gglib. Only ever launches an external daemon:
    /// the in-process fallback is a one-shot at startup, and hosting a second
    /// one inside a process that may still be unwinding the first is not worth
    /// the failure modes.
    ///
    /// [`Ownership`] is deliberately not revised. An app that adopted
    /// somebody's daemon, watched it stop, and started a new one keeps erring
    /// toward leaving it alone at quit, which is the safe direction to be
    /// wrong in.
    ///
    /// While it runs, the page's question of where the daemon is waits for
    /// the new one. A start asked for while a launch is already in flight
    /// (the app's own, or another Start gglib) starts nothing and answers
    /// with that launch's outcome, as [`Started::Joined`]: that launch's
    /// owner reports its failure, so one failure is reported once.
    ///
    /// It fails when one is already running, when the port is held by a
    /// foreign program, when no `gglib` binary can be found, or when the
    /// launched daemon does not become healthy in time.
    pub(crate) async fn restart(&self) -> Started {
        match self.service.begin() {
            Some(launch) => Started::Ran(self.relaunch(launch).await),
            None => self.service.join().await,
        }
    }

    /// Launch an external daemon again, and settle `launch` with how it went.
    async fn relaunch(&self, launch: Launch) -> Result<(), String> {
        let keeps = launch.before();

        match probe(&self.client).await {
            Probe::Running => {
                // It is there: the page may go on asking it.
                launch.settle(Ok(keeps));
                return Err("the gglib daemon is already running".to_owned());
            }
            Probe::Foreign => {
                let refusal = format!(
                    "port {} is held by another program (not a gglib daemon)",
                    daemon_port()
                );
                launch.settle(Err(refusal.clone()));
                return Err(refusal);
            }
            Probe::NotRunning => {}
        }

        let started = match spawn_external_daemon() {
            Ok(()) => {
                info!("relaunched external gglib daemon");
                wait_until_healthy(&self.client).await
            }
            Err(e) => Err(e),
        };
        launch.settle(started.clone().map(|()| keeps));
        started
    }

    /// Whether gglib's daemon answers its health probe now.
    ///
    /// Asked for a window, over this process's own connection: a window's
    /// `fetch` waits its turn behind the page's other requests to the daemon,
    /// of which the `WebView` sends six at a time, and a probe left waiting
    /// there reads a healthy daemon as gone.
    pub(crate) async fn answers(&self) -> bool {
        matches!(
            probe_within(&self.client, ANSWERS_TIMEOUT).await,
            Probe::Running
        )
    }

    /// GET a JSON value from the daemon.
    pub(crate) async fn get_json(&self, path: &str) -> Result<serde_json::Value, String> {
        let response = self
            .request(reqwest::Method::GET, path)
            .timeout(Duration::from_secs(10))
            .send()
            .await
            .map_err(|e| e.to_string())?;
        response.json().await.map_err(|e| e.to_string())
    }

    /// POST to the daemon, returning the JSON response.
    pub(crate) async fn post_json(
        &self,
        path: &str,
        body: &serde_json::Value,
    ) -> Result<serde_json::Value, String> {
        let response = self
            .request(reqwest::Method::POST, path)
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
    pub(crate) async fn settings(&self) -> Result<AppSettings, String> {
        let value = self.get_json("/api/config/settings").await?;
        serde_json::from_value(value).map_err(|e| e.to_string())
    }

    /// Ask the daemon to shut down.
    ///
    /// Returns as soon as the request is accepted; the daemon's own ordered
    /// teardown — proxy drained, every llama-server stopped gracefully,
    /// downloads cancelled — runs after. Pair with [`Self::wait_for_exit`]
    /// when the caller needs it finished.
    pub(crate) async fn request_shutdown(&self) {
        let _ = self
            .request(reqwest::Method::POST, "/api/daemon/shutdown")
            .timeout(Duration::from_secs(5))
            .send()
            .await;
    }

    /// Wait (bounded) for the daemon to finish stopping: not answering, and
    /// its lock released. It closes its listener before its teardown starts,
    /// and lets go of the lock only once that teardown is done or it exits.
    pub(crate) async fn wait_for_exit(&self, budget: Duration) {
        let deadline = tokio::time::Instant::now() + budget;
        while tokio::time::Instant::now() < deadline {
            if !lock_is_held() && matches!(probe(&self.client).await, Probe::NotRunning) {
                return;
            }
            tokio::time::sleep(Duration::from_millis(250)).await;
        }
    }
}

/// Run the app's first launch of its daemon beside the window, not before it.
///
/// Called from `setup` once the tray exists, so a failure is reported
/// alongside the Start gglib entry that acts on it. Until it settles, the
/// page's question of where the daemon is waits ([`Service`]).
pub(crate) fn start(app: &tauri::AppHandle, launch: Launch) {
    let app = app.clone();
    tauri::async_runtime::spawn(async move {
        let state = app.state::<crate::app::AppState>();
        let outcome = state.daemon.connect_or_launch(launch).await;
        match &outcome {
            Ok(ownership) => info!(?ownership, "daemon connection established"),
            Err(e) => error!(error = %e, "Could not reach or start the gglib daemon"),
        }
        // Not while quitting: the user quit during the launch.
        let failure =
            Started::Ran(outcome.map(drop)).failure_to_report(crate::lifecycle::is_shutting_down());
        if let Some(e) = failure {
            crate::tray::report_failure(&app, "gglib is not running", &e);
        }
    });
}

/// Whether a daemon on this machine still holds its lock.
fn lock_is_held() -> bool {
    gglib_core::paths::data_root().is_ok_and(|dir| gglib_axum::DaemonLock::is_held(&dir))
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
                    "port {} was taken by another program during startup",
                    daemon_port()
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
