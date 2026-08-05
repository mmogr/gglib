#![doc = include_str!("README.md")]

pub mod sse;

use std::time::Duration;

use anyhow::{Context, Result, anyhow, bail};
use serde::{Deserialize, Serialize};

use gglib_core::DAEMON_PORT;
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
pub fn base_url() -> String {
    format!("http://127.0.0.1:{DAEMON_PORT}")
}

/// What answered (or didn't) on the daemon port.
#[derive(Debug)]
pub enum DaemonProbe {
    /// A gglib daemon answered with its identity marker.
    Running,
    /// Nothing is listening.
    NotRunning,
    /// Something answered, but it is not a gglib daemon.
    ForeignServer,
}

/// Identity-check the daemon port.
pub async fn probe(client: &reqwest::Client) -> DaemonProbe {
    let url = format!("{}/health", base_url());
    let response = match client.get(&url).timeout(PROBE_TIMEOUT).send().await {
        Ok(r) => r,
        Err(_) => return DaemonProbe::NotRunning,
    };
    match response.json::<serde_json::Value>().await {
        Ok(body) if body.get("service").and_then(|s| s.as_str()) == Some("gglib-daemon") => {
            DaemonProbe::Running
        }
        _ => DaemonProbe::ForeignServer,
    }
}

/// A connected daemon: the shared HTTP client plus the base URL.
pub struct DaemonHandle {
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
pub async fn ensure_daemon() -> Result<DaemonHandle> {
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
pub struct StartProxyBody {
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
pub struct ProxyStatusDto {
    pub running: bool,
    pub port: Option<u16>,
    #[serde(default)]
    pub pinned_model: Option<String>,
}

/// `POST /api/servers/start` response.
#[derive(Debug, Clone, Deserialize)]
pub struct StartServerDto {
    pub port: u16,
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
    pub async fn start_proxy(&self, body: &StartProxyBody) -> Result<ProxyStatusDto> {
        let response = self
            .client
            .post(self.url("/api/proxy/start"))
            .json(body)
            .timeout(Duration::from_secs(30))
            .send()
            .await?;
        Ok(Self::expect_ok(response).await?.json().await?)
    }

    /// Stop the proxy (idempotent on the daemon side).
    pub async fn stop_proxy(&self) -> Result<ProxyStatusDto> {
        let response = self
            .client
            .post(self.url("/api/proxy/stop"))
            .json(&serde_json::json!({}))
            .timeout(Duration::from_secs(30))
            .send()
            .await?;
        Ok(Self::expect_ok(response).await?.json().await?)
    }

    /// Current proxy status.
    pub async fn proxy_status(&self) -> Result<ProxyStatusDto> {
        let response = self
            .client
            .get(self.url("/api/proxy/status"))
            .timeout(Duration::from_secs(5))
            .send()
            .await?;
        Ok(Self::expect_ok(response).await?.json().await?)
    }

    /// Start (or reuse) a llama-server for a model, returning its port.
    ///
    /// Long timeout: the daemon holds the request open while the model loads.
    pub async fn start_model_server(
        &self,
        model_id: i64,
        context_length: Option<u64>,
    ) -> Result<StartServerDto> {
        let response = self
            .client
            .post(self.url("/api/servers/start"))
            .json(&serde_json::json!({ "id": model_id, "context_length": context_length }))
            .timeout(Duration::from_secs(180))
            .send()
            .await?;
        Ok(Self::expect_ok(response).await?.json().await?)
    }

    /// Ask the daemon to shut down. `Ok(true)` when a shutdown was accepted,
    /// `Ok(false)` when the server said it is not running as a daemon.
    pub async fn shutdown_daemon(&self) -> Result<bool> {
        let response = self
            .client
            .post(self.url("/api/daemon/shutdown"))
            .timeout(Duration::from_secs(5))
            .send()
            .await?;
        Ok(response.status() == reqwest::StatusCode::ACCEPTED)
    }
}
