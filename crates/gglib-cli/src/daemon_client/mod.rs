#![doc = include_str!("README.md")]

pub(crate) mod sse;

use std::time::Duration;

use anyhow::{Context, Result, anyhow, bail};
use serde::{Deserialize, Serialize};

use gglib_core::DAEMON_PORT;
use gglib_core::download::QueueSnapshot;
use gglib_core::ports::PinnedSpec;

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
    let mine = gglib_core::debug_switches::build_fingerprint();
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

// ─── Typed calls ─────────────────────────────────────────────────────────────

/// Body for `POST /api/proxy/start` — the daemon-side twin of
/// `gglib_axum::handlers::proxy::StartProxyConfig`.
#[derive(Debug, Clone, Default, Serialize)]
pub(crate) struct StartProxyBody {
    pub host: Option<String>,
    pub port: Option<u16>,
    pub default_context: Option<u64>,
    pub cache: Option<bool>,
    pub slot_dir: Option<std::path::PathBuf>,
    pub pinned: Option<PinnedSpec>,
    pub cache_disk_gb: Option<u64>,
    pub inference_override: Option<gglib_core::domain::InferenceConfig>,
    pub api_key: Option<String>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub allowed_hosts: Vec<String>,
}

/// `GET /api/proxy/status` / start / stop response.
#[derive(Debug, Clone, Deserialize)]
pub(crate) struct ProxyStatusDto {
    pub running: bool,
    pub port: Option<u16>,
    #[serde(default)]
    pub pinned_model: Option<String>,
}

/// `POST /api/servers/start` response.
#[derive(Debug, Clone, Deserialize)]
pub(crate) struct StartServerDto {
    pub port: u16,
}

/// `POST /api/models/downloads/queue` request body.
#[derive(Debug, Clone, Serialize)]
pub(crate) struct QueueDownloadBody {
    pub model_id: String,
    /// `None` leaves the quantization choice to the daemon.
    pub quant: Option<String>,
}

impl DaemonHandle {
    /// One absolute URL on the daemon.
    fn url(&self, path: &str) -> String {
        format!("{}{path}", base_url())
    }

    /// Read an HTTP response, surfacing non-2xx bodies as errors.
    async fn expect_ok(response: reqwest::Response) -> Result<reqwest::Response> {
        let status = response.status();
        if status.is_success() {
            return Ok(response);
        }
        let body = response.text().await.unwrap_or_default();
        // The daemon's error envelope is {"error": "..."} — surface just the
        // message when it parses, the raw body otherwise.
        let message = serde_json::from_str::<serde_json::Value>(&body)
            .ok()
            .and_then(|v| v.get("error").and_then(|e| e.as_str()).map(String::from))
            .unwrap_or(body);
        Err(anyhow!("daemon answered {status}: {message}"))
    }

    /// Start the proxy (idempotent on the daemon side).
    pub(crate) async fn start_proxy(&self, body: &StartProxyBody) -> Result<ProxyStatusDto> {
        let response = self
            .client
            .post(self.url(paths::PROXY_START_PATH))
            .json(body)
            .timeout(Duration::from_secs(30))
            .send()
            .await?;
        Ok(Self::expect_ok(response).await?.json().await?)
    }

    /// Stop the proxy (idempotent on the daemon side).
    pub(crate) async fn stop_proxy(&self) -> Result<ProxyStatusDto> {
        let response = self
            .client
            .post(self.url(paths::PROXY_STOP_PATH))
            .json(&serde_json::json!({}))
            .timeout(Duration::from_secs(30))
            .send()
            .await?;
        Ok(Self::expect_ok(response).await?.json().await?)
    }

    /// Current proxy status.
    pub(crate) async fn proxy_status(&self) -> Result<ProxyStatusDto> {
        let response = self
            .client
            .get(self.url(paths::PROXY_STATUS_PATH))
            .timeout(Duration::from_secs(5))
            .send()
            .await?;
        Ok(Self::expect_ok(response).await?.json().await?)
    }

    /// Start (or reuse) a llama-server for a model, returning its port.
    ///
    /// Long timeout: the daemon holds the request open while the model loads.
    pub(crate) async fn start_model_server(
        &self,
        model_id: i64,
        context_length: Option<u64>,
    ) -> Result<StartServerDto> {
        let response = self
            .client
            .post(self.url(paths::SERVERS_START_PATH))
            .json(&serde_json::json!({ "id": model_id, "context_length": context_length }))
            .timeout(Duration::from_secs(180))
            .send()
            .await?;
        Ok(Self::expect_ok(response).await?.json().await?)
    }

    /// Queue a model download on the daemon.
    ///
    /// Long timeout: the daemon resolves the repo and its shard list against
    /// HuggingFace before answering.
    /// The response body carries a queue position and shard count. Nothing
    /// reads them — the caller goes straight to watching the queue — so this
    /// checks the status and discards the body rather than deserializing a
    /// shape no one inspects.
    pub(crate) async fn queue_download(&self, body: &QueueDownloadBody) -> Result<()> {
        let response = self
            .client
            .post(self.url(paths::DOWNLOADS_QUEUE_PATH))
            .json(body)
            .timeout(Duration::from_secs(30))
            .send()
            .await?;
        Self::expect_ok(response).await?;
        Ok(())
    }

    /// The daemon's download queue snapshot — what the dashboard renders.
    ///
    /// `GET` and `POST` share one path by design. The snapshot handler was once
    /// double-mounted at `/api/models/downloads` as well, and when that second
    /// mount was retired as unused (#834) the CLI was still polling it. The bare
    /// path then fell through to `/api/models/{id}`, whose `i64` extractor
    /// answers `400 text/plain` — which the poller tried to parse as JSON.
    pub(crate) async fn download_queue(&self) -> Result<QueueSnapshot> {
        let response = self
            .client
            .get(self.url(paths::DOWNLOADS_QUEUE_PATH))
            .timeout(Duration::from_secs(5))
            .send()
            .await?;
        Ok(Self::expect_ok(response).await?.json().await?)
    }

    /// Ask the daemon to shut down. `Ok(true)` when a shutdown was accepted,
    /// `Ok(false)` when the server said it is not running as a daemon.
    pub(crate) async fn shutdown_daemon(&self) -> Result<bool> {
        let response = self
            .client
            .post(self.url(paths::DAEMON_SHUTDOWN_PATH))
            .timeout(Duration::from_secs(5))
            .send()
            .await?;
        Ok(response.status() == reqwest::StatusCode::ACCEPTED)
    }
}
